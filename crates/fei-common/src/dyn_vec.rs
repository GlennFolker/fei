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

/// An unsafe statically-unknown homogenous list data container, similar to [`Vec`](Vec).
pub struct DynVec {
    array: NonNull<u8>,
    layout: Layout,
    array_layout: Layout,
    array_stride: usize,
    dropper: Option<unsafe fn(*mut u8)>,

    len: usize,
    cap: usize,
}

impl DynVec {
    #[inline]
    pub const unsafe fn new(layout: Layout, drop: Option<unsafe fn(*mut u8)>) -> Self {
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

    #[inline]
    pub const fn typed<T>() -> Self {
        unsafe { Self::new(Layout::new::<T>(), drop_for::<T>()) }
    }

    #[inline]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub const fn capacity(&self) -> usize {
        if self.layout.size() == 0 {
            usize::MAX
        } else {
            self.cap
        }
    }

    #[inline]
    pub const fn item_size(&self) -> usize {
        self.layout.size()
    }

    #[inline]
    pub const fn array_stride(&self) -> usize {
        self.array_stride
    }

    #[inline]
    pub fn set_len(&mut self, len: usize) {
        self.len = len;
    }

    #[inline]
    pub fn get(&self, index: usize) -> Option<Ptr> {
        (index < self.len).then(|| unsafe { self.get_unchecked(index) })
    }

    #[inline]
    pub unsafe fn get_unchecked(&self, index: usize) -> Ptr {
        Ptr::new(NonNull::new_unchecked(self.array.as_ptr().add(index * self.array_stride)))
    }

    #[inline]
    pub fn get_mut(&mut self, index: usize) -> Option<PtrMut> {
        (index < self.len).then(|| unsafe { self.get_unchecked_mut(index) })
    }

    #[inline]
    pub unsafe fn get_unchecked_mut(&mut self, index: usize) -> PtrMut {
        PtrMut::new(NonNull::new_unchecked(self.array.as_ptr().add(index * self.array_stride)))
    }

    #[inline]
    pub unsafe fn swap<R>(&mut self, index: usize, value: PtrOwned, prev: impl FnOnce(PtrOwned) -> R) -> Option<R> {
        (index < self.len).then(|| self.swap_unchecked(index, value, prev))
    }

    #[inline]
    pub unsafe fn swap_unchecked<R>(&mut self, index: usize, value: PtrOwned, prev: impl FnOnce(PtrOwned) -> R) -> R {
        let size = self.layout.size();
        self.get_unchecked_mut(index)
            .swap(value, size, prev)
    }

    #[inline]
    pub unsafe fn push(&mut self, value: PtrOwned) {
        let size = self.layout.size();
        self.reserve(1);
        self.get_unchecked_mut(self.len).write(value, size);
        self.len += 1;
    }

    #[inline]
    pub unsafe fn remove<R>(&mut self, index: usize, removed: impl FnOnce(PtrOwned) -> R) -> Option<R> {
        (index < self.len).then(|| self.remove_unchecked(index, removed))
    }

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

    #[inline]
    pub unsafe fn swap_remove<R>(&mut self, index: usize, removed: impl FnOnce(PtrOwned) -> R) -> Option<R> {
        (index < self.len).then(|| self.swap_remove_unchecked(index, removed))
    }

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

    #[inline]
    pub fn shrink_to_fit(&mut self) {
        self.resize(self.len);
    }

    fn resize(&mut self, new_cap: usize) {
        if self.cap == new_cap { return };
        if self.cap != 0 && new_cap == 0 {
            unsafe { dealloc(self.array.as_ptr(), self.array_layout) };

            self.array = NonNull::dangling();
            self.cap = 0;
        }

        let size = self.layout.size();
        if size == 0 { return; }

        let (new_array_layout, new_array_stride) = array_layout(self.layout, new_cap);
        let array = if self.cap == 0 {
            match NonNull::new(unsafe { alloc(new_array_layout) }) {
                Some(array) => array,
                None => handle_alloc_error(new_array_layout),
            }
        } else {
            if new_cap < self.len {
                if let Some(dropper) = self.dropper {
                    for i in new_cap..self.len {
                        unsafe { dropper(self.array.as_ptr().add(i * self.array_stride)) };
                    }
                }
            }

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

impl Drop for DynVec {
    #[inline]
    fn drop(&mut self) {
        if let Some(dropper) = self.dropper {
            for i in 0..self.len {
                unsafe { dropper(self.array.as_ptr().add(i * self.array_stride)) };
            }
        }

        if self.layout.size() != 0 && self.cap != 0 {
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
            let mut vec = DynVec::typed::<Data>();
            assert_eq!(vec.len, 0);
            assert_eq!(vec.cap, 0);

            // Convert value to owning pointer.
            PtrOwned::take(Data::new(314), |ptr| vec.push(ptr));
            PtrOwned::take(Data::new(159), |ptr| vec.push(ptr));
            PtrOwned::take(Data::new(69), |ptr| vec.push(ptr));
            PtrOwned::take(Data::new(420), |ptr| vec.push(ptr));
            PtrOwned::take(Data::new(281), |ptr| vec.push(ptr));

            // Length and capacity check.
            assert_eq!(vec.len, 5);
            assert!(vec.cap >= 5);

            // Element check.
            assert_eq!(vec.get(0).unwrap().deref::<Data>(), &Data::new(314));
            assert_eq!(vec.get(1).unwrap().deref::<Data>(), &Data::new(159));
            assert_eq!(vec.get(2).unwrap().deref::<Data>(), &Data::new(69));
            assert_eq!(vec.get(3).unwrap().deref::<Data>(), &Data::new(420));
            assert_eq!(vec.get(4).unwrap().deref::<Data>(), &Data::new(281));

            // Removal check.
            assert_eq!(vec.remove(0, |ptr| ptr.read::<Data>()).unwrap(), Data::new(314));
            assert_eq!(vec.swap_remove(1, |ptr| ptr.read::<Data>()).unwrap(), Data::new(69));

            // Length, capacity, and drop check.
            assert_eq!(vec.len, 3);
            assert!(vec.cap >= 3);
            assert_eq!(*GLOBAL.read().unwrap(), 3);

            // Shrink check.
            vec.shrink_to_fit();
            assert_eq!(vec.cap, vec.len);
            assert_eq!(*GLOBAL.read().unwrap(), 3);
        }
    }
}
