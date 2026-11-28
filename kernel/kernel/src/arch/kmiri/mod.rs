// Copyright (c) 2025 vivo Mobile Communication Co., Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//       http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! The architecture layer for running the kernel inside the KMiri
//! interpreter. The kernel is compiled for riscv64, but every operation that
//! native code performs with inline assembly (interrupt enable bit, hart id,
//! stack pointer, context switch) is mapped to a KMiri machine primitive.
//! Nothing outside this module and `boards/kmiri` knows about KMiri.

pub(crate) mod irq;

use crate::{
    boards, irq as sysirq, scheduler, scheduler::ContextSwitchHookHolder, thread, thread::Thread,
    types::Arc,
};
use core::{
    cell::Cell,
    sync::atomic::{compiler_fence, AtomicUsize, Ordering},
};

const NUM_CORES: usize = blueos_kconfig::CONFIG_NUM_CORES as usize;

unsafe extern "Rust" {
    fn kern_miri_irq_is_enabled() -> bool;
    fn kern_miri_irq_disable();
    fn kern_miri_irq_enable();
    fn kern_miri_current_cpu() -> usize;
    fn miri_create_new_thread(
        func: fn(usize),
        arg: usize,
        stack_end: usize,
        stack_size: usize,
    ) -> usize;
    fn miri_switch_to(thread: usize);
    fn kern_miri_jump(pc: usize, regs: *const usize, nregs: usize) -> !;
    fn kern_miri_register_irq_entry(entry: extern "Rust" fn(u8));
    fn kern_miri_wait_for_interrupt();
    fn kern_miri_start_cpu(
        cpu: usize,
        entry: extern "Rust" fn(usize),
        arg: usize,
        stack_end: usize,
        stack_size: usize,
    );
}

// Same as riscv: a per-hart flag consumed on the way out of an interrupt.
static mut PENDING_SWITCH_CONTEXT: [Cell<bool>; NUM_CORES] =
    [const { Cell::new(false) }; NUM_CORES];

#[inline]
pub(crate) extern "C" fn pend_switch_context() {
    if !sysirq::is_in_irq() {
        scheduler::relinquish_me();
        return;
    }
    let level = disable_local_irq_save();
    let id = current_cpu_id();
    unsafe { PENDING_SWITCH_CONTEXT[id].set(true) };
    enable_local_irq_restore(level);
}

#[inline]
pub(crate) extern "C" fn claim_switch_context() -> bool {
    let level = disable_local_irq_save();
    let id = current_cpu_id();
    let ok = unsafe { PENDING_SWITCH_CONTEXT[id].get() };
    unsafe { PENDING_SWITCH_CONTEXT[id].set(false) };
    enable_local_irq_restore(level);
    ok
}

#[inline]
pub(crate) extern "C" fn local_irq_enabled() -> bool {
    unsafe { kern_miri_irq_is_enabled() }
}

#[inline]
pub(crate) extern "C" fn disable_local_irq() {
    compiler_fence(Ordering::SeqCst);
    unsafe { kern_miri_irq_disable() };
}

#[inline]
pub(crate) extern "C" fn enable_local_irq() {
    unsafe { kern_miri_irq_enable() };
    compiler_fence(Ordering::SeqCst);
}

// riscv saves the whole mstatus; only its MIE bit is restored in effect, so
// the saved level here is that bit.
#[inline]
pub(crate) extern "C" fn disable_local_irq_save() -> usize {
    compiler_fence(Ordering::SeqCst);
    let old = local_irq_enabled();
    unsafe { kern_miri_irq_disable() };
    old as usize
}

#[inline]
pub(crate) extern "C" fn enable_local_irq_restore(old: usize) {
    if old != 0 {
        unsafe { kern_miri_irq_enable() };
    } else {
        unsafe { kern_miri_irq_disable() };
    }
    compiler_fence(Ordering::SeqCst);
}

// riscv waits for an interrupt with `wfi`, which leaves the interrupt enable
// bit alone.
#[inline]
pub(crate) extern "C" fn idle() {
    unsafe { kern_miri_wait_for_interrupt() };
}

// The address of a local of the caller's frame, i.e. a value inside the
// current stack, as `mv {}, sp` would return.
#[inline(never)]
pub extern "C" fn current_sp() -> usize {
    let marker = 0u8;
    core::ptr::addr_of!(marker).addr()
}

#[inline(always)]
pub(crate) extern "C" fn current_cpu_id() -> usize {
    unsafe { kern_miri_current_cpu() }
}

