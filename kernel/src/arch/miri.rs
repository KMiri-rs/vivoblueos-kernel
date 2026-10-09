//! Miri startup adapter: no hardware interrupts or context switches.
use crate::scheduler::ContextSwitchHookHolder;

pub(crate) extern "C" fn local_irq_enabled() -> bool {
    arch_crate::miri::irq_enabled()
}

pub(crate) extern "C" fn disable_local_irq() {
    arch_crate::miri::disable();
}

pub(crate) extern "C" fn enable_local_irq() {
    arch_crate::miri::enable();
}

pub(crate) extern "C" fn idle() {
    arch_crate::miri::idle();
}

pub(crate) extern "C" fn disable_local_irq_save() -> usize {
    arch_crate::miri::disable_save()
}

pub(crate) extern "C" fn enable_local_irq_restore(old: usize) {
    arch_crate::miri::restore(old);
}

pub extern "C" fn current_sp() -> usize {
    panic!("Miri: reading the hardware stack pointer is unsupported");
}

pub(crate) extern "C" fn ecall_switch_context_with_hook(_hook: *mut ContextSwitchHookHolder) {
    panic!("Miri: context switching is outside the idle-only experiment");
}

impl super::riscv::Context {
    pub(crate) fn __global_pointer() -> usize {
        0 // GP is stored but never executed in this no-switch model.
    }
}

pub(crate) extern "C" fn bootstrap() {
    arch_crate::miri::bootstrap();
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
