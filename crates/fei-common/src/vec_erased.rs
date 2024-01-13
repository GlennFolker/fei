//! An unsafe statically-unknown homogenous list data container, similar to [`Vec`].

use crate::{
    ptr::{
        Ptr, PtrMut, PtrOwned,
    },
    array_layout, drop_for,
};
use std::{
    alloc::{
        Layout,
        alloc, dealloc, realloc,
        handle_alloc_error,
    },
    ptr::NonNull,
};

/// An unsafe statically-unknown homogenous list data container, similar to [`Vec`]. Due to the
/// nature of unknown data types, this data collection is highly unsafe and should only be used if
/// absolutely necessary.
///
/// # Example
/// ```
/// use fei_common::{
///     vec_erased::VecErased,
///     ptr::PtrOwned,
/// };
///
/// #[derive(Debug, Copy, Clone, Eq, PartialEq)]
/// struct Data(u32);
///
/// let mut vec = VecErased::typed::<Data>();
/// // Alternatively, you can do:
/// // `unsafe { DynVec::new(Layout::new::<MyData>(), fei_common::drop_for::<MyData>().into()) }`
///
/// // The vector is initially not holding any memory.
/// assert_eq!(vec.len(), 0);
/// assert_eq!(vec.capacity(), 0);
///
/// // Push elements to the vector.
/// unsafe {
///     PtrOwned::take(Data(314), |ptr| vec.push(ptr));
///     PtrOwned::take(Data(159), |ptr| vec.push(ptr));
///     PtrOwned::take(Data(271), |ptr| vec.push(ptr));
///     PtrOwned::take(Data(828), |ptr| vec.push(ptr));
/// }
///
/// // The vector has allocated a buffer.
/// assert_eq!(vec.len(), 4);
/// assert!(vec.capacity() >= 4);
///
/// // Pop elements from the vector.
/// unsafe {
///     assert_eq!(vec.pop(|ptr| ptr.read::<Data>()), Some(Data(828)));
///     assert_eq!(vec.pop(|ptr| ptr.read::<Data>()), Some(Data(271)));
/// }
///
/// // Access the vector's elements.
/// unsafe {
///     assert_eq!(vec.get(0).unwrap().deref::<Data>(), &Data(314));
///     assert_eq!(vec.get_mut(1).unwrap().deref_mut::<Data>(), &mut Data(159));
///     assert!(vec.get(2).is_none());
/// }
///
/// // Clear the vector.
/// vec.clear();
/// assert_eq!(vec.len(), 0);
/// ```
///
/// # Safety
/// Given a `DynVec` and its data type `T`, the following must be ensured to avoid
/// [Undefined Behavior](https://doc.rust-lang.org/beta/reference/behavior-considered-undefined.html):
/// - `T` must outlive the vector.
/// - All data types inserted to the vector must be equivalent to `T`; i.e., it must have the same
///   size and alignment as `T`, and can be safely dropped with [the dropper function](VecErased::dropper).
pub struct VecErased {
    array: NonNull<u8>,
    layout: Layout,
    array_layout: Layout,
    array_stride: usize,
    dropper: DropErased,

    len: usize,
    cap: usize,
}

/// Defines how items in the [`VecErased`] are dropped. Most commonly created with
/// [`drop_for`]`::<T>().into()`, which will resolve to [`DropErased::None`] for
/// [`None`] and [`DropErased::Auto`] for [`Some`].
///
/// # Safety
/// - Argument of this function is the aligned type-erased pointer to the item to be dropped
///   in-place.
/// - The function must only call the drop implementation of the item's actual type, most commonly
///   done by casting the pointer to `T` and invoking [`drop_in_place`](std::ptr::drop_in_place).
#[derive(Copy, Clone)]
pub enum DropErased {
    /// The items will *not* be dropped. The only sensible reason this is chosen is to optimize types
    /// that don't need to be dropped, as per [`needs_drop`](std::mem::needs_drop).
    None,
    /// The items will be dropped once the vector is dropped. This is the most common behavior, as
    /// seen in regular [`Vec`]s.
    Auto(unsafe fn(*mut u8)),
    /// The items will *not* be dropped, but users are still able to manually drop the items
    /// [in-place](PtrMut::drop_in_place_with) through the [`dropper`](VecErased::dropper) getter. This
    /// is equivalent of a [`Vec`] containing [`MaybeUninit<T>`](std::mem::MaybeUninit).
    Manual(unsafe fn(*mut u8)),
}

