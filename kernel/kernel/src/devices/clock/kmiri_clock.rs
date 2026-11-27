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

//! The clock of the KMiri board.
//!
//! FIXME: this is a stand-in until the interpreter exposes a virtual clock
//! and per-CPU one-shot timers. The counter advances by one on every read,
//! which keeps time monotonic, and no timer interrupt is ever raised.

use blueos_hal::clock::Clock;
use core::sync::atomic::{AtomicU64, Ordering};

pub struct KMiriClock;

static CYCLES: AtomicU64 = AtomicU64::new(0);

impl Clock for KMiriClock {
    fn hz() -> u64 {
        10_000_000
    }

    fn estimate_current_cycles() -> u64 {
        CYCLES.fetch_add(1, Ordering::Relaxed)
    }

    fn interrupt_at(_moment: u64) {}

    fn stop() {}
}
