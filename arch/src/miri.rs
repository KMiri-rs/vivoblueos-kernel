//! Single-hart startup model, shared by the kernel and architecture dependency.
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
const MIE: usize = 1 << 3;
static MSTATUS: AtomicUsize = AtomicUsize::new(0);
pub static VECTOR_INSTALLED: AtomicBool = AtomicBool::new(false);
pub extern "C" fn bootstrap() {
    MSTATUS.store((3 << 11) | (1 << 7), Ordering::SeqCst);
}
pub extern "C" fn idle() {
    assert!(local_irq_enabled(), "idle with IRQ disabled");
}

pub extern "C" fn local_irq_enabled() -> bool {
    MSTATUS.load(Ordering::SeqCst) & MIE != 0
}

pub extern "C" fn disable_local_irq() {
    MSTATUS.fetch_and(!MIE, Ordering::SeqCst);
}

pub extern "C" fn enable_local_irq() {
    MSTATUS.fetch_or(MIE, Ordering::SeqCst);
}

pub extern "C" fn disable_local_irq_save() -> usize {
    MSTATUS.fetch_and(!MIE, Ordering::SeqCst)
}

pub extern "C" fn enable_local_irq_restore(old: usize) {
    MSTATUS.store(old, Ordering::SeqCst);
}
