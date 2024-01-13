pub use fei_common_macros;

pub use anyhow;
pub use fixedbitset;
pub use fxhash;
pub use hashbrown;
pub use parking_lot;

pub mod sparse_set_erased;
pub mod vec_erased;
pub mod sparse_set;

pub mod ptr;

mod ext;
pub use ext::*;

pub mod prelude {
    pub use fei_common_macros::{
        self,
        impl_tuples,
    };

    pub use anyhow;
    pub use fixedbitset;
    pub use fxhash;
    pub use hashbrown;
    pub use parking_lot;
    pub use thiserror::Error;

    pub use super::{
        sparse_set_erased::SparseSetErased,
        vec_erased::VecErased,
        sparse_set::{
            SparseSet, SparseIndex,
        },
        SliceExt,
        FxHashMap, FxHashSet,
    };
}

use fxhash::FxBuildHasher;
use hashbrown::{
    HashMap, HashSet,
};
use std::{
    alloc::Layout,
    mem,
};

/// A [`HashMap`] that uses [`FxHasher`](fxhash::FxHasher) as the hasher for performance gains.
pub type FxHashMap<K, V> = HashMap<K, V, FxBuildHasher>;
/// A [`HashSet`] that uses [`FxHasher`](fxhash::FxHasher) as the hasher for performance gains.
pub type FxHashSet<T> = HashSet<T, FxBuildHasher>;

/// Converts the layout of `T` into `[T; len]` while ensures the total size in bytes never exceeds
/// [`isize::MAX`].
pub const fn array_layout(item_layout: Layout, len: usize) -> (Layout, usize) {
    let size = item_layout.size();
    let align = item_layout.align();

    let padded_size = size + (size.wrapping_add(align).wrapping_sub(1) & !align.wrapping_sub(1)).wrapping_sub(size);
    if len == 0 {
        // Safety: 0 is always a valid size.
        return (unsafe { Layout::from_size_align_unchecked(0, align) }, padded_size);
    }

    let Some(alloc_size) = padded_size.checked_mul(len) else {
        panic!("too big allocation size");
    };

    let layout = {
        assert!(alloc_size <= isize::MAX as usize - (align - 1), "too big allocation size");
        // Safety: Requirements just checked above.
        unsafe { Layout::from_size_align_unchecked(alloc_size, align) }
    };

    (layout, padded_size)
}

/// Returns [`None`] if dropping a value of type `T` doesn't matter, and [`Some`] value containing
/// an untyped wrapper to [`drop_in_place`](std::ptr::drop_in_place) otherwise.
#[inline]
pub const fn drop_for<T>() -> Option<unsafe fn(*mut u8)> {
    #[inline]
    unsafe fn dropper<T>(ptr: *mut u8) {
        ptr.cast::<T>().drop_in_place();
    }

    if mem::needs_drop::<T>() {
        Some(dropper::<T>)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::prelude::*;
    use std::any::type_name;

    trait MyTrait {
        type Assoc;

        fn my_instance_function(&self);

        fn my_static_function();
    }

    macro_rules! impl_my_trait {
        ($($tuple_type:ident $tuple_index:tt),*) => {
            impl<$($tuple_type: MyTrait,)*> MyTrait for ($($tuple_type,)*) {
                type Assoc = ($($tuple_type::Assoc,)*);

                fn my_instance_function(&self) {
                    $(self.$tuple_index.my_instance_function();)*
                }

                fn my_static_function() {
                    $($tuple_type::my_static_function();)*
                }
            }
        };
    } impl_tuples!(impl_my_trait! 8);

    impl MyTrait for f32 {
        type Assoc = u32;

        fn my_instance_function(&self) {
            println!("f32: {self}");
        }

        fn my_static_function() {
            println!("f32");
        }
    }

    impl MyTrait for &str {
        type Assoc = usize;

        fn my_instance_function(&self) {
            println!("&str: {self}");
        }

        fn my_static_function() {
            println!("&str");
        }
    }

    #[test]
    fn impl_tuples() {
        #[inline]
        fn commit<T: MyTrait>(value: T) {
            value.my_instance_function();
            T::my_static_function();

            println!("{}", type_name::<T>());
        }

        commit((314f32, "fei", 159f32, "short"));
    }
}
