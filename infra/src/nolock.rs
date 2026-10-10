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

// Implement a lock based on UnsafeCell to optimize performance on single core.
// The APIs of this lock should be compatible with that in tinyrwlock.

use crate::intrusive::Adapter;
use core::{
    cell::UnsafeCell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

#[derive(Debug)]
pub struct NoLock<T: ?Sized> {
    data: UnsafeCell<T>,
}

impl<T: Default> Default for NoLock<T> {
    fn default() -> Self {
        Self::new(Default::default())
    }
}

#[derive(Debug)]
pub struct NoLockWriteGuard<'a, T: 'a + ?Sized> {
    inner: *mut T,
    _a: PhantomData<&'a mut T>,
}

impl<T: ?Sized> Deref for NoLockWriteGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.inner }
    }
}

impl<T: ?Sized> DerefMut for NoLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.inner }
    }
}

#[derive(Debug)]
pub struct NoLockReadGuard<'a, T: 'a + ?Sized> {
    inner: *const T,
    _a: PhantomData<&'a mut T>,
}

impl<T: ?Sized> Deref for NoLockReadGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.inner }
    }
}

impl<T> NoLock<T> {
    pub const fn new(val: T) -> Self {
        Self {
            data: UnsafeCell::new(val),
        }
    }
}

impl<T: ?Sized> NoLock<T> {
    pub fn try_write(&self) -> Option<NoLockWriteGuard<'_, T>> {
        Some(NoLockWriteGuard {
            inner: self.data.get(),
            _a: PhantomData,
        })
    }

    pub fn try_read(&self) -> Option<NoLockReadGuard<'_, T>> {
        Some(NoLockReadGuard {
            inner: self.data.get() as *const _,
            _a: PhantomData,
        })
    }

    pub fn write(&self) -> NoLockWriteGuard<'_, T> {
        self.try_write().unwrap()
    }

    pub fn read(&self) -> NoLockReadGuard<'_, T> {
        self.try_read().unwrap()
    }

    pub fn reader_count(&self) -> usize {
        usize::MAX
    }

    pub fn writer_count(&self) -> usize {
        usize::MAX
    }
}

unsafe impl<T: ?Sized + Send> Send for NoLock<T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for NoLock<T> {}
unsafe impl<T: ?Sized + Send + Sync> Send for NoLockWriteGuard<'_, T> {}
unsafe impl<T: ?Sized + Send + Sync> Sync for NoLockWriteGuard<'_, T> {}
unsafe impl<T: ?Sized + Sync> Send for NoLockReadGuard<'_, T> {}
unsafe impl<T: ?Sized + Sync> Sync for NoLockReadGuard<'_, T> {}

#[derive(Default, Debug)]
pub struct INoLock<T: Sized, A: Adapter<T>> {
    lock: NoLock<()>,
    _a: PhantomData<(T, A)>,
}

impl<T: Sized, A: Adapter<T>> INoLock<T, A> {
    #[inline]
    pub const fn new() -> Self {
        Self {
            lock: NoLock::new(()),
            _a: PhantomData,
        }
    }

    #[inline]
    fn this(&self) -> &T {
        let ptr = self as *const _ as *const u8;
        let base = unsafe { ptr.sub(A::offset()) as *const T };
        unsafe { &*base }
    }

    #[inline]
    fn this_mut(&self) -> &mut T {
        let ptr = self as *const _ as *mut u8;
        let base = unsafe { ptr.sub(A::offset()) as *mut T };
        unsafe { &mut *base }
    }

