//! Provides a safer way to deal with raw pointers through [`PtrOwned`], [`PtrMut`], and [`Ptr`]. Refer
//! to the documentations of these structs for further information.

use std::{
    ptr::NonNull,
    marker::PhantomData,
    mem::ManuallyDrop,
};

/// Represents an untyped thin-pointer that logically owns the data over the lifetime `'a`. This pointer
/// is responsible for calling the data's drop implementation, but *not* deallocation. This pointer is
/// mentally equivalent to [`ManuallyDrop<T>`].
///
/// # Examples
/// Safely constructing a `PtrOwned` from an owned value is done by using [`take`](PtrOwned::take):
/// ```
/// use fei_common::ptr::PtrOwned;
///
/// struct Data(u32);
///
/// let my_data = Data(314);
/// PtrOwned::take(my_data, |ptr| {
///     // `my_data` is now logically moved to this closure as `ptr`, which allows:
///     // - Transferring the ownership of `ptr` into another function, such as `DynVec::push`.
///     // - Transmuting it back by calling `ptr.read::<Data>()`, consuming the `ptr`.
///     // - Dropping it by calling `ptr.drop_as::<Data>()`, consuming the `ptr`.
///     // Note that *not* doing any of these will not drop the owned data, causing a memory leak.
/// });
/// ```
/// Functions that receive ownership of a `PtrOwned` from the caller take a by-value parameter, and
/// potentially returning it back on a failure case:
/// ```
/// use fei_common::ptr::PtrOwned;
///
/// struct Data(u32);
/// impl Drop for Data {
///     fn drop(&mut self) {
///         println!("Dropped {}!", self.0);
///     }
/// }
///
/// unsafe fn accept_no_fail(ptr: PtrOwned) {
///     // Consume `ptr` by reading it into a statically known type.
///     let data = ptr.read::<Data>();
///     println!("Accepted {}!", data.0);
/// }
///
/// unsafe fn accept_may_fail(ptr: PtrOwned, fail: bool) -> Result<(), PtrOwned> {
///     if fail {
///         // Return the unconsumed `ptr` to give back ownership.
///         Err(ptr)
///     } else {
///         // Consume `ptr` by dropping it as a statically known type.
///         ptr.drop_as::<Data>();
///         Ok(())
///     }
/// }
///
/// PtrOwned::take(Data(314), |ptr| unsafe { accept_no_fail(ptr) });
/// // Prints "Accepted 314!".
/// // Prints "Dropped 314!".
///
/// PtrOwned::take(Data(159), |ptr| unsafe {
///     if let Err(returned_ptr) = accept_may_fail(ptr, true) {
///         println!("Failed {}!", returned_ptr.read::<Data>().0);
///     }
/// });
/// // Prints "Failed 159!"
/// // Prints "Dropped 159!"
/// ```
/// In contrast, functions that grants the caller an ownership of a `PtrOwned` don't simply return it,
/// but instead requires the caller to pass a callback to consume the owning pointer. This is because
/// we can't use ordinary move semantics, since the value type itself is statically unknown and can't
/// be stored on the stack:
/// ```
/// use fei_common::ptr::PtrOwned;
///
/// struct Data(u32);
///
/// fn supply<R>(callback: impl FnOnce(PtrOwned) -> R) -> R {
///     // - `my_data` itself lives inside the scope of `PtrOwned::take(...)`.
///     // - `my_data`'s destructor won't be run, however, and `callback` receives the pointer to it
///     //   just before it goes out of scope.
///     // - It is now `callback`'s responsibility to actually manage `my_data`.
///     let my_data = Data(314);
///     PtrOwned::take(my_data, callback)
/// }
///
/// let num = supply(|ptr| unsafe { ptr.read::<Data>() }.0);
/// assert_eq!(num, 314);
/// ```
#[must_use = "read or drop it to avoid memory leak"]
pub struct PtrOwned<'a> {
    ptr: NonNull<u8>,
    _marker: PhantomData<&'a mut u8>,
}

