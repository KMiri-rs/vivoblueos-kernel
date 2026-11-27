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
//! qemu_riscv64: the console is an NS16550 UART at the same address and the
//! clock ticks at 10 MHz. The UART registers are plain memory provided by the
//! KMiri machine description.

pub use crate::devices::clock::kmiri_clock::KMiriClock as ClockImpl;
use crate::{arch, devices::clock::kmiri_clock::KMiriClock};

pub const PHYS_DRAM_BASE: usize = 0x8000_0000;
pub const PHYS_DRAM_SIZE: usize = 128 * 1024 * 1024;
const NS16550_UART0_BASE: u32 = 0x1000_0000;

pub(crate) fn init() {
    assert!(!arch::local_irq_enabled());
    crate::boot::init_runtime();
    crate::boot::init_heap();
    <KMiriClock as blueos_hal::clock::Clock>::stop();
}

crate::define_peripheral! {
    (console_uart, blueos_driver::uart::ns16x50::Ns16x50,
     blueos_driver::uart::ns16x50::Ns16x50::new(NS16550_UART0_BASE as _)),
}

crate::define_pin_states!(None);

// On riscv an IPI only wakes a hart sleeping in `wfi`; the KMiri idle loop
// never sleeps, so there is nothing to wake.
#[inline(always)]
pub(crate) fn send_ipi(_hart: usize) {}

#[inline(always)]
pub(crate) fn clear_ipi(_hart: usize) {}