pub(crate) extern "C" fn send_ipi(hart: usize) {
    boards::send_ipi(hart);
}

// Each kernel thread runs on an interpreter thread of its own. The saved
// context stays the riscv one, on the thread's stack at `saved_sp`; its `tp`
// holds the interpreter thread that resumes it (0: the thread never ran), and
// `mepc` the address to continue at.

// mstatus.MPIE of each hart: the interrupt enable bit a trap sets aside and
// `mret` puts back. It belongs to the hart, not to a thread.
static mut SAVED_IRQ_ENABLE: [Cell<bool>; NUM_CORES] = [const { Cell::new(false) }; NUM_CORES];

// The switch in flight on each hart, `(hook, old_sp)`: left by the thread
// switching away for the thread switched to, which runs the hook as native code
// runs it on the next thread's stack.
static mut SWITCH_IN_FLIGHT: [Cell<(*mut ContextSwitchHookHolder, usize)>; NUM_CORES] =
    [const { Cell::new((core::ptr::null_mut(), 0)) }; NUM_CORES];

// The interpreter thread running on each hart.
static mut RUNNING_THREAD: [Cell<usize>; NUM_CORES] = [const { Cell::new(0) }; NUM_CORES];

// The return address of a context saved by `switch_context_with_hook`: the
// thread resumes inside that call. Only the address is used.
extern "C" fn resume_in_switch() {
    unreachable!("kmiri: a saved switch context is resumed, not called");
}

// Rust does not promise that two casts of the same function give the same
// address, and KMiri does not either; the resume address is taken once, as
// native code uses the one address of a label.
static RESUME_IN_SWITCH_PC: AtomicUsize = AtomicUsize::new(0);

fn resume_in_switch_pc() -> usize {
    let pc = RESUME_IN_SWITCH_PC.load(Ordering::Relaxed);
    if pc != 0 {
        return pc;
    }
    let pc = resume_in_switch as *const () as usize;
    match RESUME_IN_SWITCH_PC.compare_exchange(0, pc, Ordering::Relaxed, Ordering::Relaxed) {
        Ok(_) => pc,
        Err(first) => first,
    }
}

// Taking the trap: MPIE = MIE, MIE = 0, then `trap_entry` enters the IRQ level.
fn trap_enter() {
    compiler_fence(Ordering::SeqCst);
    let id = current_cpu_id();
    unsafe { SAVED_IRQ_ENABLE[id].set(local_irq_enabled()) };
    unsafe { kern_miri_irq_disable() };
    sysirq::enter_irq();
}

// Leaving it: `trap_entry` leaves the IRQ level, then `mret` sets MIE = MPIE.
fn trap_return() {
    sysirq::leave_irq();
    let id = current_cpu_id();
    if unsafe { SAVED_IRQ_ENABLE[id].get() } {
        unsafe { kern_miri_irq_enable() };
    } else {
        unsafe { kern_miri_irq_disable() };
    }
    compiler_fence(Ordering::SeqCst);
}

// Runs on the thread switched to: the hook, as `handle_switch`, then the trap
// return. Returns the context to restore.
fn finish_switch() -> usize {
    let id = current_cpu_id();
    let (hook, old_sp) = unsafe { SWITCH_IN_FLIGHT[id].get() };
    let sp = scheduler::save_context_finish_hook(unsafe { &mut *hook }, old_sp);
    trap_return();
    sp
}

// Entry of the interpreter thread of a kernel thread that never ran: finishes
// the switch to it, then continues at the entry of its initial context.
fn start_thread(ctx_addr: usize) {
    let sp = finish_switch();
    debug_assert_eq!(sp, ctx_addr);
    let ctx = unsafe { &*core::ptr::with_exposed_provenance::<Context>(sp) };
    let regs = [
        ctx.a0, ctx.a1, ctx.a2, ctx.a3, ctx.a4, ctx.a5, ctx.a6, ctx.a7,
    ];
    unsafe { kern_miri_jump(ctx.mepc, regs.as_ptr(), regs.len()) }
}

// The interrupt causes that riscv's `handle_trap` dispatches on, `mcause`
// without its interrupt bit. KMiri's CLINT delivers them as these vectors.
const MSI: u8 = 3;
#[cfg(has_mtime)]
const TIMER_INT: u8 = 7;

