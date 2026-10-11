// Copyright (c) 2026 vivo Mobile Communication Co., Ltd.
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

mod config;
use crate::{scheduler, support::SmpStagedInit};
use blueos_hal::clock::Clock;
pub use config::{PHYS_DRAM_BASE, PHYS_DRAM_SIZE};
use core::sync::atomic::{AtomicU64, Ordering};

pub struct ClockImpl;

static CLOCK_CYCLES: AtomicU64 = AtomicU64::new(0);

impl Clock for ClockImpl {
    fn hz() -> u64 {
        1_000_000
    }

    fn estimate_current_cycles() -> u64 {
        CLOCK_CYCLES.fetch_add(1, Ordering::Relaxed)
    }

    fn interrupt_at(_moment: u64) {}

    fn stop() {}
}

static STAGING: SmpStagedInit = SmpStagedInit::new();

pub(crate) fn init() {
    STAGING.run(0, true, crate::boot::init_runtime);
    STAGING.run(1, true, crate::boot::init_heap);

    // Keep the same single-hart behavior as miri arch backend.
    if crate::arch::current_cpu_id() != 0 {
        scheduler::wait_and_then_start_schedule();
        unreachable!("Secondary cores should have jumped to the scheduler");
    }
}

crate::define_peripheral! {
    (console_uart, blueos_driver::uart::ns16x50::Ns16x50,
     blueos_driver::uart::ns16x50::Ns16x50::new(
        config::NS16550_UART0_BASE as _,
    )),
}

crate::define_pin_states!(None);

#[inline(always)]
pub(crate) fn send_ipi(_hart: usize) {
    // Miri board does not model hardware IPIs.
}

#[inline(always)]
pub(crate) fn clear_ipi(_hart: usize) {
    // Miri board does not model hardware IPIs.
}
