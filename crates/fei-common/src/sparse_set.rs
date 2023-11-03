use fixedbitset::{
    FixedBitSet, Ones,
};
use std::{
    marker::PhantomData,
    mem::{
        ManuallyDrop, MaybeUninit,
    },
    ops::{
        Index, IndexMut,
    },
    ptr,
};

pub trait SparseIndex {
    fn into_index(self) -> usize;
    fn from_index(index: usize) -> Self;
}

macro_rules! impl_sparse_index {
    ($name:ty) => {
        impl SparseIndex for $name {
            #[inline]
            fn into_index(self) -> usize {
                return self as usize;
            }

            #[inline]
            fn from_index(index: usize) -> Self {
                return index as Self;
            }
        }
    };
}

impl_sparse_index!(u8);
impl_sparse_index!(u16);
impl_sparse_index!(u32);
impl_sparse_index!(u64);
impl_sparse_index!(usize);

pub struct SparseSet<I: SparseIndex, T> {
    dense: FixedBitSet,
    sparse: Vec<MaybeUninit<T>>,
    len: usize,
    _marker: PhantomData<I>,
}

impl<I: SparseIndex, T> SparseSet<I, T> {
    #[inline]
    pub const fn new() -> Self {
        Self {
            dense: FixedBitSet::new(),
            sparse: Vec::new(),
            len: 0,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn insert(&mut self, index: I, value: T) -> Option<T> {
        let index = index.into_index();
        if self.dense.contains(index) {
            // Safety: If the key exists, then the value exists and is initialized.
            Some(unsafe {
                let stored = self.sparse.get_unchecked_mut(index);
                let prev = stored.assume_init_read();

                stored.write(value);
                prev
            })
        } else {
            self.len += 1;
            self.dense.grow(index + 1);
            self.dense.set(index, true);

            let sparse_len = self.sparse.len();
            if sparse_len <= index {
                self.sparse.reserve(index - sparse_len + 1);
                // Safety:
                // - Length fits the allocated memory; note the call to `reserve()` before.
                // - It is okay for the new elements to be uninitialized, as per `MaybeUninit<T>`.
                unsafe { self.sparse.set_len(index + 1) };
            }

            // Safety: Sparse container is ensured to contain uninitialized value at `index`.
            unsafe { self.sparse.get_unchecked_mut(index).write(value) };
            None
        }
    }

    pub fn remove(&mut self, index: I) -> Option<T> {
        let index = index.into_index();
        self.dense.contains(index)
            .then(|| {
                self.len -= 1;
                self.dense.set(index, false);

                // Safety: If the key exists, then the value exists and is initialized.
                unsafe { self.sparse.get_unchecked(index).assume_init_read() }
            })
    }

    #[inline]
    pub fn contains(&self, index: I) -> bool {
        let index = index.into_index();
        self.dense.contains(index)
    }

    #[inline]
    pub fn get(&self, index: I) -> Option<&T> {
        let index = index.into_index();
        self.dense
            .contains(index)
            // Safety: If the key exists, then the value exists and is initialized.
            .then(|| unsafe { self.sparse.get_unchecked(index).assume_init_ref() })
    }

    #[inline]
    pub fn get_mut(&mut self, index: I) -> Option<&mut T> {
        let index = index.into_index();
        self.dense
            .contains(index)
            // Safety: If the key exists, then the value exists and is initialized.
            .then(|| unsafe { self.sparse.get_unchecked_mut(index).assume_init_mut() })
    }

    #[inline]
    pub unsafe fn get_unchecked(&self, index: I) -> &T {
        let index = index.into_index();
        // Safety: Whether the key exists is upheld by the caller.
        self.sparse.get_unchecked(index).assume_init_ref()
    }

    #[inline]
    pub unsafe fn get_unchecked_mut(&mut self, index: I) -> &mut T {
        let index = index.into_index();
        // Safety: Whether the key exists is upheld by the caller.
        self.sparse.get_unchecked_mut(index).assume_init_mut()
    }

    #[inline]
    pub fn shrink_to_fit(&mut self) {
        let len = match self.dense.ones().last() {
            Some(pos) => pos + 1,
            None => 0,
        };

        // Safety: Anything beyond [0, `len`) is uninitialized and can be shrunk.
        unsafe { self.sparse.set_len(len) };
        self.sparse.shrink_to_fit();
    }

    #[inline]
    pub fn iter(&self) -> Iter<I, T> {
        Iter {
            dense: self.dense.ones(),
            sparse: self.sparse.as_ptr(),
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn iter_mut(&mut self) -> IterMut<I, T> {
        IterMut {
            dense: self.dense.ones(),
            sparse: self.sparse.as_mut_ptr(),
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn iter_dense(&self) -> IterDense<I> {
        IterDense {
            dense: self.dense.ones(),
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn iter_sparse(&self) -> IterSparse<T> {
        IterSparse {
            dense: self.dense.ones(),
            sparse: self.sparse.as_ptr(),
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn iter_sparse_mut(&mut self) -> IterSparseMut<T> {
        IterSparseMut {
            dense: self.dense.ones(),
            sparse: self.sparse.as_mut_ptr(),
            _marker: PhantomData,
        }
    }
}

impl<I: SparseIndex, T> Index<I> for SparseSet<I, T> {
    type Output = T;

    #[inline]
    fn index(&self, index: I) -> &Self::Output {
        self.get(index).unwrap()
    }
}

impl<I: SparseIndex, T> IndexMut<I> for SparseSet<I, T> {
    #[inline]
    fn index_mut(&mut self, index: I) -> &mut Self::Output {
        self.get_mut(index).unwrap()
    }
}

impl<I: SparseIndex, T> Drop for SparseSet<I, T> {
    #[inline]
    fn drop(&mut self) {
        for index in self.dense.ones() {
            // Safety: If the key exists, then the value exists and is initialized.
            unsafe { self.sparse.get_unchecked_mut(index).assume_init_drop() };
        }
    }
}

impl<I: SparseIndex, T> IntoIterator for SparseSet<I, T> {
    type Item = (I, T);
    type IntoIter = IterOwned<I, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        let this = ManuallyDrop::new(self);
        // Safety: References are always valid for reads, initialized, and aligned.
        let (dense, sparse) = unsafe { (ptr::read(&this.dense), ptr::read(&this.sparse)) };

        IterOwned {
            dense: dense.ones().collect(),
            dense_index: 0,
            sparse,
            _marker: PhantomData,
        }
    }
}

impl<I: SparseIndex, T: Clone> Clone for SparseSet<I, T> {
    #[inline]
    fn clone(&self) -> Self {
        let mut sparse = Vec::<MaybeUninit<T>>::with_capacity(self.sparse.len());
        // Safety: It is okay for the new elements to be uninitialized, as per `MaybeUninit<T>`.
        unsafe { sparse.set_len(sparse.capacity()) };

        let dense = self.dense.clone();
        for index in dense.ones() {
            unsafe {
                // Safety: If the key exists, then the value exists and is initialized.
                let clone = self.sparse.get_unchecked(index).assume_init_ref().clone();
                // Safety: `index` always points to in-bound uninitialized data.
                sparse.get_unchecked_mut(index).write(clone);
            }
        }

        Self {
            dense,
            sparse,
            len: self.len,
            _marker: PhantomData,
        }
    }
}

impl<I: SparseIndex, T: Default> Default for SparseSet<I, T> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

pub struct IterOwned<I: SparseIndex, T> {
    dense: Box<[usize]>,
    dense_index: usize,
    sparse: Vec<MaybeUninit<T>>,
    _marker: PhantomData<I>,
}

impl<I: SparseIndex, T> Iterator for IterOwned<I, T> {
    type Item = (I, T);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let index = *self.dense.get(self.dense_index)?;
        // Convert first; if it panics, `dense_index` won't be advanced and cause a memory leak.
        let conv = I::from_index(index);

        self.dense_index += 1;
        // Safety: If the key exists, then the value exists and is initialized.
        Some((conv, unsafe { self.sparse.get_unchecked(index).assume_init_read() }))
    }
}

impl<I: SparseIndex, T> Drop for IterOwned<I, T> {
    #[inline]
    fn drop(&mut self) {
        while let Some(&index) = self.dense.get(self.dense_index) {
            self.dense_index += 1;
            // Safety: If the key exists, then the value exists and is initialized.
            unsafe { self.sparse.get_unchecked_mut(index).assume_init_drop() };
        }
    }
}

pub struct Iter<'a, I: SparseIndex, T> {
    dense: Ones<'a>,
    sparse: *const MaybeUninit<T>,
    _marker: PhantomData<(I, &'a T)>,
}

impl<'a, I: SparseIndex, T> Iterator for Iter<'a, I, T> {
    type Item = (I, &'a T);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let index = self.dense.next()?;
        // - If the key exists, then the value exists and is initialized.
        // - Pointer will never be null.
        Some((I::from_index(index), unsafe { self.sparse.add(index).as_ref().unwrap_unchecked().assume_init_ref() }))
    }
}

pub struct IterMut<'a, I: SparseIndex, T> {
    dense: Ones<'a>,
    sparse: *mut MaybeUninit<T>,
    _marker: PhantomData<(I, &'a mut T)>,
}

impl<'a, I: SparseIndex, T> Iterator for IterMut<'a, I, T> {
    type Item = (I, &'a mut T);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let index = self.dense.next()?;
        // Safety:
        // - If the key exists, then the value exists and is initialized.
        // - Pointer will never be null.
        Some((I::from_index(index), unsafe { self.sparse.add(index).as_mut().unwrap_unchecked().assume_init_mut() }))
    }
}

pub struct IterDense<'a, I: SparseIndex> {
    dense: Ones<'a>,
    _marker: PhantomData<I>,
}

impl<'a, I: SparseIndex> Iterator for IterDense<'a, I> {
    type Item = I;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let index = self.dense.next()?;
        Some(I::from_index(index))
    }
}

pub struct IterSparse<'a, T> {
    dense: Ones<'a>,
    sparse: *const MaybeUninit<T>,
    _marker: PhantomData<&'a T>,
}

impl<'a, T> Iterator for IterSparse<'a, T> {
    type Item = &'a T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let index = self.dense.next()?;
        // - If the key exists, then the value exists and is initialized.
        // - Pointer will never be null.
        Some(unsafe { self.sparse.add(index).as_ref().unwrap_unchecked().assume_init_ref() })
    }
}

pub struct IterSparseMut<'a, T> {
    dense: Ones<'a>,
    sparse: *mut MaybeUninit<T>,
    _marker: PhantomData<&'a mut T>,
}

impl<'a, T> Iterator for IterSparseMut<'a, T> {
    type Item = &'a mut T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let index = self.dense.next()?;
        // Safety:
        // - If the key exists, then the value exists and is initialized.
        // - Pointer will never be null.
        Some(unsafe { self.sparse.add(index).as_mut().unwrap_unchecked().assume_init_mut() })
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
        let mut set = SparseSet::<usize, Data>::new();
        set.insert(0, Data::new(314));
        set.insert(5, Data::new(159));
        set.insert(12, Data::new(69));
        set.insert(20, Data::new(420));

        // Initial state.
        assert_eq!(*GLOBAL.read().unwrap(), 4);

        // The set holds 4 elements across >20 sparse slots.
        assert_eq!(set.len, 4);
        assert!(set.sparse.len() > 20);

        // Sparse checks.
        assert!(set.contains(0));
        assert!(set.contains(5));
        assert!(set.contains(12));
        assert!(set.contains(20));
        for i in 1..5 { assert!(!set.contains(i)); }
        for i in 6..12 { assert!(!set.contains(i)); }
        for i in 13..20 { assert!(!set.contains(i)); }

        // Cloned set check.
        let cloned = set.clone();
        // The cloned set holds the same amount of elements over the same amount of sparse slots.
        assert_eq!(cloned.len, 4);
        assert!(cloned.sparse.len() > 20);

        // Cloned sparse checks.
        assert!(cloned.contains(0));
        assert!(cloned.contains(5));
        assert!(cloned.contains(12));
        assert!(cloned.contains(20));
        for i in 1..5 { assert!(!cloned.contains(i)); }
        for i in 6..12 { assert!(!cloned.contains(i)); }
        for i in 13..20 { assert!(!cloned.contains(i)); }

        // Set drop checks.
        assert_eq!(*GLOBAL.read().unwrap(), 8);
        drop(cloned);
        assert_eq!(*GLOBAL.read().unwrap(), 4);

        // Getter checks.
        assert_eq!(set.get(0), Some(&Data::new(314)));
        assert_eq!(unsafe { set.get_unchecked(5) }, &Data::new(159));
        assert_eq!(set.get_mut(12), Some(&mut Data::new(69)));
        assert_eq!(unsafe { set.get_unchecked_mut(20) }, &mut Data::new(420));

        // Exchange checks.
        assert_eq!(set.insert(0, Data::new(123)), Some(Data::new(314)));
        assert_eq!(set.insert(0, Data::new(314)), Some(Data::new(123)));
        assert_eq!(set.len, 4);

        // Remove checks.
        assert_eq!(set.remove(12), Some(Data::new(69)));
        assert_eq!(set.remove(12), None);
        assert_eq!(set.remove(20), Some(Data::new(420)));
        assert_eq!(set.remove(20), None);
        assert_eq!(set.remove(25), None);
        assert_eq!(set.len, 2);

        // Shrink checks.
        set.shrink_to_fit();
        assert_eq!(set.sparse.len(), 6);

        // Borrowed iterator checks.
        let mut iter = set.iter();
        assert_eq!(iter.next(), Some((0, &Data::new(314))));
        assert_eq!(iter.next(), Some((5, &Data::new(159))));
        assert_eq!(iter.next(), None);

        // Owned iterator checks.
        let mut iter = set.into_iter();
        assert_eq!(*GLOBAL.read().unwrap(), 2);

        assert_eq!(iter.next(), Some((0, Data::new(314))));
        assert_eq!(*GLOBAL.read().unwrap(), 1);

        // Owned iterator drop checks.
        drop(iter);
        assert_eq!(*GLOBAL.read().unwrap(), 0);
    }
}