impl<'a> PtrOwned<'a> {
    /// Takes a by-value parameter and passes a pointer owning that parameter into a callback,
    /// ensuring no use-after-frees.
    #[inline]
    pub fn take<T: 'a, R>(value: T, acceptor: impl FnOnce(PtrOwned<'a>) -> R) -> R {
        // Don't call the destructor, as the data is logically moved to `acceptor`.
        let mut value = ManuallyDrop::new(value);
        // Safety:
        // - The value outlives the owning pointer.
        // - `take` owns `value`.
        acceptor(unsafe { PtrMut::from(&mut *value).own() })
    }

    /// Arbitrarily creates an `OwningPtr` from a pointer.
    ///
    /// # Safety
    /// Given `T` as the actual value type, callers must ensure the following:
    /// - The scope must logically own the pointed-to value, in the sense of nothing else currently
    ///   or in the future may obtain a reference to the value.
    /// - The original value mustn't be dropped, preferably by wrapping it inside [`ManuallyDrop`].
    /// - The resulting `PtrOwned` mustn't live longer than the pointed-to value; it must be consumed
    ///   before the original value goes out of scope.
    /// - `ptr` must point to an initialized instance of `T`.
    /// - `ptr` must be valid for both reads and writes.
    /// - `ptr` must be properly aligned to the alignment of `T`.
    #[inline]
    pub unsafe fn new(ptr: NonNull<u8>) -> Self {
        Self {
            ptr,
            _marker: PhantomData,
        }
    }

    /// Consumes the pointer and reads it as `T`.
    ///
    /// # Safety
    /// The actual type of the pointed-to value must be `T`.
    #[inline]
    pub unsafe fn read<T: 'a>(self) -> T {
        self.ptr.cast::<T>().as_ptr().read()
    }

    /// Consumes the pointer and drops it as `T`.
    ///
    /// # Safety
    /// The actual type of the pointed-to value must be `T`.
    #[inline]
    pub unsafe fn drop_as<T: 'a>(self) {
        self.ptr.cast::<T>().as_ptr().drop_in_place();
    }

    /// Consumes the pointer and supplies it into a dropper callback to be dropped.
    ///
    /// # Safety
    /// `dropper` must *only* read or drop the pointer in-place as whatever the actual type of the
    /// pointed-to value is.
    #[inline]
    pub unsafe fn drop_with(self, dropper: unsafe fn(*mut u8)) {
        dropper(self.ptr.as_ptr());
    }

    /// Immutably dereferences the pointer as `&T`.
    ///
    /// # Safety
    /// The actual type of the pointed-to value must be `T`.
    #[inline]
    pub unsafe fn deref<T: 'a>(&self) -> &'a T {
        self.ptr.cast::<T>().as_ref()
    }

    /// Mutably dereferences the pointer as `&mut T`.
    ///
    /// # Safety
    /// The actual type of the pointed-to value must be `T`.
    #[inline]
    pub unsafe fn deref_mut<T: 'a>(&mut self) -> &'a mut T {
        self.ptr.cast::<T>().as_mut()
    }

    /// Calculates the offset from the pointer, in bytes. See
    /// [ptr::add](https://doc.rust-lang.org/std/primitive.pointer.html#method.add) for usage and
    /// safety concerns.
    #[inline]
    pub unsafe fn byte_add(self, add: usize) -> Self {
        Self::new(NonNull::new_unchecked(self.ptr.as_ptr().add(add)))
    }

    /// Calculates the offset from the pointer, in bytes. See
    /// [ptr::offset](https://doc.rust-lang.org/std/primitive.pointer.html#method.offset) for usage and
    /// safety concerns.
    #[inline]
    pub unsafe fn byte_offset(self, offset: isize) -> Self {
        Self::new(NonNull::new_unchecked(self.ptr.as_ptr().offset(offset)))
    }

    /// Immutably borrows the owning pointer as [`Ptr`].
    #[inline]
    pub fn as_ref(&mut self) -> Ptr {
        // Safety: The pointer is owned, so may be borrowed.
        unsafe { Ptr::new(self.ptr) }
    }

    /// Mutably borrows the owning pointer as [`PtrMut`].
    #[inline]
    pub fn as_mut(&mut self) -> PtrMut {
        // Safety: The pointer is owned, so may be borrowed.
        unsafe { PtrMut::new(self.ptr) }
    }
}

/// Represents an untyped thin-pointer that logically mutably references the data over the lifetime
/// `'a`. This pointer is mentally equivalent to [`&mut MaybeUninit<T>`](std::mem::MaybeUninit).
pub struct PtrMut<'a> {
    ptr: NonNull<u8>,
    _marker: PhantomData<&'a mut u8>,
}

