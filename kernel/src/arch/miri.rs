//! Miri startup adapter: no hardware interrupts or context switches.
use crate::scheduler::ContextSwitchHookHolder;
pub(crate) use arch_crate::{
    bootstrap, disable_local_irq, disable_local_irq_save, enable_local_irq,
    enable_local_irq_restore, idle, local_irq_enabled,
};
// These files contain only Rust layout/types and context preparation.
#[path = "riscv/context.rs"]
mod context;
pub(crate) use context::*;
#[path = "riscv/irq.rs"]
pub(crate) mod irq;

pub extern "C" fn current_sp() -> usize {
    panic!("Miri: reading the hardware stack pointer is unsupported");
}

pub(crate) extern "C" fn switch_context_with_hook(_hook: *mut ContextSwitchHookHolder) {
    panic!("Miri: context switching is outside the idle-only experiment");
}

impl Context {
    pub(crate) fn __global_pointer() -> usize {
        0 // GP is stored but never executed in this no-switch model.
    }
}

pub(crate) extern "C" fn start_schedule(cont: extern "C" fn() -> !) {
    let current = crate::scheduler::current_thread_ref();
    current.reset_saved_sp();
    assert_ne!(current.saved_sp(), 0);
    blueos_infra::miri_println!("BLUEOS_START_SCHEDULE_NO_SP_SWITCH");
    cont();
}

pub(crate) extern "C" fn current_cpu_id() -> usize {
    0
}

pub(crate) extern "C" fn pend_switch_context() {
    assert!(
        !crate::irq::is_in_irq(),
        "Miri: hardware interrupts are unsupported"
    );
    crate::scheduler::relinquish_me();
}

pub(crate) extern "C" fn switch_stack(
    _to_sp: usize,
    _cont: extern "C" fn(sp: usize, old_sp: usize),
) -> ! {
    panic!("Miri: stack switching is outside the idle-only experiment");
}

pub(crate) extern "C" fn send_ipi(_hart: usize) {
    panic!("Miri: inter-processor interrupts are unsupported");
}