impl DropErased {
    #[inline]
    pub const fn automatic<T>() -> Self {
        match drop_for::<T>() {
            None => Self::None,
            Some(dropper) => Self::Auto(dropper),
        }
    }

    #[inline]
    pub const fn manual<T>() -> Self {
        match drop_for::<T>() {
            None => Self::None,
            Some(dropper) => Self::Manual(dropper),
        }
    }

    /// Converts [`Automatic`](DropErased::Auto) to [`Manual`](DropErased::Manual).
    #[inline]
    pub const fn into_manual(self) -> Self {
        match self {
            Self::Auto(dropper) => Self::Manual(dropper),
            _ => self,
        }
    }

    /// Converts [`Manual`](DropErased::Manual) to [`Automatic`](DropErased::Auto).
    #[inline]
    pub const fn into_automatic(self) -> Self {
        match self {
            Self::Manual(dropper) => Self::Auto(dropper),
            _ => self,
        }
    }
}

impl From<Option<unsafe fn(*mut u8)>> for DropErased {
    #[inline]
    fn from(dropper: Option<unsafe fn(*mut u8)>) -> Self {
        match dropper {
            Some(dropper) => Self::Auto(dropper),
            None => Self::None,
        }
    }
}

impl VecErased {
    /// Constructs a new [`VecErased`] from the item layout and drop implementation without pre-allocating
    /// the buffer.
    ///
    /// # Safety
    /// - The dropper must follow the safety requirements mentioned in [`DropErased`].
    #[inline]
    pub const unsafe fn new(layout: Layout, drop: DropErased) -> Self {
        let (array_layout, array_stride) = array_layout(layout, 0);
        Self {
            array: NonNull::dangling(),
            layout,
            array_layout,
            array_stride,
            dropper: drop,

            len: 0,
            cap: 0,
        }
    }

    /// Constructs a new [`VecErased`] from the item layout and drop implementation that pre-allocates
    /// the buffer with the size of the given `capacity`.
    ///
    /// # Safety
    /// - The dropper must follow the safety requirements mentioned in [`DropErased`].
    #[inline]
    pub unsafe fn with_capacity(layout: Layout, drop: DropErased, capacity: usize) -> Self {
        let mut this = Self::new(layout, drop);
        if capacity == 0 { return this; }

        this.resize(capacity);
        this
    }

    /// Safely constructs a new [`VecErased`] containing `T` with automatic dropping without
    /// pre-allocating the buffer.
    #[inline]
    pub const fn typed<T>() -> Self {
        unsafe { Self::new(Layout::new::<T>(), DropErased::automatic::<T>()) }
    }

    /// Safely constructs a new [`VecErased`] containing `T` with automatic dropping that pre-allocates
    /// the buffer with the size of the given `capacity`.
    #[inline]
    pub fn typed_with_capacity<T>(capacity: usize) -> Self {
        let mut this = Self::typed::<T>();
        if capacity == 0 { return this; }

        this.resize(capacity);
        this
    }

    /// Returns the length (the number of elements) of the vector.
    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Returns the maximum [length](VecErased::len) the vector can hold without a larger reallocation.
    #[inline]
    pub const fn capacity(&self) -> usize {
        if self.layout.size() == 0 {
            usize::MAX
        } else {
            self.cap
        }
    }

    /// Returns the contained item type's size, in bytes. For usage with pointer offsets, see
    /// [`array_stride`](VecErased::array_stride).
    #[inline]
    pub const fn item_size(&self) -> usize {
        self.layout.size()
    }

    /// Returns the pointer offset between an item and the next one, in bytes.
    #[inline]
    pub const fn array_stride(&self) -> usize {
        self.array_stride
    }

    /// Returns the drop implementation this vector uses.
    #[inline]
    pub const fn dropper(&self) -> DropErased {
        self.dropper
    }

