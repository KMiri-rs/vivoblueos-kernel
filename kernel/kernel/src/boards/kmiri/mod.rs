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

//! The board of the KMiri machine (see `arch/kmiri`). It follows
//! qemu_riscv64: the console is an NS16550 UART and the timer and
//! inter-processor interrupts come from a CLINT, at the same addresses, with
//! the same drivers. KMiri models both devices (its machine description is
//! KMiri-rs' `blueos/kmiri.toml`).

pub use crate::devices::clock::riscv_clock::QemuRiscvClock as ClockImpl;
use crate::{arch, drivers::msip::Msip, scheduler, support::SmpStagedInit};
use blueos_hal::clock::Clock;

pub const PHYS_DRAM_BASE: usize = 0x8000_0000;
pub const PHYS_DRAM_SIZE: usize = 128 * 1024 * 1024;
const NS16550_UART0_BASE: u32 = 0x1000_0000;

static STAGING: SmpStagedInit = SmpStagedInit::new();

// As qemu_riscv64, without the PLIC (KMiri does not model one).
pub(crate) fn init() {
    assert!(!arch::local_irq_enabled());
    STAGING.run(0, true, crate::boot::init_runtime);
    STAGING.run(1, true, crate::boot::init_heap);
    STAGING.run(2, false, arch::init_vector_table);
    STAGING.run(3, false, ClockImpl::stop);
    // From now on, all work will be done by core 0.
    if arch::current_cpu_id() != 0 {
        scheduler::wait_and_then_start_schedule();
        unreachable!("Secondary cores should have jumped to the scheduler");
    }
}

crate::define_peripheral! {
    (console_uart, blueos_driver::uart::ns16x50::Ns16x50,
     blueos_driver::uart::ns16x50::Ns16x50::new(NS16550_UART0_BASE as _)),
}

crate::define_pin_states!(None);

type MyMsip = Msip<0x0200_0000>;

#[inline(always)]
pub(crate) fn send_ipi(hart: usize) {
    MyMsip::send_ipi(hart)
}

#[inline(always)]
pub(crate) fn clear_ipi(hart: usize) {
    MyMsip::clear_ipi(hart)
}
