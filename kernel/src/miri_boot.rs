//! Experimental startup adapter: no applications, interrupts or context switches.
use core::{alloc::{GlobalAlloc, Layout}, sync::atomic::Ordering};
pub(crate) const HEAP_SIZE: usize = 8 * 1024 * 1024;
#[global_allocator]
static GLOBAL: crate::allocator::KernelAllocator = crate::allocator::KernelAllocator;
unsafe extern "C" {
    #[link_name = "exit"] fn interpreter_exit(code: i32) -> !;
    fn abort() -> !;
}
#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    blueos_infra::miri_println!("BLUEOS_MIRI_PANIC: {}", info);
    unsafe { abort() }
}
#[alloc_error_handler]
fn oom(layout: Layout) -> ! { panic!("kernel allocation failed: {:?}", layout); }
pub(crate) unsafe fn heap_start() -> *mut u8 {
    0x03000000 as *mut u8 // boot.toml supplies an actual, aligned 8 MiB allocation
}
pub(crate) fn verify_heap() {
    unsafe {
        let layout = Layout::from_size_align(64, 16).unwrap();
        let p = GLOBAL.alloc(layout);
        assert!(!p.is_null());
        assert_eq!(p as usize % 16, 0);
        for i in 0..64 { p.add(i).write(i as u8); }
        for i in 0..64 { assert_eq!(p.add(i).read(), i as u8); }
        GLOBAL.dealloc(p, layout);
    }
    blueos_infra::miri_println!("BLUEOS_HEAP_READY");
}
pub(crate) fn console_ready() {
    assert!(arch_crate::miri::VECTOR_INSTALLED.load(Ordering::SeqCst));
    assert!(crate::devices::DeviceManager::get().get_char_device("ttyS0").is_some());
    let _console = crate::devices::console::get_console();
    unsafe {
        assert_eq!((0x10000005 as *const u8).read_volatile(), 0x60);
        assert_eq!((0x0c000028 as *const u32).read_volatile(), 1);
        assert_ne!((0x0c002000 as *const u32).read_volatile() & (1 << 10), 0);
    }
    blueos_infra::miri_println!("BLUEOS_CONSOLE_READY");
}
pub(crate) fn scheduler_ready() {
    assert_eq!(crate::thread::Thread::id(crate::scheduler::current_thread_ref()),
               crate::thread::Thread::id(crate::scheduler::current_idle_thread_ref()));
    assert_eq!(crate::scheduler::current_thread_ref().state(), crate::thread::RUNNING);
    blueos_infra::miri_println!("BLUEOS_SCHEDULER_READY");
}
#[inline(never)]
pub(crate) fn calibrate() {
    let byte = if cfg!(blueos_miri_invalid_bool) { 2u8 } else { 1u8 };
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
    unsafe {
        // Real backing allocations for the modeled MMIO register ranges are in boot.toml.
        let uart = 0x10000000 as *mut u8;
        uart.write_volatile(0);
        for i in 1..7 { uart.add(i).write_volatile(0); }
        uart.add(5).write_volatile(0x60);
        let plic = 0x0c000000 as *mut u8;
        plic.cast::<u32>().write_volatile(0);
        for offset in [0x28, 0x2000, 0x200000, 0x200004] {
            plic.add(offset).cast::<u32>().write_volatile(0);
        }
        let clint = 0x02000000 as *mut u8;
        clint.cast::<u64>().write_volatile(0);
        clint.add(0xbff8).cast::<u64>().write_volatile(0);
        clint.add(0x4000).cast::<u64>().write_volatile(u64::MAX);
    }
    crate::arch::bootstrap();
    assert!(!crate::arch::local_irq_enabled());
    crate::boot::init();
    panic!("boot unexpectedly returned");
}
