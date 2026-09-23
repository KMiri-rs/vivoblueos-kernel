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

// An intrusive minimum heap.

use crate::{
    impl_simple_intrusive_adapter,
    intrusive::{Adapter, Nested},
    list::typed_ilist::ListHead,
};
use core::{marker::PhantomData, ptr::NonNull};

#[derive(Default)]
#[repr(transparent)]
pub struct IouMinHeapNodeMut<'a, T, A: const Adapter<T>> {
    node: Option<NonNull<MinHeapNode<T, A>>>,
    _lt: PhantomData<&'a mut T>,
}

impl<T, A: const Adapter<T>> IouMinHeapNodeMut<'_, T, A> {
    pub const fn new() -> Self {
        Self {
            node: None,
            _lt: PhantomData,
        }
    }

    pub unsafe fn from_mut(node: &mut T) -> Self {
        Self {
            node: Some(MinHeapNode::<T, A>::node_of(node)),
            _lt: PhantomData,
        }
    }
}

#[derive(Default, Debug)]
struct Link<T, A: const Adapter<T>>(PhantomData<(T, A)>);

const impl<T, A: const Adapter<T>> Adapter<MinHeapNode<T, A>> for Link<T, A> {
    fn offset() -> usize {
        core::mem::offset_of!(MinHeapNode<T, A>, link)
    }
}

#[allow(clippy::type_complexity)]
type LinkType<T, A> = ListHead<T, Nested<T, A, MinHeapNode<T, A>, Link<T, A>>>;

#[derive(Default, Debug)]
pub struct MinHeapNode<T, A: const Adapter<T>> {
    // Put the `link` first so that we don't need to use the complex Nested
    // adapter.
    link: LinkType<T, A>,
    parent: Option<NonNull<MinHeapNode<T, A>>>,
}

impl<T, A: const Adapter<T>> MinHeapNode<T, A> {
    pub const fn new() -> Self {
        Self {
            link: LinkType::new(),
            parent: None,
        }
    }

    /// Converts a mutable reference to the complete type `T` into a `NonNull`
    /// pointer to its embedded `MinHeapNode`.
    ///
    /// This function preserves the provenance of the complete `T`, allowing
    /// safe recovery of an `&T` reference later via [`owner_ptr`].
    ///
    /// # Safety
    /// The caller must ensure that `this` actually contains a valid `MinHeapNode`
    /// at offset `A::offset()`.
    fn node_of(this: &mut T) -> NonNull<Self> {
        let ptr = (this as *mut T).cast::<u8>().wrapping_add(A::offset());
        unsafe { NonNull::new_unchecked(ptr.cast()) }
    }

    /// Recovers a `NonNull` pointer to `Self` from a `NonNull` pointer to its
    /// embedded `LinkType`.
    ///
    /// # Safety
    /// The caller must ensure that `link` points to a valid `LinkType` that is
    /// part of a properly constructed `MinHeapNode`.
    fn node_of_link(link: NonNull<LinkType<T, A>>) -> NonNull<Self> {
        let ptr = link
            .as_ptr()
            .cast::<u8>()
            .wrapping_sub(Link::<T, A>::offset());
        unsafe { NonNull::new_unchecked(ptr.cast()) }
    }

    /// Recovers a raw pointer to the complete type `T` from a `NonNull` pointer
    /// to its embedded `MinHeapNode`.
    ///
    /// This is the inverse operation of [`node_of`]. Unlike `node_of`, which
    /// starts from `&mut T`, this function works backwards from the node.
    ///
    /// # Safety
    /// The caller must ensure that `node` points to a valid `MinHeapNode` that
    /// belongs to some `T` instance.
    fn owner_ptr(node: NonNull<Self>) -> *mut T {
        node.as_ptr().cast::<u8>().wrapping_sub(A::offset()).cast()
    }

    /// Returns the `NonNull` pointer to the `LinkType` field within the node.
    ///
    /// # Safety
    /// The caller must ensure that `node` points to a valid `MinHeapNode`.
    unsafe fn link_ptr(node: NonNull<Self>) -> NonNull<LinkType<T, A>> {
        NonNull::new_unchecked(core::ptr::addr_of_mut!((*node.as_ptr()).link))
    }

    /// Returns the left child of the given node, if it exists.
    ///
    /// # Safety
    /// The caller must ensure that `node` points to a valid heap node.
    unsafe fn left(node: NonNull<Self>) -> Option<NonNull<Self>> {
        (*node.as_ptr()).link.left().map(Self::node_of_link)
    }

    /// Returns the right child of the given node, if it exists.
    ///
    /// # Safety
    /// The caller must ensure that `node` points to a valid heap node.
    unsafe fn right(node: NonNull<Self>) -> Option<NonNull<Self>> {
        (*node.as_ptr()).link.right().map(Self::node_of_link)
    }