impl<'a> PtrMut<'a> {
    /// Arbitrarily creates an `PtrMut` from a pointer.
    ///
    /// # Safety
    /// Given `T` as the actual value type, callers must ensure the following:
    /// - There may not be other references (including `PtrMut` and [`Ptr`]) to the given pointed-to
    ///   value. [`PtrOwned`] is allowed only if there is only one instance, and this function is
    ///   called from [`PtrOwned::as_mut`].
    /// - The resulting `PtrMut` mustn't live longer than the pointed-to value; it must be consumed
    ///   before the original value goes out of scope.
    /// - `ptr` must point to a (maybe uninitialized) instance of `T`.
    /// - `ptr` must be valid for both reads and writes.
    /// - `ptr` must be properly aligned to the alignment of `T`.
    #[inline]
    pub unsafe fn new(ptr: NonNull<u8>) -> Self {
        Self {
            ptr,
            _marker: PhantomData,
        }
    }

    /// Claims ownership of the pointed-to value.
    ///
    /// # Safety
    /// Refer to the safety guidelines mentioned in [PtrOwned::new].
    #[inline]
    pub unsafe fn own<'t: 'a>(self) -> PtrOwned<'t> {
        PtrOwned::new(self.ptr)
    }

    /// Drops the pointed-to value in-place as `T`, leaving the value in an *uninitialized* state.
    ///
    /// # Safety
    /// The pointer must point to an initialized instance of `T`.
    #[inline]
    pub unsafe fn drop_in_place_as<T: 'a>(&mut self) {
        self.ptr.cast::<T>().as_ptr().drop_in_place()
    }

    /// Drops the pointed-value in-place with the given drop implementation, leaving the value in an
    /// *uninitialized* state.
    ///
    /// # Safety
    /// Given `T` as the actual value type, callers must ensure the following:
    /// - The pointer must point to an initialized instance of `T`.
    /// - `dropper` must *only* read or drop the pointer in-place as `T`.
    #[inline]
    pub unsafe fn drop_in_place_with(&mut self, dropper: unsafe fn(*mut u8)) {
        dropper(self.ptr.as_ptr());
    }

    /// Overwrites the pointed-to value with the given new value, without dropping the previous value.
    ///
    /// # Safety
    /// Given `T` as the actual value type, callers must ensure the following:
    /// - This pointer and `new_value` must point to an instance of `T`.
    /// - `size` must be equal to [`size_of::<T>()`](std::mem::size_of).
    #[inline]
    pub unsafe fn write<'t: 'a>(&mut self, new_value: PtrOwned<'t>, size: usize) {
        self.ptr.as_ptr().copy_from_nonoverlapping(new_value.ptr.as_ptr(), size);
    }

    /// Swaps the pointed-to value with the given new value.
    ///
    /// # Safety
    /// Given `T` as the actual value type, callers must ensure the following:
    /// - This pointer must point to an initialized instance of `T`.
    /// - This pointer and `new_value` must point to an instance of `T`.
    /// - `size` must be equal to [`size_of::<T>()`](std::mem::size_of).
    #[inline]
    pub unsafe fn swap<'t: 'a, R>(&mut self, new_value: PtrOwned<'t>, size: usize, prev: impl FnOnce(PtrOwned<'t>) -> R) -> R {
        let ret = prev(PtrOwned::new(self.ptr));
        self.write(new_value, size);
        ret
    }

    /// Immutably dereferences the pointer as `&T`.
    ///
    /// # Safety
    /// This pointer must point to an initialized instance of `T`.
    #[inline]
    pub unsafe fn deref<T: 'a>(&self) -> &'a T {
        self.ptr.cast::<T>().as_ref()
    }

    /// Mutably dereferences the pointer as `&mut T`.
    ///
    /// # Safety
    /// This pointer must point to an initialized instance of `T`.
    #[inline]
    pub unsafe fn deref_mut<T: 'a>(&mut self) -> &'a mut T {
        self.ptr.cast::<T>().as_mut()
    }

    /// Calculates the offset from the pointer, in bytes. See
    /// [ptr::add](https://doc.rust-lang.org/std/primitive.pointer.html#method.add) for usage and
    /// safety concerns.
    #[inline]
    pub unsafe fn byte_add(self, add: usize) -> Self {
        Self::new(NonNull::new_unchecked(self.ptr.as_ptr().add(add)))
    }

    /// Calculates the offset from the pointer, in bytes. See
    /// [ptr::offset](https://doc.rust-lang.org/std/primitive.pointer.html#method.offset) for usage and
    /// safety concerns.
    #[inline]
    pub unsafe fn byte_offset(self, offset: isize) -> Self {
        Self::new(NonNull::new_unchecked(self.ptr.as_ptr().offset(offset)))
    }

    /// Immutably re-borrows the pointer as [`Ptr`].
    #[inline]
    pub fn as_ref(&mut self) -> Ptr {
        unsafe { Ptr::new(self.ptr) }
    }
}