// riscv points `mtvec` at `trap_entry`; KMiri calls the registered entry with
// the cause of the interrupt. Every hart runs this and natively writes the same
// `trap_entry` into its own `mtvec`; KMiri's entry serves all CPUs and is
// registered once, so only hart 0 registers it.
pub(crate) fn init_vector_table() {
    if current_cpu_id() == 0 {
        unsafe { kern_miri_register_irq_entry(irq_entry) };
    }
}

// `trap_entry` and `handle_trap` for an interrupt. Taking it set MPIE = MIE,
// which was 1 or the interrupt would not have been taken, and MIE = 0, which
// KMiri did.
extern "Rust" fn irq_entry(vector: u8) {
    debug_assert!(!local_irq_enabled());
    let id = current_cpu_id();
    unsafe { SAVED_IRQ_ENABLE[id].set(true) };
    sysirq::enter_irq();
    let switched = match vector {
        #[cfg(has_mtime)]
        TIMER_INT => {
            crate::time::handle_clock_interrupt();
            might_switch_context()
        }
        // For waking up from wfi.
        MSI => {
            boards::clear_ipi(id);
            false
        }
        _ => unreachable!("kmiri: unexpected interrupt {vector}"),
    };
    // A thread that switched away ran the trap return when it was switched
    // back to (`finish_switch`).
    if !switched {
        trap_return();
    }
}

// riscv's `might_switch_context`: on the way out of an interrupt, gives the
// CPU to a ready thread of strictly higher priority if a switch was asked for.
// Returns whether it switched.
fn might_switch_context() -> bool {
    if !claim_switch_context() {
        return false;
    }
    let old = scheduler::current_thread_ref();
    if old.preempt_count() != 0 {
        return false;
    }
    let Some(next) =
        scheduler::next_preferred_thread(old.priority(), scheduler::PreferMode::StrictHigher)
    else {
        return false;
    };
    debug_assert_eq!(next.state(), thread::READY);
    debug_assert_ne!(Thread::id(old), Thread::id(&next));
    let mut hook = ContextSwitchHookHolder::new(next);
    if Thread::id(old) == Thread::id(scheduler::current_idle_thread_ref()) {
        let res = old.transfer_state(thread::RUNNING, thread::READY);
        debug_assert_eq!(res, Ok(()));
    } else {
        let res = scheduler::queue_ready_thread(thread::RUNNING, unsafe { Arc::clone_from(old) });
        debug_assert_eq!(res, Ok(()));
    };
    switch_in_trap(&mut hook);
    true
}

// riscv traps with `ecall`, saves the context on the current stack, moves to
// the next thread's stack, runs the hook there and restores the next context.
pub(crate) extern "C" fn switch_context_with_hook(hook: *mut ContextSwitchHookHolder) {
    trap_enter();
    switch_in_trap(hook);
}

// The switch inside a trap: saves the context, moves to the next thread, and
// once switched back runs the hook left for this thread and the trap return.
fn switch_in_trap(hook: *mut ContextSwitchHookHolder) {
    let id = current_cpu_id();
    let mut ctx = Context::default();
    ctx.set_return_address(resume_in_switch_pc());
    ctx.tp = unsafe { RUNNING_THREAD[id].get() };
    let old_sp = (&raw mut ctx).expose_provenance();
    let next = unsafe { (*hook).next_thread() };
    let next_sp = scheduler::spin_until_ready_to_run(next);
    unsafe { SWITCH_IN_FLIGHT[id].set((hook, old_sp)) };
    let next_ctx = unsafe { &*core::ptr::with_exposed_provenance::<Context>(next_sp) };
    let thread = if next_ctx.tp == 0 {
        // Below its initial context, the rest of the stack is the thread's.
        let stack_size = next_sp - next.stack_base();
        unsafe { miri_create_new_thread(start_thread, next_sp, next_sp, stack_size) }
    } else if next_ctx.mepc == resume_in_switch_pc() {
        next_ctx.tp
    } else {
        unimplemented!(
            "kmiri: resuming a context at a changed return address {:#x}",
            next_ctx.mepc
        );
    };
    unsafe { RUNNING_THREAD[id].set(thread) };
    unsafe { miri_switch_to(thread) };
    // Switched back to: the thread that switched here left its hook to run.
    let sp = finish_switch();
    debug_assert_eq!(sp, old_sp);
    core::hint::black_box(&mut ctx);
}

#[allow(clippy::empty_loop)]
pub(crate) extern "C" fn restore_context_with_hook(hook: *mut ContextSwitchHookHolder) -> ! {
    switch_context_with_hook(hook);
    unreachable!("Should have switched to another thread");
}