    /// Sets the left child of the given node.
    ///
    /// # Safety
    /// The caller must ensure that `node` points to a valid heap node.
    unsafe fn set_left(node: NonNull<Self>, child: Option<NonNull<Self>>) {
        let child = child.map(|c| Self::link_ptr(c));
        (*node.as_ptr()).link.set_left(child);
    }

    /// Sets the right child of the given node.
    ///
    /// # Safety
    /// The caller must ensure that `node` points to a valid heap node.
    unsafe fn set_right(node: NonNull<Self>, child: Option<NonNull<Self>>) {
        let child = child.map(|c| Self::link_ptr(c));
        (*node.as_ptr()).link.set_right(child);
    }

    /// Asserts that the given node is detached (has no children).
    ///
    /// This is a helper for debug assertions in testing and validation.
    ///
    /// # Safety
    /// The caller must ensure that `node` points to a valid heap node.
    #[cfg(debug_assertions)]
    fn assert_detached(node: NonNull<Self>) {
        unsafe {
            debug_assert!(Self::left(node).is_none());
            debug_assert!(Self::right(node).is_none());
        }
    }
}

#[derive(Debug)]
pub struct MinHeap<T, A: const Adapter<T>, Compare>
where
    Compare: Fn(&T, &T) -> core::cmp::Ordering,
{
    popped: LinkType<T, A>,
    root: Option<NonNull<MinHeapNode<T, A>>>,
    size: usize,
    compare: Compare,
}

// Compute height and branch directions.
fn compute_path(mut i: usize) -> (usize, usize) {
    let mut height = 0;
    let mut direction = 0;
    while i > 0 {
        direction = (direction << 1) | ((i - 1) & 1);
        i = (i - 1) / 2;
        height += 1;
    }
    (height, direction)
}