impl<'a, T> From<&'a mut T> for PtrMut<'a> {
    #[inline]
    fn from(value: &'a mut T) -> Self {
        unsafe { Self::new(NonNull::from(value).cast()) }
    }
}

/// Represents an untyped thin-pointer that logically immutably references the data over the lifetime
/// `'a`. This pointer is mentally equivalent to `&T`.
#[derive(Copy, Clone)]
pub struct Ptr<'a> {
    ptr: NonNull<u8>,
    _marker: PhantomData<&'a u8>,
}

impl<'a> Ptr<'a> {
    /// Arbitrarily creates an `Ptr` from a pointer.
    ///
    /// # Safety
    /// Given `T` as the actual value type, callers must ensure the following:
    /// - There may not be other mutable references to the given pointed-to value. [`PtrOwned`] or
    ///   [`PtrMut`] is allowed only if there is only one instance, and this function is called from
    ///   [`PtrOwned::as_ref`] or [`PtrMut::as_ref`].
    /// - The resulting `Ptr` mustn't live longer than the pointed-to value; it must be consumed
    ///   before the original value goes out of scope.
    /// - `ptr` must point to an initialized instance of `T`.
    /// - `ptr` must be valid for both reads and writes.
    /// - `ptr` must be properly aligned to the alignment of `T`.
    #[inline]
    pub unsafe fn new(ptr: NonNull<u8>) -> Self {
        Self {
            ptr,
            _marker: PhantomData,
        }
    }

    /// Immutably dereferences the pointer as `&T`.
    ///
    /// # Safety
    /// The actual type of the pointed-to value must be `T`.
    #[inline]
    pub unsafe fn deref<T: 'a>(&self) -> &'a T {
        self.ptr.cast::<T>().as_ref()
    }

    /// Calculates the offset from the pointer, in bytes. See
    /// [ptr::add](https://doc.rust-lang.org/std/primitive.pointer.html#method.add) for usage and
    /// safety concerns.
    #[inline]
    pub unsafe fn byte_add(self, add: usize) -> Self {
        Self::new(NonNull::new_unchecked(self.ptr.as_ptr().add(add)))
    }

    /// Calculates the offset from the pointer, in bytes. See
    /// [ptr::offset](https://doc.rust-lang.org/std/primitive.pointer.html#method.offset) for usage and
    /// safety concerns.
    #[inline]
    pub unsafe fn byte_offset(self, offset: isize) -> Self {
        Self::new(NonNull::new_unchecked(self.ptr.as_ptr().offset(offset)))
    }
}

impl<'a, T> From<&'a mut T> for Ptr<'a> {
    #[inline]
    fn from(value: &'a mut T) -> Self {
        unsafe { Self::new(NonNull::from(value).cast()) }
    }
}

impl<'a, T> From<&'a T> for Ptr<'a> {
    #[inline]
    fn from(value: &'a T) -> Self {
        unsafe { Self::new(NonNull::from(value).cast()) }
    }
}

pub trait OptionPtrMutExt<'a>: OptionPtrExt<'a> {
    unsafe fn ptr_deref_mut<T: 'a>(self) -> Option<&'a mut T>;
}

pub trait OptionPtrExt<'a> {
    unsafe fn ptr_deref<T: 'a>(self) -> Option<&'a T>;
}

impl<'a> OptionPtrExt<'a> for Option<PtrMut<'a>> {
    #[inline]
    unsafe fn ptr_deref<T: 'a>(self) -> Option<&'a T> {
        match self {
            Some(ptr) => Some(ptr.deref()),
            None => None,
        }
    }
}

impl<'a> OptionPtrMutExt<'a> for Option<PtrMut<'a>> {
    #[inline]
    unsafe fn ptr_deref_mut<T: 'a>(self) -> Option<&'a mut T> {
        match self {
            Some(mut ptr) => Some(ptr.deref_mut()),
            None => None,
        }
    }
}

impl<'a> OptionPtrExt<'a> for Option<Ptr<'a>> {
    #[inline]
    unsafe fn ptr_deref<T: 'a>(self) -> Option<&'a T> {
        match self {
            Some(ptr) => Some(ptr.deref()),
            None => None,
        }
    }
}