    /// Forcibly sets the [length](VecErased::len) of the vector for advanced usages. This does *not*
    /// drop leftover items if the new length is lesser, and exposes *uninitialized* items if the new
    /// length is greater.
    ///
    /// # Safety
    /// Let [`cap`](VecErased::capacity) be the current maximum length, `prev_len` be the current length,
    /// and [`dropper`](VecErased::dropper) be the drop implementation, callers must ensure the following:
    /// - `len` <= `cap`.
    /// - If `dropper` is [`Automatic`](DropErased::Auto) and `len` > `prev_len`, new exposed
    ///   uninitialized items must immediately be written to, most commonly by using
    ///   [`write`](VecErased::write) or [`write_unchecked`](VecErased::write_unchecked).
    #[inline]
    pub unsafe fn set_len(&mut self, len: usize) {
        debug_assert!(len <= self.cap);
        self.len = len;
    }

    /// Returns an untyped immutable pointer to the item at `index`, with bounds-checking.
    #[inline]
    pub fn get(&self, index: usize) -> Option<Ptr> {
        if index < self.len {
            Some(unsafe { self.get_unchecked(index) })
        } else {
            None
        }
    }

    /// Returns an untyped immutable pointer to the item at `index`, without bounds-checking.
    ///
    /// # Safety
    /// `index` must be lesser than [`len`](VecErased::len).
    #[inline]
    pub unsafe fn get_unchecked(&self, index: usize) -> Ptr {
        debug_assert!(index < self.len);
        Ptr::new(NonNull::new_unchecked(self.array.as_ptr().add(index * self.array_stride)))
    }

    /// Returns an untyped mutable pointer to the item at `index`, with bounds-checking.
    #[inline]
    pub fn get_mut(&mut self, index: usize) -> Option<PtrMut> {
        if index < self.len {
            Some(unsafe { self.get_unchecked_mut(index) })
        } else {
            None
        }
    }

    /// Returns an untyped mutable pointer to the item at `index`, without bounds-checking.
    ///
    /// # Safety
    /// `index` must be lesser than [`len`](VecErased::len).
    #[inline]
    pub unsafe fn get_unchecked_mut(&mut self, index: usize) -> PtrMut {
        debug_assert!(index < self.len);
        PtrMut::new(NonNull::new_unchecked(self.array.as_ptr().add(index * self.array_stride)))
    }