// Continues at `cont` with no return address, as `jalr x0` after `li ra, 0`.
fn enter_schedule(cont: usize) {
    unsafe { kern_miri_jump(cont, core::ptr::null(), 0) }
}

// riscv moves to the top of the current (idle) thread's stack and jumps to
// `cont`. The boot context is never resumed.
pub(crate) extern "C" fn start_schedule(cont: extern "C" fn() -> !) {
    let current = scheduler::current_thread_ref();
    current.reset_saved_sp();
    let sp = current.saved_sp();
    let stack_size = sp - current.stack_base();
    let thread = unsafe { miri_create_new_thread(enter_schedule, cont as usize, sp, stack_size) };
    unsafe { RUNNING_THREAD[current_cpu_id()].set(thread) };
    unsafe { miri_switch_to(thread) };
}

pub(crate) extern "C" fn switch_stack(
    _to_sp: usize,
    _cont: extern "C" fn(sp: usize, old_sp: usize),
) -> ! {
    unimplemented!("kmiri: switch_stack");
}

// The saved register file of a thread, laid out like riscv's so that the
// thread code that builds it is unchanged.
#[repr(C, align(16))]
#[derive(Default, Debug)]
pub(crate) struct Context {
    pub ra: usize,
    pub mepc: usize,
    pub gp: usize,
    pub tp: usize,
    pub t0: usize,
    pub t1: usize,
    pub t2: usize,
    pub fp: usize,
    pub a0: usize,
    pub a1: usize,
    pub a2: usize,
    pub a3: usize,
    pub a4: usize,
    pub a5: usize,
    pub a6: usize,
    pub a7: usize,
    pub t3: usize,
    pub t4: usize,
    pub t5: usize,
    pub t6: usize,
    pub s1: usize,
    pub s2: usize,
    pub s3: usize,
    pub s4: usize,
    pub s5: usize,
    pub s6: usize,
    pub s7: usize,
    pub s8: usize,
    pub s9: usize,
    pub s10: usize,
    pub s11: usize,
    pub padding: usize,
}

#[repr(C, align(16))]
#[derive(Default, Debug)]
pub(crate) struct IsrContext {
    pub mstatus: usize,
    pub mcause: usize,
    pub mtval: usize,
    pub mepc: usize,
}

impl Context {
    #[inline]
    pub(crate) fn init(&mut self) -> &mut Self {
        self
    }

    #[inline(never)]
    pub(crate) fn set_return_address(&mut self, pc: usize) -> &mut Self {
        self.mepc = pc;
        self
    }

    #[inline(never)]
    pub(crate) fn set_arg(&mut self, index: usize, val: usize) -> &mut Self {
        match index {
            0 => self.a0 = val,
            1 => self.a1 = val,
            2 => self.a2 = val,
            3 => self.a3 = val,
            4 => self.a4 = val,
            5 => self.a5 = val,
            6 => self.a6 = val,
            7 => self.a7 = val,
            _ => {}
        }
        self
    }
}

// The interpreter enters the program at `miri_start` instead of `_start`, so
// the bootstrap defines it and continues with `$cont`. The naked `_start` is
// kept for the symbol but never runs.
//
// On qemu_riscv64 every hart leaves reset at `_start` and continues with
// `$cont` on its own boot stack, `BOOT_STACK_SIZE` bytes below
// `$stack_end - mhartid * BOOT_STACK_SIZE`. KMiri runs the entry on CPU 0
// only, so CPU 0 starts the other harts there first, before it touches any
// shared state (starting a CPU orders only what came before it).
#[macro_export]
macro_rules! arch_bootstrap {
    ($stack_start:path, $stack_end:path, $cont:path) => {
        #[unsafe(no_mangle)]
        fn miri_start(_argc: isize, _argv: *const *const u8) -> isize {
            extern "Rust" fn secondary_hart(_: usize) {
                $cont();
            }
            let stack_end = core::ptr::addr_of!($stack_end) as usize;
            $crate::arch::kmiri::start_secondary_harts(stack_end, secondary_hart);
            $cont();
            0
        }
        core::arch::naked_asm!("unimp")
    };
}

/// Per-hart boot stack size, as on riscv.
pub const BOOT_STACK_SIZE: usize = 0x1000;

pub fn start_secondary_harts(stack_end: usize, entry: extern "Rust" fn(usize)) {
    for hart in 1..NUM_CORES {
        let top = stack_end - hart * BOOT_STACK_SIZE;
        unsafe { kern_miri_start_cpu(hart, entry, hart, top, BOOT_STACK_SIZE) };
    }
}
