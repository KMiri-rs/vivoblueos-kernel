use crate::tlsf::TlsfHeap;
use const_default::ConstDefault;
use core::{alloc::Layout, ptr::NonNull};

/// Backing store aligned well enough for `insert_free_block_ptr_aligned`
/// (the free-block header needs 32-byte alignment on 64-bit targets).
#[repr(align(4096))]
struct Pool([u8; 4096]);

/// One aligned backing pool, then an insert/allocate/write/read/deallocate round-trip.
/// A second live allocation is added to check that two live blocks stay disjoint.
#[test]
fn tlsf_insert_alloc_dealloc_roundtrip() {
    let mut storage = Pool([0; 4096]);
    let base = storage.0.as_mut_ptr().cast::<u8>();
    let pool = NonNull::slice_from_raw_parts(NonNull::new(base).unwrap(), 4096);

    let mut heap: TlsfHeap = ConstDefault::DEFAULT;
    unsafe { heap.insert_free_block_ptr_aligned(pool).unwrap() };

    let layout = Layout::from_size_align(64, 16).unwrap();
    let first = heap.allocate(&layout).unwrap();
    let second = heap.allocate(&layout).unwrap();

    // The two live allocations must not overlap.
    let a = first.as_ptr() as usize;
    let b = second.as_ptr() as usize;
    assert!(
        a + 64 <= b || b + 64 <= a,
        "allocations overlap: {a:#x} {b:#x}"
    );

    unsafe {
        for i in 0..64 {
            first.as_ptr().add(i).write(i as u8);
            second.as_ptr().add(i).write((255 - i) as u8);
        }
        for i in 0..64 {
            assert_eq!(first.as_ptr().add(i).read(), i as u8);
            assert_eq!(second.as_ptr().add(i).read(), (255 - i) as u8);
        }
        heap.deallocate(first, layout.align());
        heap.deallocate(second, layout.align());
    }
}