    /// Sets the item at `index` and drops the previous item, with bounds-checking.
    #[inline]
    pub unsafe fn set<'a>(&mut self, index: usize, value: PtrOwned<'a>) -> Result<(), PtrOwned<'a>> {
        if index < self.len {
            Ok(self.set_unchecked(index, value))
        } else {
            Err(value)
        }
    }

    /// Sets the item at `index` and drops the previous item, without bounds-checking.
    ///
    /// # Safety
    /// - `index` must be lesser than [`len`](VecErased::len).
    /// - `value` must contain the same data type as the vector contains.
    #[inline]
    pub unsafe fn set_unchecked(&mut self, index: usize, value: PtrOwned) {
        debug_assert!(index < self.len);

        let size = self.layout.size();
        let dropper = self.dropper;
        self.get_unchecked_mut(index)
            .swap(value, size, |prev| if let DropErased::Auto(dropper) = dropper {
                prev.drop_with(dropper)
            });
    }

    /// Swaps an item at `index` with `value`, with bounds-checking.
    ///
    /// # Safety
    /// - `value` must contain the same data type as the vector contains.
    #[inline]
    pub unsafe fn swap<'a, R>(&mut self, index: usize, value: PtrOwned<'a>, prev: impl FnOnce(PtrOwned) -> R) -> Result<R, PtrOwned<'a>> {
        if index < self.len {
            Ok(self.swap_unchecked(index, value, prev))
        } else {
            Err(value)
        }
    }

    /// Swaps an item at `index` with `value`, without bounds-checking.
    ///
    /// # Safety
    /// - `index` must be lesser than [`len`](VecErased::len).
    /// - `value` must contain the same data type as the vector contains.
    #[inline]
    pub unsafe fn swap_unchecked<R>(&mut self, index: usize, value: PtrOwned, prev: impl FnOnce(PtrOwned) -> R) -> R {
        debug_assert!(index < self.len);

        let size = self.layout.size();
        self.get_unchecked_mut(index)
            .swap(value, size, prev)
    }

    /// Overwrites an item at `index` with `value` without running its destructor.
    ///
    /// # Safety
    /// - `value` must contain the same data type as the vector contains.
    #[inline]
    pub unsafe fn write<'a>(&mut self, index: usize, value: PtrOwned<'a>) -> Result<(), PtrOwned<'a>> {
        if index < self.len {
            Ok(self.write_unchecked(index, value))
        } else {
            Err(value)
        }
    }

    /// Overwrites an item at `index` with `value` without running its destructor.
    ///
    /// # Safety
    /// - `index` must be lesser than [`len`](VecErased::len).
    /// - `value` must contain the same data type as the vector contains.
    #[inline]
    pub unsafe fn write_unchecked(&mut self, index: usize, value: PtrOwned) {
        debug_assert!(index < self.len);

        let size = self.layout.size();
        self.get_unchecked_mut(index)
            .write(value, size);
    }

    /// Removes an item at `index` and shifts the rest of the items to fill the empty space.
    ///
    /// # Safety
    /// - `value` must contain the same data type as the vector contains.
    #[inline]
    pub unsafe fn remove<R>(&mut self, index: usize, removed: impl FnOnce(PtrOwned) -> R) -> Option<R> {
        if index < self.len {
            Some(self.remove_unchecked(index, removed))
        } else {
            None
        }
    }

    /// Removes an item at `index` and shifts the rest of the items to fill the empty space.
    ///
    /// # Safety
    /// - `index` must be lesser than [`len`](VecErased::len).
    /// - `value` must contain the same data type as the vector contains.
    #[inline]
    pub unsafe fn remove_unchecked<R>(&mut self, index: usize, removed: impl FnOnce(PtrOwned) -> R) -> R {
        let ret = removed(self.get_unchecked_mut(index).own());

        if index != self.len - 1 {
            let ptr = self.array.as_ptr();
            ptr.add(index * self.array_stride).copy_from(
                ptr.add((index + 1) * self.array_stride),
                (self.len - index - 1) * self.array_stride,
            );
        }

        self.len -= 1;
        ret
    }

    /// Removes an item at `index` and moves the last item to `index` to fill the empty space, if any.
    #[inline]
    pub fn swap_remove<R>(&mut self, index: usize, removed: impl FnOnce(PtrOwned) -> R) -> Option<R> {
        if index < self.len {
            Some(unsafe { self.swap_remove_unchecked(index, removed) })
        } else {
            None
        }
    }

    /// Removes an item at `index` and moves the last item to `index` to fill the empty space, if any.
    ///
    /// # Safety
    /// - `index` must be lesser than [`len`](VecErased::len).
    #[inline]
    pub unsafe fn swap_remove_unchecked<R>(&mut self, index: usize, removed: impl FnOnce(PtrOwned) -> R) -> R {
        let ret = removed(self.get_unchecked_mut(index).own());

        if index != self.len - 1 {
            let ptr = self.array.as_ptr();
            ptr.add(index * self.array_stride).copy_from_nonoverlapping(
                ptr.add((self.len - 1) * self.array_stride),
                self.array_stride,
            );
        }

        self.len -= 1;
        ret
    }

    /// Drops an item at `index` and moves the last item to `index` to fill the empty space, if any.
    #[inline]
    pub fn swap_remove_and_drop(&mut self, index: usize) {
        if index < self.len {
            unsafe { self.swap_remove_unchecked_and_drop(index) }
        }
    }

    /// Drops an item at `index` and moves the last item to `index` to fill the empty space, if any.
    ///
    /// # Safety
    /// - `index` must be lesser than [`len`](VecErased::len).
    #[inline]
    pub unsafe fn swap_remove_unchecked_and_drop(&mut self, index: usize) {
        let dropper = self.dropper;
        self.swap_remove_unchecked(index, |ptr| if let DropErased::Auto(dropper) = dropper {
            ptr.drop_with(dropper);
        });
    }

    /// Pushes an item to the back of the vector.
    ///
    /// # Safety
    /// `value` must contain the same data type as the vector contains.
    #[inline]
    pub unsafe fn push(&mut self, value: PtrOwned) {
        let size = self.layout.size();
        self.reserve(1);
        self.len += 1;
        self.get_unchecked_mut(self.len - 1).write(value, size);
    }

    /// Pops an item from the back of the vector, with bounds-checking. Note that while the function
    /// itself is safe, using the owning pointer passed to `popped` is unsafe.
    #[inline]
    pub fn pop<R>(&mut self, popped: impl FnOnce(PtrOwned) -> R) -> Option<R> {
        if self.len > 0 {
            Some(unsafe { self.pop_unchecked(popped) })
        } else {
            None
        }
    }

    /// Pops an item from the back of the vector, without bounds-checking. Note that while the function
    /// itself is safe asides from the bounds-checking, using the owning pointer passed to `popped`
    /// is unsafe.
    ///
    /// # Safety
    /// [`len`](VecErased::len) must be greater than 0.
    #[inline]
    pub unsafe fn pop_unchecked<R>(&mut self, popped: impl FnOnce(PtrOwned) -> R) -> R {
        let ret = popped(self.get_unchecked_mut(self.len - 1).own());
        self.len -= 1;
        ret
    }

    /// Clears the vector, dropping the items as per the drop implementation.
    #[inline]
    pub fn clear(&mut self) {
        if let DropErased::Auto(dropper) = self.dropper {
            for i in 0..self.len {
                // Safety:
                // - `len` <= `capacity`, so the pointer will always be within the same allocated object.
                // - The buffer size never crosses `isize::MAX`, so the offset never overflows.
                // - Safety requirements on `dropper` is enforced in the constructor.
                unsafe { dropper(self.array.as_ptr().add(i * self.array_stride)) };
            }
        }

        self.len = 0;
    }

    /// Reallocates the buffer such that [`push`](VecErased::push)ing `additional` amount of items will
    /// not cause another reallocation. The resulting [`capacity`](VecErased::capacity) is greater than
    /// or equal to [`len`](VecErased::len) + `additional`, given that a reallocation is actually done.
    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        if additional > self.cap.wrapping_sub(self.len) {
            self.resize((self.cap * 2).max(self.len.checked_add(additional).expect("overflow")).max(if self.array_stride == 1 {
                8
            } else if self.array_stride <= 1024 {
                4
            } else {
                1
            }));
        }
    }

    /// Reallocates the buffer such that [`push`](VecErased::push)ing `additional` amount of items will
    /// not cause another reallocation. The resulting [`capacity`](VecErased::capacity) is equal to
    /// [`len`](VecErased::len) + `additional`, given that a reallocation is actually done.
    #[inline]
    pub fn reserve_exact(&mut self, additional: usize) {
        if additional > self.cap.wrapping_sub(self.len) {
            self.resize(self.len.checked_add(additional).expect("overflow"));
        }
    }

    /// Shrinks the buffer such that [`len`](VecErased::len) is equal to [`capacity`](VecErased::capacity),
    /// dropping the items as per the drop implementation.
    #[inline]
    pub fn shrink_to_fit(&mut self) {
        self.resize(self.len);
    }

    /// Resizes the buffer size to `new_cap`, dropping the items in case of shrinking as per the drop
    /// implementation.
    fn resize(&mut self, new_cap: usize) {
        // Don't bother if the capacity doesn't even change.
        if self.cap == new_cap { return };
        // Simply deallocate if the new capacity is 0.
        if self.cap != 0 && new_cap == 0 {
            // Safety:
            // - Same allocator is used.
            // - `array_layout` is used in the `alloc` of `array`.
            unsafe { dealloc(self.array.as_ptr(), self.array_layout) };

            self.array = NonNull::dangling();
            self.cap = 0;
            return;
        }

        // Don't allocate buffer for ZSTs.
        let size = self.layout.size();
        if size == 0 { return; }

        let (new_array_layout, new_array_stride) = array_layout(self.layout, new_cap);
        let array = if self.cap == 0 {
            // Safety:
            // - ZST-check is done.
            // - `new_array_layout`'s size never overflows `isize::MAX`.
            match NonNull::new(unsafe { alloc(new_array_layout) }) {
                Some(array) => array,
                None => handle_alloc_error(new_array_layout),
            }
        } else {
            if new_cap < self.len {
                if let DropErased::Auto(dropper) = self.dropper {
                    for i in new_cap..self.len {
                        unsafe { dropper(self.array.as_ptr().add(i * self.array_stride)) };
                    }
                }
            }

            // Safety:
            // - Same allocator is used.
            // - `array_layout` is used in the `alloc` of `array`.
            // - `new_array_layout`'s size never overflows `isize::MAX`.
            // - `new_cap` > 0 at this point.
            match NonNull::new(unsafe { realloc(self.array.as_ptr(), self.array_layout, new_array_layout.size()) }) {
                Some(array) => array,
                None => handle_alloc_error(new_array_layout),
            }
        };

        self.array = array;
        self.array_layout = new_array_layout;
        self.array_stride = new_array_stride;
        self.cap = new_cap;
    }
}

