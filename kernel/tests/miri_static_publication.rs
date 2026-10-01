#![no_std]
#![no_main]
use blueos_infra::tinyarc::{TinyArc, TinyArcInner};
use core::mem::MaybeUninit;
struct NoGlobalAllocation;
unsafe impl core::alloc::GlobalAlloc for NoGlobalAllocation {
    unsafe fn alloc(&self, _: core::alloc::Layout) -> *mut u8 { panic!("unexpected allocation") }
    unsafe fn dealloc(&self, _: *mut u8, _: core::alloc::Layout) { panic!("unexpected deallocation") }
}
#[global_allocator]
static GLOBAL: NoGlobalAllocation = NoGlobalAllocation;
unsafe extern "Rust" { fn miri_write_to_stdout(bytes: &[u8]); }
unsafe extern "C" { fn abort() -> !; }
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! { unsafe { abort() } }
static mut SLOT: MaybeUninit<TinyArcInner<u64>> = MaybeUninit::uninit();
#[unsafe(no_mangle)]
fn miri_start(_: isize, _: *const *const u8) -> isize {
    unsafe {
        let slot = (&raw mut SLOT).cast::<TinyArcInner<u64>>();
        slot.write(TinyArcInner::new(17));
        #[cfg(before_fix)]
        let mut arc = TinyArc::from_static_inner_ref(&*slot);
        #[cfg(not(before_fix))]
        let mut arc = TinyArc::from_inner(core::ptr::NonNull::new_unchecked(slot));
        *TinyArc::get_mut_unchecked(&mut arc) = 42;
        assert_eq!(*arc, 42);
        core::mem::forget(arc); // static storage must not be passed to Box::drop
        miri_write_to_stdout(b"STATIC_ARC_MUTABLE_PUBLICATION_OK\n");
    }
    0
}