    #[inline]
    pub fn read(&self) -> NoLockReadGuard<'_, T> {
        let inner = self.this() as *const T;
        NoLockReadGuard {
            inner,
            _a: PhantomData,
        }
    }

    #[inline]
    pub fn try_read(&self) -> Option<NoLockReadGuard<'_, T>> {
        let inner = self.this() as *const T;
        Some(NoLockReadGuard {
            inner,
            _a: PhantomData,
        })
    }

    #[inline]
    pub fn write(&self) -> NoLockWriteGuard<'_, T> {
        let inner = self.this_mut() as *mut T;
        NoLockWriteGuard {
            inner,
            _a: PhantomData,
        }
    }

    #[inline]
    pub fn try_write(&self) -> Option<NoLockWriteGuard<'_, T>> {
        let inner = self.this_mut() as *mut T;
        Some(NoLockWriteGuard {
            inner,
            _a: PhantomData,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read() {
        let l = NoLock::new(42);
        let r = l.read();
        assert_eq!(*r, 42);
    }

    #[test]
    fn test_write() {
        let l = NoLock::new(0);
        let mut w = l.write();
        *w = 42;
        drop(w);
        let r = l.read();
        assert_eq!(*r, 42);
    }

    #[test]
    fn test_static_publication() {
        use crate::tinyarc::{TinyArc, TinyArcInner};
        use core::{mem::MaybeUninit, ptr::NonNull};

        static mut SLOT: MaybeUninit<TinyArcInner<u64>> = MaybeUninit::uninit();
        unsafe {
            let slot = (&raw mut SLOT).cast::<TinyArcInner<u64>>();
            slot.write(TinyArcInner::new(17));
            // Publish a shared TinyArc over the static storage, then mutate it.
            let mut arc = TinyArc::from_inner(NonNull::new_unchecked(slot));
            *TinyArc::get_mut_unchecked(&mut arc) = 42;
            assert_eq!(*arc, 42);
            // Static storage must not be passed to Box::drop on unwind.
            core::mem::forget(arc);
        }
    }

    // `INoLock::this_mut` computes its owning struct's address by subtracting
    // an adapter offset from `self`, then retags it `Unique`. This is only sound
    // when the owner is reached through a reference that still carries exclusive
    // (`Unique`) provenance. When the lock is an intrusive, zero-sized field of
    // a struct in static storage that was published through a *shared* reference
    // (as `ThreadNode::from_static_inner_ref` used to do), the `Unique` retag
    // fails under Stacked Borrows ("that tag does not exist"). The fix is
    // caller-side: initialize through the exclusive `&mut` borrow before
    // publishing, then derive the shared handle from `&raw mut` provenance. This
    // mirrors `ISpinLock<Thread, OffsetOfLock>` embedded in a static `Thread`.
    struct ThreadLike {
        // Keep `lock` at a non-zero offset so `this_mut` walks back to a
        // distinct, non-zero-sized region, as it does for the real `Thread`.
        _id: u64,
        lock: INoLock<ThreadLike, ThreadLock>,
    }

    crate::impl_simple_intrusive_adapter!(ThreadLock, ThreadLike, lock);

    // NOTE: this is an example to show `&'static mut` + `lock.write()` violates alising rules.
    #[test]
    fn test_inlock_from_static_storage() {
        use core::{mem::MaybeUninit, ptr};

        static mut SLOT: MaybeUninit<ThreadLike> = MaybeUninit::uninit();
        unsafe {
            let slot = ptr::addr_of_mut!(SLOT).cast::<ThreadLike>();
            // Initialize the whole owner while the storage is still exclusively
            // borrowed (`&raw mut`), before any shared handle is published.
            slot.write(ThreadLike {
                _id: 0,
                lock: INoLock::new(),
            });
            // Mutate through the exclusive borrow first, exactly like
            // `build_static_thread` does via `s.arc.get_mut()`.
            (*slot).lock.write();
        }
    }

    // NOTE: this is an example to show `lock.write()` still violates alising rules,
    // and thus it's not a sound API at all.
    #[test]
    fn test_inlock_from_local_storage() {
        let slot = ThreadLike {
            _id: 0,
            lock: INoLock::new(),
        };
        unsafe { slot.lock.write() };
    }
}
