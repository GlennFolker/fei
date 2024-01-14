use crate::{
    ptr::{
        Ptr, PtrMut, PtrOwned,
    },
    drop_for,
};
use std::{
    alloc::{
        Layout,
        alloc, dealloc,
        handle_alloc_error,
    },
    marker::PhantomData,
    mem::ManuallyDrop,
    ptr::NonNull,
};

pub struct BoxErased<'a> {
    ptr: NonNull<u8>,
    layout: Layout,
    dropper: Option<unsafe fn(*mut u8)>,

    _marker: PhantomData<&'a mut u8>,
}

impl<'a> BoxErased<'a> {
    #[inline]
    pub unsafe fn new<'t: 'a>(value: PtrOwned<'t>, layout: Layout, dropper: Option<unsafe fn(*mut u8)>) -> Self {
        if layout.size() == 0 {
            return Self {
                ptr: NonNull::dangling(),
                layout,
                dropper,

                _marker: PhantomData,
            }
        }

        Self {
            ptr: match NonNull::new(alloc(layout)) {
                Some(ptr) => {
                    PtrMut::new(ptr).write(value, layout.size());
                    ptr
                },
                None => handle_alloc_error(layout),
            },
            layout,
            dropper,

            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn typed<T: 'a>(value: T) -> Self {
        PtrOwned::take(value, |ptr| unsafe { Self::new(ptr, Layout::new::<T>(), drop_for::<T>()) })
    }

    #[inline]
    pub fn take<R>(self, acceptor: impl FnOnce(PtrOwned<'a>) -> R) -> R {
        if self.layout.size() == 0 {
            return acceptor(unsafe { PtrOwned::new(NonNull::dangling()) });
        }

        let this = ManuallyDrop::new(self);
        let ret = acceptor(unsafe { PtrOwned::new(this.ptr) });

        unsafe { dealloc(this.ptr.as_ptr(), this.layout) };
        ret
    }

    #[inline]
    pub unsafe fn cast<T: 'a>(self) -> T {
        self.take(|ptr| unsafe { ptr.read() })
    }

    #[inline]
    pub fn borrow(&self) -> Ptr {
        unsafe { Ptr::new(self.ptr) }
    }

    #[inline]
    pub fn borrow_mut(&mut self) -> PtrMut {
        unsafe { PtrMut::new(self.ptr) }
    }

    #[inline]
    pub unsafe fn deref<T: 'a>(&self) -> &T {
        self.borrow().deref()
    }

    #[inline]
    pub unsafe fn deref_mut<T: 'a>(&mut self) -> &mut T {
        self.borrow_mut().deref_mut()
    }
}

impl<'a> Drop for BoxErased<'a> {
    #[inline]
    fn drop(&mut self) {
        let ptr = self.ptr.as_ptr();
        unsafe {
            if let Some(dropper) = self.dropper {
                dropper(ptr);
            }

            if self.layout.size() != 0 {
                dealloc(ptr, self.layout);
            }
        }
    }
}

pub trait OptionExt<'a> {
    unsafe fn casted<T: 'a>(self) -> Option<T>;
}

impl<'a> OptionExt<'a> for Option<BoxErased<'a>> {
    #[inline]
    unsafe fn casted<T: 'a>(self) -> Option<T> {
        match self {
            Some(value) => Some(value.cast()),
            None => None,
        }
    }
}
