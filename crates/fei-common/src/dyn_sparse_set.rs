use crate::{
    dyn_vec::{
        DynVec, DynVecDrop,
    },
    ptr::{
        Ptr, PtrMut, PtrOwned,
    },
    sparse_set::SparseIndex,
    drop_for,
};
use fixedbitset::FixedBitSet;
use std::{
    alloc::Layout,
    marker::PhantomData,
};

pub struct DynSparseSet<I: SparseIndex> {
    dense: FixedBitSet,
    sparse: DynVec,
    len: usize,
    _marker: PhantomData<I>,
}

impl<I: SparseIndex> DynSparseSet<I> {
    #[inline]
    pub const unsafe fn new(item_layout: Layout, drop: Option<unsafe fn(*mut u8)>) -> Self {
        Self {
            dense: FixedBitSet::new(),
            sparse: DynVec::new(item_layout, match drop {
                Some(dropper) => DynVecDrop::Manual(dropper),
                None => DynVecDrop::None,
            }),
            len: 0,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub const fn typed<T>() -> Self {
        Self {
            dense: FixedBitSet::new(),
            sparse: unsafe { DynVec::new(Layout::new::<T>(), match drop_for::<T>() {
                Some(dropper) => DynVecDrop::Manual(dropper),
                None => DynVecDrop::None,
            }) },
            len: 0,
            _marker: PhantomData,
        }
    }

    pub unsafe fn insert<R>(&mut self, index: I, value: PtrOwned, prev: impl FnOnce(PtrOwned) -> R) -> Option<R> {
        let index = index.into_index();
        if self.dense.contains(index) {
            Some(self.sparse.swap_unchecked(index, value, prev))
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
                self.sparse.set_len(index + 1);
            }

            // Safety: Sparse container is ensured to contain uninitialized value at `index`.
            self.sparse.write_unchecked(index, value);
            None
        }
    }

    #[inline]
    pub unsafe fn insert_and_drop(&mut self, index: I, value: PtrOwned) {
        let dropper = self.sparse.dropper();
        self.insert(index, value, |prev| if let DynVecDrop::Manual(dropper) = dropper {
            prev.drop_with(dropper)
        });
    }

    pub fn remove<R>(&mut self, index: I, removed: impl FnOnce(PtrOwned) -> R) -> Option<R> {
        let index = index.into_index();
        self.dense.contains(index)
            .then(|| {
                self.len -= 1;
                self.dense.set(index, false);

                // Safety: If the key exists, then the value exists and is initialized.
                removed(unsafe { self.sparse.get_unchecked_mut(index).own() })
            })
    }

    #[inline]
    pub fn remove_and_drop(&mut self, index: I) {
        let dropper = self.sparse.dropper();
        self.remove(index, |prev| if let DynVecDrop::Manual(dropper) = dropper {
            unsafe { prev.drop_with(dropper) }
        });
    }

    #[inline]
    pub fn contains(&self, index: I) -> bool {
        let index = index.into_index();
        self.dense.contains(index)
    }

    #[inline]
    pub fn get(&self, index: I) -> Option<Ptr> {
        let index = index.into_index();
        self.dense
            .contains(index)
            // Safety: If the key exists, then the value exists and is initialized.
            .then(|| unsafe { self.sparse.get_unchecked(index) })
    }

    #[inline]
    pub fn get_mut(&mut self, index: I) -> Option<PtrMut> {
        let index = index.into_index();
        self.dense
            .contains(index)
            // Safety: If the key exists, then the value exists and is initialized.
            .then(|| unsafe { self.sparse.get_unchecked_mut(index) })
    }

    #[inline]
    pub unsafe fn get_unchecked(&self, index: I) -> Ptr {
        let index = index.into_index();
        // Safety: Whether the key exists is upheld by the caller.
        self.sparse.get_unchecked(index)
    }

    #[inline]
    pub unsafe fn get_unchecked_mut(&mut self, index: I) -> PtrMut {
        let index = index.into_index();
        // Safety: Whether the key exists is upheld by the caller.
        self.sparse.get_unchecked_mut(index)
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
}

impl<I: SparseIndex> Drop for DynSparseSet<I> {
    #[inline]
    fn drop(&mut self) {
        if let DynVecDrop::Manual(dropper) = self.sparse.dropper() {
            for index in self.dense.ones() {
                unsafe {
                    // Safety: If the key exists, then the value exists and is initialized.
                    self.sparse
                        .get_unchecked_mut(index)
                        .drop_in_place_with(dropper);
                }
            }
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
            let mut set = DynSparseSet::<usize>::typed::<Data>();

            // Convert value to owning pointer.
            PtrOwned::take(Data::new(314), |ptr| set.insert(0, ptr, |prev| prev.drop_as::<Data>()));
            PtrOwned::take(Data::new(159), |ptr| set.insert(5, ptr, |prev| prev.drop_as::<Data>()));
            PtrOwned::take(Data::new(69), |ptr| set.insert(12, ptr, |prev| prev.drop_as::<Data>()));
            PtrOwned::take(Data::new(420), |ptr| set.insert(20, ptr, |prev| prev.drop_as::<Data>()));

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

            // Getter checks.
            assert_eq!(set.get(0).unwrap().deref::<Data>(), &Data::new(314));
            assert_eq!(set.get_unchecked(5).deref::<Data>(), &Data::new(159));
            assert_eq!(set.get_mut(12).unwrap().deref::<Data>(), &mut Data::new(69));
            assert_eq!(set.get_unchecked_mut(20).deref::<Data>(), &mut Data::new(420));

            // Exchange checks.
            assert_eq!(
                PtrOwned::take(Data::new(123), |ptr| set.insert(0, ptr, |prev| prev.read::<Data>())),
                Some(Data::new(314)),
            );
            assert_eq!(
                PtrOwned::take(Data::new(314), |ptr| set.insert(0, ptr, |prev| prev.read::<Data>())),
                Some(Data::new(123)),
            );
            assert_eq!(set.len, 4);

            // Remove checks.
            assert_eq!(set.remove(12, |ptr| ptr.read::<Data>()), Some(Data::new(69)));
            assert_eq!(set.remove(12, |ptr| ptr.read::<Data>()), None);
            assert_eq!(set.remove(20, |ptr| ptr.read::<Data>()), Some(Data::new(420)));
            assert_eq!(set.remove(20, |ptr| ptr.read::<Data>()), None);
            assert_eq!(set.remove(25, |ptr| ptr.read::<Data>()), None);

            assert_eq!(set.len, 2);
            assert_eq!(*GLOBAL.read().unwrap(), 2);

            // Shrink checks.
            set.shrink_to_fit();
            assert_eq!(set.sparse.len(), 6);

            drop(set);
            assert_eq!(*GLOBAL.read().unwrap(), 0);
        }
    }
}
