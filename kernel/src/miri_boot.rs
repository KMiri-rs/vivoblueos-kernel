//! Experimental startup adapter: no applications, interrupts or context switches.
use core::{
    alloc::{GlobalAlloc, Layout},
    sync::atomic::Ordering,
};
pub(crate) const HEAP_SIZE: usize = crate::boards::KERNEL_HEAP_SIZE;
#[cfg_attr(not(test), global_allocator)]
static GLOBAL: crate::allocator::KernelAllocator = crate::allocator::KernelAllocator;
unsafe extern "C" {
    #[link_name = "exit"]
    fn interpreter_exit(code: i32) -> !;
    fn abort() -> !;
}
#[cfg(not(test))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    blueos_infra::miri_println!("BLUEOS_MIRI_PANIC: {}", info);
    unsafe { abort() }
}
#[cfg(not(test))]
#[alloc_error_handler]
fn oom(layout: Layout) -> ! {
    panic!("kernel allocation failed: {:?}", layout);
}
pub(crate) unsafe fn heap_start() -> *mut u8 {
    // Same physical address as link.x `__heap_start` and boot.toml HeapBacking.
    crate::boards::KERNEL_HEAP_PHYS as *mut u8
}
pub(crate) fn verify_heap() {
    unsafe {
        let layout = Layout::from_size_align(64, 16).unwrap();
        let p = GLOBAL.alloc(layout);
        assert!(!p.is_null());
        assert_eq!(p as usize % 16, 0);
        for i in 0..64 {
            p.add(i).write(i as u8);
        }
        for i in 0..64 {
            assert_eq!(p.add(i).read(), i as u8);
        }
        GLOBAL.dealloc(p, layout);
    }
    blueos_infra::miri_println!("BLUEOS_HEAP_READY");
}
pub(crate) fn console_ready() {
    assert!(crate::arch::VECTOR_INSTALLED.load(Ordering::SeqCst));
    assert!(crate::devices::DeviceManager::get()
        .get_char_device("ttyS0")
        .is_some());
    let _console = crate::devices::console::get_console();
    blueos_infra::miri_println!("BLUEOS_CONSOLE_READY");
}
pub(crate) fn scheduler_ready() {
    assert_eq!(
        crate::thread::Thread::id(crate::scheduler::current_thread_ref()),
        crate::thread::Thread::id(crate::scheduler::current_idle_thread_ref())
    );
    assert_eq!(
        crate::scheduler::current_thread_ref().state(),
        crate::thread::RUNNING
    );
    blueos_infra::miri_println!("BLUEOS_SCHEDULER_READY");
}
#[inline(never)]
pub(crate) fn calibrate() {
    let byte = if cfg!(blueos_miri_invalid_bool) {
        2u8
    } else {
        1u8
    };
    let value = unsafe { core::ptr::read((&raw const byte).cast::<bool>()) };
    assert!(value);
}
pub(crate) fn done() -> ! {
    unsafe {
        assert!(crate::boot::INIT_BSS_DONE);
        assert!(crate::boot::INIT_HEAP_DONE);
        assert!(crate::boot::INIT_ARRAY_DONE);
    }
    blueos_infra::miri_println!("BLUEOS_MIRI_DONE");
    // The boot singleton objects remain live; this does not validate absence of leaks.
    unsafe { interpreter_exit(0) }
}
#[unsafe(no_mangle)]
fn miri_start(_: isize, _: *const *const u8) -> isize {
    assert_eq!(blueos_kconfig::CONFIG_NUM_CORES, 1);
    blueos_infra::miri_println!("BLUEOS_MIRI_ENTRY");
    crate::arch::bootstrap();
    assert!(!crate::arch::local_irq_enabled());
    crate::boot::init();
    panic!("boot unexpectedly returned");
}
