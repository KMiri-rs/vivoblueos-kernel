//! Single-hart startup model, shared by the kernel and architecture dependency.
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
const MIE: usize = 1 << 3;
static MSTATUS: AtomicUsize = AtomicUsize::new(0);
pub static VECTOR_INSTALLED: AtomicBool = AtomicBool::new(false);
pub fn bootstrap() { MSTATUS.store((3 << 11) | (1 << 7), Ordering::SeqCst); }
pub fn irq_enabled() -> bool { MSTATUS.load(Ordering::SeqCst) & MIE != 0 }
pub fn disable() { MSTATUS.fetch_and(!MIE, Ordering::SeqCst); }
pub fn enable() { MSTATUS.fetch_or(MIE, Ordering::SeqCst); }
pub fn disable_save() -> usize { MSTATUS.fetch_and(!MIE, Ordering::SeqCst) }
pub fn restore(old: usize) { MSTATUS.store(old, Ordering::SeqCst); }
pub extern "C" fn idle() { assert!(irq_enabled(), "idle with IRQ disabled"); }

pub extern "C" fn local_irq_enabled() -> bool {
    crate::miri::irq_enabled()
}

pub extern "C" fn disable_local_irq() {
    crate::miri::disable();
}

pub extern "C" fn enable_local_irq() {
    crate::miri::enable();
}

pub extern "C" fn disable_local_irq_save() -> usize {
    crate::miri::disable_save()
}

pub extern "C" fn enable_local_irq_restore(old: usize) {
    crate::miri::restore(old);
}
