//! Diagnostic reproduction, not the BlueOS startup acceptance entry.
#![no_std]
#![no_main]
use core::{alloc::{GlobalAlloc, Layout}, ptr::NonNull};
use const_default::ConstDefault;
use allocator_crate::tlsf::TlsfHeap;
unsafe extern "Rust" { fn miri_write_to_stdout(bytes: &[u8]); }
unsafe extern "C" { fn abort() -> !; }
struct NoGlobalAllocation;
unsafe impl GlobalAlloc for NoGlobalAllocation {
    unsafe fn alloc(&self, _: Layout) -> *mut u8 { panic!("unexpected global allocation") }
    unsafe fn dealloc(&self, _: *mut u8, _: Layout) { panic!("unexpected global deallocation") }
}
#[global_allocator]
static GLOBAL: NoGlobalAllocation = NoGlobalAllocation;
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! { unsafe { abort() } }
#[repr(align(4096))]
struct Pool([u8; 4096]);
#[unsafe(no_mangle)]
fn miri_start(_: isize, _: *const *const u8) -> isize {
    let mut storage = Pool([0; 4096]);
    let base = (&raw mut storage.0).cast::<u8>();
    let pool = NonNull::slice_from_raw_parts(NonNull::new(base).unwrap(), 4096);
    let mut heap: TlsfHeap = ConstDefault::DEFAULT;
    unsafe { miri_write_to_stdout(b"TLSF_REPRO_ENTER_NO_MMIO\n"); }
    unsafe { heap.insert_free_block_ptr_aligned(pool).unwrap(); }
    unsafe { miri_write_to_stdout(b"TLSF_REPRO_INIT_OK\n"); }
    let layout = Layout::from_size_align(64,16).unwrap();
    let allocation = heap.allocate(&layout).unwrap();
    unsafe {
        for i in 0..64 { allocation.as_ptr().add(i).write(i as u8); }
        for i in 0..64 { assert_eq!(allocation.as_ptr().add(i).read(), i as u8); }
        heap.deallocate(allocation,layout.align());
        miri_write_to_stdout(b"TLSF_REPRO_DONE\n");
    }
    0
}