impl<T, A: const Adapter<T>, C> MinHeap<T, A, C>
where
    C: Fn(&T, &T) -> core::cmp::Ordering,
{
    pub const fn new(compare: C) -> Self {
        Self {
            popped: LinkType::new(),
            root: None,
            size: 0,
            compare,
        }
    }

    pub fn iou_owner<'a>(iou: &IouMinHeapNodeMut<'a, T, A>) -> Option<&'a T>
    where
        A: 'a,
    {
        let node = iou.node?;
        Some(unsafe { &*MinHeapNode::owner_ptr(node) })
    }

    /// Locates the heap node at the given index `i` by computing its path
    /// from the root.
    ///
    /// Returns a tuple of `(current_node, parent_node)`, where `current_node`
    /// is the node at index `i` and `parent_node` is its parent.
    #[allow(clippy::type_complexity)]
    fn node_at(
        &self,
        i: usize,
        path: &mut (usize, usize),
    ) -> (
        Option<NonNull<MinHeapNode<T, A>>>,
        Option<NonNull<MinHeapNode<T, A>>>,
    ) {
        let (mut height, mut direction) = compute_path(i);
        path.0 = height;
        path.1 = direction;
        let mut current = self.root;
        let mut current_parent = None;
        while height > 0 && current.is_some() {
            current_parent = current;
            current = current.and_then(|node| unsafe {
                if direction & 1 == 0 {
                    MinHeapNode::left(node)
                } else {
                    MinHeapNode::right(node)
                }
            });
            direction >>= 1;
            height -= 1;
        }
        (current, current_parent)
    }

    /// Compares two heap nodes by their owner values.
    ///
    /// The comparison is performed on the complete `T` instances that own
    /// these nodes, ensuring correct ordering semantics.
    fn compare_nodes(
        &self,
        a: NonNull<MinHeapNode<T, A>>,
        b: NonNull<MinHeapNode<T, A>>,
    ) -> core::cmp::Ordering {
        // The owner references end before the caller changes any links.
        unsafe { (self.compare)(&*MinHeapNode::owner_ptr(a), &*MinHeapNode::owner_ptr(b)) }
    }

    /// Restores the heap property by moving a node up toward the root.
    ///
    /// This is called after inserting a new node or after removing a node
    /// where the replacement may violate the min-heap invariant.
    fn bottom_up_adjust(&mut self, node: NonNull<MinHeapNode<T, A>>) {
        while let Some(parent) = unsafe { (*node.as_ptr()).parent } {
            if self.compare_nodes(node, parent) != core::cmp::Ordering::Less {
                break;
            }
            unsafe { self.swap_nodes(parent, node) };
        }
    }

    /// Restores the heap property by moving a node down toward a leaf.
    ///
    /// This is called after removing the root node and replacing it with
    /// the last node in the heap.
    fn top_down_adjust(&mut self, node: NonNull<MinHeapNode<T, A>>) {
        loop {
            let mut min_child = unsafe { MinHeapNode::left(node) };
            if let Some(right) = unsafe { MinHeapNode::right(node) } {
                if min_child.is_none()
                    || self.compare_nodes(right, min_child.unwrap()) == core::cmp::Ordering::Less
                {
                    min_child = Some(right);
                }
            }
            let Some(child) = min_child else {
                break;
            };
            if self.compare_nodes(node, child) == core::cmp::Ordering::Less {
                break;
            }
            unsafe { self.swap_nodes(node, child) };
        }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn push<'a>(&mut self, val: &'a mut T) -> Option<IouMinHeapNodeMut<'a, T, A>> {
        let node = MinHeapNode::node_of(val);
        self.push_node(node).then_some(IouMinHeapNodeMut {
            node: Some(node),
            _lt: PhantomData,
        })
    }

    /// Attempts to insert a node into the heap.
    ///
    /// This internal method performs all validation checks and tree modifications.
    /// Returns `true` if the node was successfully inserted, `false` otherwise.
    ///
    /// Validation criteria:
    /// - Node is not already in the heap (not root and no parent)
    /// - Node has no existing children (not already linked)
    /// - Node is not already in the popped list
    fn push_node(&mut self, node: NonNull<MinHeapNode<T, A>>) -> bool {
        unsafe {
            if Some(node) == self.root
                || (*node.as_ptr()).parent.is_some()
                || MinHeapNode::left(node).is_some()
                || MinHeapNode::right(node).is_some()
                || self.popped.next == Some(MinHeapNode::link_ptr(node))
            {
                return false;
            }
            let mut path = (0, 0);
            let (current, parent) = self.node_at(self.size, &mut path);
            debug_assert!(current.is_none());
            (*node.as_ptr()).parent = parent;
            self.size += 1;
            let Some(parent) = parent else {
                debug_assert_eq!(path.0, 0);
                self.root = Some(node);
                return true;
            };
            if (1 << (path.0 - 1)) & path.1 == 0 {
                debug_assert_eq!(MinHeapNode::left(parent), None);
                MinHeapNode::set_left(parent, Some(node));
            } else {
                debug_assert_eq!(MinHeapNode::right(parent), None);
                MinHeapNode::set_right(parent, Some(node));
            }
            self.bottom_up_adjust(node);
            true
        }
    }

    /// Swaps two nodes in the heap, preserving all tree relationships.
    ///
    /// This is a complex operation that maintains:
    /// - Parent-child relationships for both swapped nodes
    /// - Sibling node parent pointers
    /// - The root pointer if one of the nodes is the root
    ///
    /// # Safety
    /// The caller must ensure that both `x` and `y` point to valid heap nodes.
    unsafe fn swap_nodes(&mut self, x: NonNull<MinHeapNode<T, A>>, y: NonNull<MinHeapNode<T, A>>) {
        if x == y {
            return;
        }

        let px = (*x.as_ptr()).parent;
        let py = (*y.as_ptr()).parent;

        if px == Some(y) {
            return self.swap_nodes(y, x);
        }

        let get_left = MinHeapNode::left;
        let get_right = MinHeapNode::right;
        let set_left = MinHeapNode::set_left;
        let set_right = MinHeapNode::set_right;

        let lx = get_left(x);
        let rx = get_right(x);

        let ly = get_left(y);
        let ry = get_right(y);

        let mut update_parent_child = |old_ptr: NonNull<MinHeapNode<T, A>>,
                                       new_ptr: NonNull<MinHeapNode<T, A>>,
                                       parent: Option<NonNull<MinHeapNode<T, A>>>|
         -> Option<(bool, NonNull<MinHeapNode<T, A>>)> {
            match parent {
                None => {
                    self.root = Some(new_ptr);
                    None
                }
                Some(p) => {
                    let is_left = if get_left(p) == Some(old_ptr) {
                        set_left(p, Some(new_ptr));
                        true
                    } else {
                        set_right(p, Some(new_ptr));
                        false
                    };
                    Some((is_left, p))
                }
            }
        };

        if py == Some(x) {
            update_parent_child(x, y, px);
            (*y.as_ptr()).parent = px;
            (*x.as_ptr()).parent = Some(y);

            if lx == Some(y) {
                set_left(y, Some(x));
                set_right(y, rx);
                if let Some(r) = rx {
                    (*r.as_ptr()).parent = Some(y);
                }
            } else {
                set_right(y, Some(x));
                set_left(y, lx);
                if let Some(l) = lx {
                    (*l.as_ptr()).parent = Some(y);
                }
            }

            set_left(x, ly);
            if let Some(l) = ly {
                (*l.as_ptr()).parent = Some(x);
            }
            set_right(x, ry);
            if let Some(r) = ry {
                (*r.as_ptr()).parent = Some(x);
            }
        } else {
            let maybe_left = update_parent_child(x, y, px);
            // Sibling case.
            if let Some((is_left, p)) = maybe_left {
                if px == py {
                    if is_left {
                        set_right(p, Some(x));
                    } else {
                        set_left(p, Some(x));
                    }
                } else {
                    update_parent_child(y, x, py);
                }
            } else {
                update_parent_child(y, x, py);
            }

            (*x.as_ptr()).parent = py;
            (*y.as_ptr()).parent = px;

            set_left(x, ly);
            if let Some(l) = ly {
                (*l.as_ptr()).parent = Some(x);
            }
            set_left(y, lx);
            if let Some(l) = lx {
                (*l.as_ptr()).parent = Some(y);
            }

            set_right(x, ry);
            if let Some(r) = ry {
                (*r.as_ptr()).parent = Some(x);
            }
            set_right(y, rx);
            if let Some(r) = rx {
                (*r.as_ptr()).parent = Some(y);
            }
        }
    }

    fn is_linked_in_heap(&self, node: NonNull<MinHeapNode<T, A>>) -> bool {
        if Some(node) == self.root {
            return true;
        }
        unsafe { (*node.as_ptr()).parent.is_some() }
    }

    /// Removes a node from the heap and restores the heap property.
    ///
    /// The removed node is effectively replaced by the last node in the heap,
    /// then either bubbled up or pushed down as needed.
    fn inner_remove(&mut self, node: NonNull<MinHeapNode<T, A>>) {
        let mut path = (0, 0);
        let (last, last_parent) = self.node_at(self.size - 1, &mut path);
        let last = last.expect("Node should not be None when the index is valid");
        unsafe {
            MinHeapNode::assert_detached(last);
            self.swap_nodes(node, last);
            MinHeapNode::assert_detached(node);
            let node_parent = (*node.as_ptr()).parent;
            (*node.as_ptr()).parent = None;
            self.size -= 1;
            let Some(last_parent) = last_parent else {
                let root = self.root.take();
                debug_assert_eq!(root, Some(node));
                debug_assert_eq!(last, node);
                return;
            };
            debug_assert!(path.0 > 0);
            let is_left = (1 << (path.0 - 1)) & path.1 == 0;
            let parent = if node == last_parent {
                last
            } else {
                last_parent
            };
            debug_assert_eq!(Some(parent), node_parent);
            if is_left {
                debug_assert_eq!(MinHeapNode::left(parent), Some(node));
                MinHeapNode::set_left(parent, None);
            } else {
                debug_assert_eq!(MinHeapNode::right(parent), Some(node));
                MinHeapNode::set_right(parent, None);
            }
            if node == last {
                return;
            }
            if let Some(parent) = (*last.as_ptr()).parent {
                if self.compare_nodes(last, parent) == core::cmp::Ordering::Less {
                    self.bottom_up_adjust(last);
                    return;
                }
            } else {
                debug_assert_eq!(self.root, Some(last));
            }
            self.top_down_adjust(last);
        }
    }

    /// Detaches a popped node from the popped list.
    ///
    /// This removes the node's link from the doubly-linked list of popped nodes.
    ///
    /// # Safety
    /// The caller must ensure that `node` points to a valid heap node that is
    /// currently linked in the popped list.
    fn detach_popped(&mut self, node: NonNull<MinHeapNode<T, A>>) {
        unsafe {
            let link = MinHeapNode::link_ptr(node);
            let prev = (*link.as_ptr()).prev;
            let next = (*link.as_ptr()).next;
            if let Some(prev) = prev {
                (*prev.as_ptr()).next = next;
            } else if self.popped.next == Some(link) {
                self.popped.next = next;
            } else {
                panic!("Internal error: failed to detach popped node");
            }
            if let Some(next) = next {
                (*next.as_ptr()).prev = prev;
            }
            (*link.as_ptr()).prev = None;
            (*link.as_ptr()).next = None;
        }
    }

    pub fn remove<'a>(
        &mut self,
        iou: IouMinHeapNodeMut<'_, T, A>,
    ) -> Option<IouMinHeapNodeMut<'a, T, A>> {
        let Some(node) = iou.node else {
            panic!("Nil node")
        };
        if !self.is_linked_in_heap(node) {
            self.detach_popped(node);
            return Some(IouMinHeapNodeMut {
                node: None,
                _lt: PhantomData,
            });
        }
        self.inner_remove(node);
        Some(IouMinHeapNodeMut {
            node: None,
            _lt: PhantomData,
        })
    }

    pub fn is_active(&self, iou: &IouMinHeapNodeMut<'_, T, A>) -> bool {
        let Some(node) = iou.node else {
            return false;
        };
        self.is_linked_in_heap(node)
    }

    pub fn pop(&mut self) -> &mut Self {
        let Some(node) = self.root else {
            return self;
        };
        debug_assert!(self.is_linked_in_heap(node));
        self.inner_remove(node);
        unsafe {
            let link = MinHeapNode::link_ptr(node);
            debug_assert!((*link.as_ptr()).prev.is_none());
            debug_assert!((*link.as_ptr()).next.is_none());
            // The first popped node has no predecessor. Storing &self.popped
            // here would leave a pointer behind when the heap is moved/reborrowed.
            if let Some(next) = self.popped.next {
                (*next.as_ptr()).prev = Some(link);
            }
            (*link.as_ptr()).next = self.popped.next;
            self.popped.next = Some(link);
        }
        self
    }

    pub fn peek(&self) -> Option<&T> {
        let root = self.root?;
        Some(unsafe { &*MinHeapNode::owner_ptr(root) })
    }

    pub fn for_each_popped_value<F>(&mut self, f: F)
    where
        F: Fn(&mut T),
    {
        let mut current = self.popped.next;
        while let Some(link) = current {
            unsafe {
                current = (*link.as_ptr()).next;
                let node = MinHeapNode::node_of_link(link);
                f(&mut *MinHeapNode::owner_ptr(node));
            }
        }
    }

    /// Moves selected popped values back into the heap.
    ///
    /// Iterates over all nodes in the popped list and applies the `choose`
    /// predicate to each. Nodes that return `true` are detached from the
    /// popped list and re-inserted into the heap.
    ///
    /// # Returns
    /// The number of nodes that were moved back into the heap.
    pub fn move_chosen_popped_values_to_heap<F>(&mut self, choose: F) -> usize
    where
        F: Fn(&T) -> bool,
    {
        let mut chosen = 0;
        let mut current = self.popped.next;
        while let Some(link) = current {
            let node = MinHeapNode::node_of_link(link);
            unsafe {
                current = (*link.as_ptr()).next;
                debug_assert!(!self.is_linked_in_heap(node));
                if !choose(&*MinHeapNode::owner_ptr(node)) {
                    continue;
                }
            }
            self.detach_popped(node);
            // Keep stored pointers derived from the original borrow, not a temporary reborrow.
            let inserted = self.push_node(node);
            debug_assert!(inserted);
            chosen += 1;
        }
        chosen
    }

    pub fn validate(&self) {
        let size = self.size();
        for i in 0..size {
            let mut path = (0, 0);
            let (current, parent) = self.node_at(i, &mut path);
            let node = current.expect("Node should not be None when the index is valid");
            unsafe {
                if 2 * i + 1 >= size {
                    assert!(MinHeapNode::left(node).is_none());
                }
                if 2 * i + 2 >= size {
                    assert!(MinHeapNode::right(node).is_none());
                }
                assert_eq!((*node.as_ptr()).parent, parent);
                let Some(parent) = parent else {
                    assert_eq!(path.0, 0);
                    assert_eq!(self.root, current);
                    continue;
                };
                let order = self.compare_nodes(parent, node);
                assert!(order == core::cmp::Ordering::Less || order == core::cmp::Ordering::Equal);
                if 1 << (path.0 - 1) & path.1 == 0 {
                    assert_eq!(MinHeapNode::left(parent), current);
                } else {
                    assert_eq!(MinHeapNode::right(parent), current);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate test;
    use super::*;
    use test::Bencher;

    impl_simple_intrusive_adapter!(Node, Foo, node);

    struct Foo {
        node: MinHeapNode<Foo, Node>,
        val: usize,
    }

    #[test]
    fn test_nonzero_offset_push_pop() {
        // Use explicit alignment to ensure the node field is not at offset 0.
        // This tests the heap's ability to handle non-trivial field offsets.
        #[repr(C)]
        struct Entry {
            key: usize,
            node: MinHeapNode<Entry, EntryAdapter>,
        }
        impl_simple_intrusive_adapter!(EntryAdapter, Entry, node);

        let offset = core::mem::offset_of!(Entry, node);
        assert_ne!(offset, 0, "node offset should be non-zero due to alignment");

        let mut high = Box::new(Entry {
            key: 2,
            node: MinHeapNode::new(),
        });
        let mut low = Box::new(Entry {
            key: 1,
            node: MinHeapNode::new(),
        });
        let mut heap = MinHeap::<Entry, EntryAdapter, _>::new(|a, b| a.key.cmp(&b.key));
        let high_iou = heap.push(&mut *high).unwrap();
        let low_iou = heap.push(&mut *low).unwrap();
        assert_eq!(heap.peek().unwrap().key, 1);
        heap.pop();
        assert_eq!(heap.peek().unwrap().key, 2);
        assert!(heap.remove(low_iou).is_some());
        heap.pop();
        assert!(heap.peek().is_none());
        assert!(heap.remove(high_iou).is_some());
    }

    #[test]
    fn test_compute_path() {
        assert_eq!(compute_path(0), (0, 0));
        assert_eq!(compute_path(1), (1, 0b0));
        assert_eq!(compute_path(2), (1, 0b1));
        assert_eq!(compute_path(3), (2, 0b00));
        assert_eq!(compute_path(4), (2, 0b10));
        assert_eq!(compute_path(5), (2, 0b01));
        assert_eq!(compute_path(6), (2, 0b11));
        assert_eq!(compute_path(7), (3, 0b000));
        assert_eq!(compute_path(8), (3, 0b100));
        assert_eq!(compute_path(9), (3, 0b010));
        assert_eq!(compute_path(10), (3, 0b110));
        assert_eq!(compute_path(11), (3, 0b001));
        assert_eq!(compute_path(12), (3, 0b101));
        assert_eq!(compute_path(13), (3, 0b011));
        assert_eq!(compute_path(14), (3, 0b111));
    }

    #[test]
    fn test_basic_insertion() {
        let mut heap = MinHeap::<Foo, Node, _>::new(|l, r| l.val.cmp(&r.val));
        let mut path = (0, 0);
        let (n, p) = heap.node_at(0, &mut path);
        assert!(n.is_none());
        assert!(p.is_none());
        assert_eq!(heap.size(), 0);
        let mut n0 = Box::new(Foo {
            node: MinHeapNode::new(),
            val: 42,
        });
        heap.push(&mut n0);
        assert_eq!(heap.size(), 1);
        let root = heap.peek();
        assert!(root.is_some());
        assert_eq!(root.unwrap().val, 42);
        let mut n1 = Box::new(Foo {
            node: MinHeapNode::new(),
            val: 37,
        });
        heap.push(&mut n1);
        assert_eq!(heap.size(), 2);
        let root = heap.peek();
        assert!(root.is_some());
        assert_eq!(root.unwrap().val, 37);
        let mut n2 = Box::new(Foo {
            node: MinHeapNode::new(),
            val: 43,
        });
        heap.push(&mut n2);
        assert_eq!(heap.size(), 3);
        let root = heap.peek();
        assert!(root.is_some());
        assert_eq!(root.unwrap().val, 37);
        let mut n3 = Box::new(Foo {
            node: MinHeapNode::new(),
            val: 23,
        });
        heap.push(&mut n3);
        assert_eq!(heap.size(), 4);
        let root = heap.peek();
        assert!(root.is_some());
        assert_eq!(root.unwrap().val, 23);
    }

    #[test]
    fn test_removal() {
        let mut heap = MinHeap::<Foo, Node, _>::new(|l, r| l.val.cmp(&r.val));
        let mut n0 = Box::new(Foo {
            node: MinHeapNode::new(),
            val: 0,
        });
        let mut n1 = Box::new(Foo {
            node: MinHeapNode::new(),
            val: 1,
        });
        let mut n2 = Box::new(Foo {
            node: MinHeapNode::new(),
            val: 2,
        });
        let mut n3 = Box::new(Foo {
            node: MinHeapNode::new(),
            val: 3,
        });
        let iou2 = heap.push(&mut n2).unwrap();
        heap.validate();
        let iou1 = heap.push(&mut n1).unwrap();
        heap.validate();
        let iou0 = heap.push(&mut n0).unwrap();
        heap.validate();
        assert_eq!(heap.size(), 3);
        assert_eq!(heap.peek().unwrap().val, 0);
        let iou3 = unsafe { IouMinHeapNodeMut::from_mut(&mut *n3) };
        assert!(heap.remove(iou3).is_none());
        assert_eq!(heap.size(), 3);
        assert_eq!(heap.peek().unwrap().val, 0);
        heap.validate();
        heap.remove(iou0);
        heap.validate();
        assert_eq!(heap.size(), 2);
        assert_eq!(heap.peek().unwrap().val, 1);
        heap.remove(iou2);
        heap.validate();
        assert_eq!(heap.peek().unwrap().val, 1);
        let mut n3 = Box::new(Foo {
            node: MinHeapNode::new(),
            val: 3,
        });
        heap.validate();
        let iou3 = heap.push(&mut n3).unwrap();
        assert_eq!(heap.size(), 2);
        assert_eq!(heap.peek().unwrap().val, 1);
        heap.pop();
        assert_eq!(heap.size(), 1);
        heap.validate();
        heap.pop();
        heap.validate();
        assert_eq!(heap.size(), 0);
        heap.validate();
        heap.remove(iou1);
        heap.validate();
        heap.remove(iou3);
        heap.validate();
    }

    #[test]
    fn test_stack_removal() {
        let mut heap = MinHeap::<Foo, Node, _>::new(|l, r| l.val.cmp(&r.val).reverse());
        {
            let mut n0 = Foo {
                node: MinHeapNode::new(),
                val: 0,
            };
            let b0 = heap.push(&mut n0).unwrap();
            let mut n1 = Foo {
                node: MinHeapNode::new(),
                val: 1,
            };
            let b1 = heap.push(&mut n1).unwrap();
            let mut n2 = Foo {
                node: MinHeapNode::new(),
                val: 2,
            };
            let b2 = heap.push(&mut n2).unwrap();
            let mut n3 = Foo {
                node: MinHeapNode::new(),
                val: 3,
            };
            let b3 = heap.push(&mut n3).unwrap();
            let mut n4 = Foo {
                node: MinHeapNode::new(),
                val: 4,
            };
            let b4 = heap.push(&mut n4).unwrap();
            let mut n5 = Foo {
                node: MinHeapNode::new(),
                val: 5,
            };
            let b5 = heap.push(&mut n5).unwrap();
            assert_eq!(heap.peek().unwrap().val, 5);
            heap.remove(b3);
            assert_eq!(heap.peek().unwrap().val, 5);
            heap.remove(b5);
            assert_eq!(heap.peek().unwrap().val, 4);
            heap.remove(b2);
            assert_eq!(heap.peek().unwrap().val, 4);
            heap.remove(b4);
            assert_eq!(heap.peek().unwrap().val, 1);
            heap.remove(b0);
            assert_eq!(heap.peek().unwrap().val, 1);
            heap.remove(b1);
            assert!(heap.peek().is_none());
        }
    }

    #[test]
    fn test_remove_sibling() {
        let mut heap = MinHeap::<Foo, Node, _>::new(|l, r| l.val.cmp(&r.val));
        let mut n0 = Foo {
            node: MinHeapNode::new(),
            val: 0,
        };
        let mut n1 = Foo {
            node: MinHeapNode::new(),
            val: 1,
        };
        let mut n2 = Foo {
            node: MinHeapNode::new(),
            val: 2,
        };
        heap.push(&mut n0);
        let iou1 = heap.push(&mut n1).unwrap();
        heap.push(&mut n2);
        heap.validate();
        heap.remove(iou1);
    }

    #[bench]
    fn bench_push_and_pop(b: &mut Bencher) {
        b.iter(|| {
            let mut heap = MinHeap::<Foo, Node, _>::new(|l, r| l.val.cmp(&r.val).reverse());
            let mut n0 = Foo {
                node: MinHeapNode::new(),
                val: 0,
            };
            let mut n1 = Foo {
                node: MinHeapNode::new(),
                val: 1,
            };
            let mut n2 = Foo {
                node: MinHeapNode::new(),
                val: 2,
            };
            let mut n3 = Foo {
                node: MinHeapNode::new(),
                val: 3,
            };
            heap.push(&mut n0);
            heap.push(&mut n1);
            heap.push(&mut n2);
            heap.push(&mut n3);
            debug_assert_eq!(heap.peek().unwrap().val, 3);
            let size = heap.size();
            debug_assert_eq!(size, 4);
            for _i in 0..size {
                heap.pop();
            }
        });
    }

    #[bench]
    fn bench_push_and_pop_std_heap(b: &mut Bencher) {
        use std::collections::BinaryHeap;
        #[derive(Eq, Ord, PartialEq, PartialOrd)]
        struct Foo {
            val: usize,
        }
        b.iter(|| {
            let mut heap = BinaryHeap::new();
            heap.push(0);
            heap.push(1);
            heap.push(2);
            heap.push(3);
            debug_assert_eq!(*heap.peek().unwrap(), 3);
            let size = heap.len();
            debug_assert_eq!(size, 4);
            for _i in 0..size {
                heap.pop();
            }
        });
    }

    #[bench]
    fn bench_push_and_pop_std_btree(b: &mut Bencher) {
        use std::collections::BTreeSet;
        #[derive(Eq, Ord, PartialEq, PartialOrd)]
        struct Foo {
            val: usize,
        }
        b.iter(|| {
            let mut heap = BTreeSet::new();
            heap.insert(0);
            heap.insert(1);
            heap.insert(2);
            heap.insert(3);
            let size = heap.len();
            debug_assert_eq!(size, 4);
            for _i in 0..size {
                heap.pop_first();
            }
        });
    }

    #[test]
    fn fuzz() {
        let mut iheap = MinHeap::<Foo, Node, _>::new(|l, r| l.val.cmp(&r.val).reverse());
        let mut vec = Vec::new();
        for i in 0..11 {
            vec.push(Foo {
                node: MinHeapNode::new(),
                val: i,
            });
        }
        iheap.push(&mut vec[0]);
        iheap.validate();
        iheap.push(&mut vec[2]);
        iheap.validate();
        iheap.push(&mut vec[4]);
        iheap.validate();
        iheap.push(&mut vec[6]);
        iheap.validate();
        iheap.push(&mut vec[8]);
        iheap.validate();
        iheap.push(&mut vec[10]);
        iheap.validate();
        iheap.remove(unsafe { IouMinHeapNodeMut::from_mut(&mut vec[0]) });
        iheap.validate();
        iheap.push(&mut vec[1]);
        iheap.validate();
        iheap.remove(unsafe { IouMinHeapNodeMut::from_mut(&mut vec[2]) });
        iheap.validate();
        iheap.push(&mut vec[3]);
        iheap.validate();
        iheap.remove(unsafe { IouMinHeapNodeMut::from_mut(&mut vec[4]) });
        iheap.validate();
        iheap.push(&mut vec[5]);
        iheap.validate();
        iheap.remove(unsafe { IouMinHeapNodeMut::from_mut(&mut vec[6]) });
        iheap.validate();
    }

    #[test]
    fn fuzz1() {
        let mut iheap = MinHeap::<Foo, Node, _>::new(|l, r| l.val.cmp(&r.val).reverse());
        let mut vec = Vec::new();
        for i in 0..4 {
            vec.push(Foo {
                node: MinHeapNode::new(),
                val: i,
            });
        }
        for i in 0..3 {
            for f in vec.iter_mut() {
                if f.val % 2 == i % 2 {
                    iheap.validate();
                    iheap.push(f);
                    iheap.validate();
                } else {
                    iheap.validate();
                    iheap.remove(unsafe { IouMinHeapNodeMut::from_mut(f) });
                    iheap.validate();
                }
            }
        }
    }

    #[test]
    fn fuzz2() {
        let mut iheap = MinHeap::<Foo, Node, _>::new(|l, r| l.val.cmp(&r.val).reverse());
        let mut vec = Vec::new();
        for i in 0..1 << 10 {
            vec.push(Foo {
                node: MinHeapNode::new(),
                val: i,
            });
        }
        for i in 0..3 {
            for f in vec.iter_mut() {
                if f.val % 2 == i % 2 {
                    iheap.validate();
                    iheap.push(f);
                    iheap.validate();
                } else {
                    iheap.validate();
                    iheap.remove(unsafe { IouMinHeapNodeMut::from_mut(f) });
                    iheap.validate();
                }
            }
        }
    }

    #[test]
    fn concurrent_stress() {
        use crate::{tinyarc::TinyArc as Arc, tinyrwlock::RwLock};
        let num_cores = std::thread::available_parallelism().unwrap();
        let scratch = MinHeap::<Foo, Node, _>::new(|l, r| l.val.cmp(&r.val).reverse());
        let iheap = Arc::new(RwLock::new(scratch));
        let mut vec = Arc::new(RwLock::new(Vec::new()));
        {
            let mut vec_mut = vec.write();
            for i in 0..1 << 10 {
                vec_mut.push(Foo {
                    node: MinHeapNode::new(),
                    val: i,
                });
            }
        }
        let closure = {
            let iheap = iheap.clone();
            let vec = vec.clone();
            move || {
                {
                    let mut vec_mut = vec.write();
                    for f in vec_mut.iter_mut() {
                        iheap.write().push(f);
                    }
                }
                {
                    let mut vec_mut = vec.write();
                    for f in vec_mut.iter_mut() {
                        let iou = unsafe { IouMinHeapNodeMut::from_mut(f) };
                        iheap.write().remove(iou);
                    }
                }
            }
        };
        let mut handles = vec![];
        for i in 0..num_cores.into() {
            let handle = std::thread::spawn(closure.clone());
            handles.push(handle);
        }
        for handle in handles {
            handle.join().expect("Thread panicked");
        }
    }

    #[test]
    fn push_root_twice() {
        let mut iheap = MinHeap::<Foo, Node, _>::new(|l, r| l.val.cmp(&r.val).reverse());
        let mut root = Foo {
            node: MinHeapNode::new(),
            val: 42,
        };
        let mut iou = iheap.push(&mut root);
        assert!(iou.is_some());
        let iou_again = iheap.push(&mut root);
        assert!(iou_again.is_none());
        // Adding following statement won't compile due to iou is re-borrowed. So it demonstrates that
        // using Iou properly can prevent pushing an node twice.
        //iheap.remove(iou.unwrap());
    }
}
