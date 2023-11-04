use std::{
    ptr::NonNull,
    marker::PhantomData,
    mem::ManuallyDrop,
};

#[must_use = "read() or drop_as() it to avoid memory leak"]
pub struct PtrOwned<'a> {
    ptr: NonNull<u8>,
    _marker: PhantomData<&'a mut u8>,
}

impl<'a> PtrOwned<'a> {
    #[inline]
    pub fn take<T, R>(value: T, acceptor: impl FnOnce(PtrOwned) -> R) -> R {
        let mut value = ManuallyDrop::new(value);
        acceptor(unsafe { PtrMut::from(&mut *value).own() })
    }

    #[inline]
    pub unsafe fn new(ptr: NonNull<u8>) -> Self {
        Self {
            ptr,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub unsafe fn read<T>(self) -> T {
        self.ptr.cast::<T>().as_ptr().read()
    }

    #[inline]
    pub unsafe fn drop_as<T>(self) {
        self.ptr.cast::<T>().as_ptr().drop_in_place();
    }

    #[inline]
    pub unsafe fn drop_with(self, dropper: unsafe fn(*mut u8)) {
        dropper(self.ptr.as_ptr());
    }

    #[inline]
    pub unsafe fn deref<T>(&self) -> &T {
        self.ptr.cast::<T>().as_ref()
    }

    #[inline]
    pub unsafe fn deref_mut<T>(&mut self) -> &mut T {
        self.ptr.cast::<T>().as_mut()
    }

    #[inline]
    pub unsafe fn byte_add(self, add: usize) -> Self {
        Self::new(NonNull::new_unchecked(self.ptr.as_ptr().add(add)))
    }

    #[inline]
    pub unsafe fn byte_offset(self, offset: isize) -> Self {
        Self::new(NonNull::new_unchecked(self.ptr.as_ptr().offset(offset)))
    }

    #[inline]
    pub fn as_ref(&mut self) -> Ptr {
        unsafe { Ptr::new(self.ptr) }
    }

    #[inline]
    pub fn as_mut(&mut self) -> PtrMut {
        unsafe { PtrMut::new(self.ptr) }
    }
}

pub struct PtrMut<'a> {
    ptr: NonNull<u8>,
    _marker: PhantomData<&'a mut u8>,
}

impl<'a> PtrMut<'a> {
    #[inline]
    pub unsafe fn new(ptr: NonNull<u8>) -> Self {
        Self {
            ptr,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub unsafe fn own(self) -> PtrOwned<'a> {
        PtrOwned::new(self.ptr)
    }

    #[inline]
    pub unsafe fn swap<R>(&mut self, value: PtrOwned, size: usize, prev: impl FnOnce(PtrOwned) -> R) -> R {
        let ret = prev(PtrOwned::new(self.ptr));
        self.write(value, size);
        ret
    }

    #[inline]
    pub unsafe fn drop_in_place_as<T>(&mut self) {
        self.ptr.cast::<T>().as_ptr().drop_in_place()
    }

    #[inline]
    pub unsafe fn drop_in_place_with(&mut self, dropper: unsafe fn(*mut u8)) {
        dropper(self.ptr.as_ptr());
    }

    #[inline]
    pub unsafe fn write(&mut self, value: PtrOwned, size: usize) {
        self.ptr.as_ptr().copy_from_nonoverlapping(value.ptr.as_ptr(), size);
    }

    #[inline]
    pub unsafe fn deref<T>(&self) -> &T {
        self.ptr.cast::<T>().as_ref()
    }

    #[inline]
    pub unsafe fn deref_mut<T>(&mut self) -> &mut T {
        self.ptr.cast::<T>().as_mut()
    }

    #[inline]
    pub unsafe fn byte_add(self, add: usize) -> Self {
        Self::new(NonNull::new_unchecked(self.ptr.as_ptr().add(add)))
    }

    #[inline]
    pub unsafe fn byte_offset(self, offset: isize) -> Self {
        Self::new(NonNull::new_unchecked(self.ptr.as_ptr().offset(offset)))
    }

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

#[derive(Copy, Clone)]
pub struct Ptr<'a> {
    ptr: NonNull<u8>,
    _marker: PhantomData<&'a u8>,
}

impl<'a> Ptr<'a> {
    #[inline]
    pub unsafe fn new(ptr: NonNull<u8>) -> Self {
        Self {
            ptr,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub unsafe fn deref<T>(&self) -> &T {
        self.ptr.cast::<T>().as_ref()
    }

    #[inline]
    pub unsafe fn byte_add(self, add: usize) -> Self {
        Self::new(NonNull::new_unchecked(self.ptr.as_ptr().add(add)))
    }

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
