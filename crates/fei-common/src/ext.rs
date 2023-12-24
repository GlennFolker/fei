use sealed::Sealed;
use std::mem::MaybeUninit;

pub trait SliceExt: Sealed {
    type Item;

    unsafe fn many_unchecked<const N: usize>(&self, indices: [usize; N]) -> [&Self::Item; N];

    unsafe fn many_unchecked_mut<const N: usize>(&mut self, indices: [usize; N]) -> [&mut Self::Item; N];
}

impl<T> SliceExt for [T] {
    type Item = T;

    #[inline]
    unsafe fn many_unchecked<const N: usize>(&self, indices: [usize; N]) -> [&Self::Item; N] {
        let slice = self as *const [T] as *const T;
        let mut arr: MaybeUninit<[&Self::Item; N]> = MaybeUninit::uninit();
        let arr_ptr = arr.as_mut_ptr();

        for i in 0..N {
            let idx = *indices.get_unchecked(i);
            *(*arr_ptr).get_unchecked_mut(i) = &*slice.add(idx);
        }
        arr.assume_init()
    }

    #[inline]
    unsafe fn many_unchecked_mut<const N: usize>(&mut self, indices: [usize; N]) -> [&mut Self::Item; N] {
        let slice = self as *mut [T] as *mut T;
        let mut arr: MaybeUninit<[&mut Self::Item; N]> = MaybeUninit::uninit();
        let arr_ptr = arr.as_mut_ptr();

        for i in 0..N {
            let idx = *indices.get_unchecked(i);
            *(*arr_ptr).get_unchecked_mut(i) = &mut *slice.add(idx);
        }
        arr.assume_init()
    }
}

mod sealed {
    pub trait Sealed {}

    impl<T> Sealed for [T] {}
}