impl Drop for VecErased {
    #[inline]
    fn drop(&mut self) {
        if let DropErased::Auto(dropper) = self.dropper {
            for i in 0..self.len {
                // Safety:
                // - `len` <= `capacity`, so the pointer will always be within the same allocated object.
                // - The buffer size never crosses `isize::MAX`, so the offset never overflows.
                // - Safety requirements on `dropper` is enforced in the constructor.
                unsafe { dropper(self.array.as_ptr().add(i * self.array_stride)) };
            }
        }

        if self.layout.size() != 0 && self.cap != 0 {
            // Safety:
            // - Same allocator is used.
            // - `array_layout` is used in the `alloc` of `array`.
            unsafe { dealloc(self.array.as_ptr(), self.array_layout) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::RwLock;

    static GLOBAL: RwLock<usize> = RwLock::new(0);

    #[derive(Debug, Eq, PartialEq)]
    struct Data(usize);
    impl Data {
        #[inline]
        fn new(content: usize) -> Self {
            *GLOBAL.write().unwrap() += 1;
            Self(content)
        }
    }

    impl Clone for Data {
        #[inline]
        fn clone(&self) -> Self {
            Self::new(self.0)
        }
    }

    impl Drop for Data {
        #[inline]
        fn drop(&mut self) {
            *GLOBAL.write().unwrap() -= 1;
        }
    }

    #[test]
    fn soundness() {
        unsafe {
            let mut vec = VecErased::typed::<Data>();
            assert_eq!(vec.len, 0);
            assert_eq!(vec.cap, 0);

            // Convert value to owning pointer.
            PtrOwned::take(Data::new(314), |ptr| vec.push(ptr));
            PtrOwned::take(Data::new(159), |ptr| vec.push(ptr));
            PtrOwned::take(Data::new(69), |ptr| vec.push(ptr));
            PtrOwned::take(Data::new(420), |ptr| vec.push(ptr));

            // Length and capacity check.
            assert_eq!(vec.len, 4);
            assert!(vec.cap >= 4);

            // Element check.
            assert_eq!(vec.get(0).unwrap().deref::<Data>(), &Data::new(314));
            assert_eq!(vec.get_mut(1).unwrap().deref_mut::<Data>(), &mut Data::new(159));
            assert_eq!(vec.get_unchecked(2).deref::<Data>(), &Data::new(69));
            assert_eq!(vec.get_unchecked_mut(3).deref_mut::<Data>(), &mut Data::new(420));

            // Removal check.
            assert_eq!(vec.remove(0, |ptr| ptr.read::<Data>()).unwrap(), Data::new(314));
            assert_eq!(vec.swap_remove(1, |ptr| ptr.read::<Data>()).unwrap(), Data::new(69));

            // Length, capacity, and drop check.
            assert_eq!(vec.len, 2);
            assert!(vec.cap >= 2);
            assert_eq!(*GLOBAL.read().unwrap(), 2);

            // Shrink check.
            vec.shrink_to_fit();
            assert_eq!(vec.cap, vec.len);
            assert_eq!(*GLOBAL.read().unwrap(), 2);

            drop(vec);
            assert_eq!(*GLOBAL.read().unwrap(), 0);
        }
    }
}
