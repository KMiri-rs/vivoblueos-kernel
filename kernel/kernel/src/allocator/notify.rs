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

//! Tells the KMiri interpreter which blocks the kernel allocator hands out and
//! takes back, so that it checks every heap access against the bounds and the
//! lifetime of its block. The allocator itself runs unchanged and keeps
//! deciding where blocks go. In a native build these functions only return
//! their pointer argument.

#[cfg(not(miri))]
mod imp {
    use core::alloc::Layout;

    #[inline(always)]
    pub(in crate::allocator) fn born(ptr: *mut u8, _layout: Layout) -> *mut u8 {
        ptr
    }

    #[inline(always)]
    pub(in crate::allocator) fn died(ptr: *mut u8) -> *mut u8 {
        ptr
    }

    #[inline(always)]
    pub(in crate::allocator) fn realloc_begin(ptr: *mut u8) -> *mut u8 {
        ptr
    }

    #[inline(always)]
    pub(in crate::allocator) fn realloc_end(
        _old: *mut u8,
        new: *mut u8,
        _size: usize,
        _align: usize,
    ) -> *mut u8 {
        new
    }
}

#[cfg(miri)]
mod imp {
    use core::alloc::Layout;

    unsafe extern "Rust" {
        fn kern_miri_object_born(ptr: *mut u8, size: usize, align: usize) -> *mut u8;
        fn kern_miri_object_died(ptr: *mut u8) -> *mut u8;
        fn kern_miri_object_realloc_begin(ptr: *mut u8) -> *mut u8;
        fn kern_miri_object_realloc_end(
            old: *mut u8,
            new: *mut u8,
            size: usize,
            align: usize,
        ) -> *mut u8;
    }

    /// After the allocator returned `ptr` (null if it failed) for `layout`.
    /// Returns the pointer to give to the caller.
    pub(in crate::allocator) fn born(ptr: *mut u8, layout: Layout) -> *mut u8 {
        if ptr.is_null() {
            return ptr;
        }
        // SAFETY: The allocator just handed out `ptr` for `layout`.
        unsafe { kern_miri_object_born(ptr, layout.size(), layout.align()) }
    }

    /// Before the block at `ptr` goes back to the allocator. Returns the
    /// pointer to pass to the allocator.
    pub(in crate::allocator) fn died(ptr: *mut u8) -> *mut u8 {
        // SAFETY: The caller frees the block at `ptr`.
        unsafe { kern_miri_object_died(ptr) }
    }

    /// Before the allocator reallocates the block at `ptr`. Returns the
    /// pointer to pass to the allocator.
    pub(in crate::allocator) fn realloc_begin(ptr: *mut u8) -> *mut u8 {
        // SAFETY: The caller reallocates the block at `ptr`.
        unsafe { kern_miri_object_realloc_begin(ptr) }
    }

    /// After the allocator reallocated `old` to `size` bytes aligned to
    /// `align` at `new`, or failed (`new` is null). Returns the pointer to give
    /// to the caller.
    pub(in crate::allocator) fn realloc_end(
        old: *mut u8,
        new: *mut u8,
        size: usize,
        align: usize,
    ) -> *mut u8 {
        // SAFETY: `realloc_begin(old)` came before, and the allocator returned `new`.
        unsafe { kern_miri_object_realloc_end(old, new, size, align) }
    }
}

pub(super) use imp::*;
