pub use fei_common_macros;

pub use anyhow;
pub use fixedbitset;
pub use fxhash;
pub use hashbrown;
pub use parking_lot;

pub mod sparse_set;

pub mod box_erased;
pub mod sparse_set_erased;
pub mod vec_erased;

pub mod ptr;

mod ext;
pub use ext::*;

pub mod prelude {
    pub use fei_common_macros::{
        self,
        impl_tuples, fei_panic,
    };

    pub use anyhow;
    pub use fixedbitset;
    pub use fxhash;
    pub use hashbrown;
    pub use parking_lot;
    pub use thiserror::Error;

    pub use super::{
        sparse_set::{
            SparseSet, SparseIndex,
        },
        box_erased::{
            BoxErased,
            OptionBoxErasedExt,
        },
        ptr::{
            OptionPtrExt, OptionPtrMutExt,
        },
        sparse_set_erased::SparseSetErased,
        vec_erased::VecErased,
        SliceExt,
        FxHashMap, FxHashSet,
        default,
    };
}

use fei_common_macros::fei_panic;
use fxhash::FxBuildHasher;
use hashbrown::{
    HashMap, HashSet,
};
use std::alloc::Layout;

/// A [`HashMap`] that uses [`FxHasher`](fxhash::FxHasher) as the hasher for performance gains.
pub type FxHashMap<K, V> = HashMap<K, V, FxBuildHasher>;
/// A [`HashSet`] that uses [`FxHasher`](fxhash::FxHasher) as the hasher for performance gains.
pub type FxHashSet<T> = HashSet<T, FxBuildHasher>;

/// Converts the layout of `T` into `[T; len]` while ensures the total size in bytes never exceeds
/// [`isize::MAX`].
pub const fn array_layout(item_layout: Layout, len: usize) -> (Layout, usize) {
    #[fei_panic]
    const fn overallocate_error() -> ! {
        panic!("too big allocation size")
    }

    let size = item_layout.size();
    let align = item_layout.align();

    let padded_size = size + (size.wrapping_add(align).wrapping_sub(1) & !align.wrapping_sub(1)).wrapping_sub(size);
    if len == 0 {
        // Safety: 0 is always a valid size.
        return (unsafe { Layout::from_size_align_unchecked(0, align) }, padded_size);
    }

    let Some(alloc_size) = padded_size.checked_mul(len) else {
        overallocate_error()
    };

    let layout = {
        if alloc_size > isize::MAX as usize - (align - 1) {
            overallocate_error()
        } else {
            unsafe { Layout::from_size_align_unchecked(alloc_size, align) }
        }
    };

    (layout, padded_size)
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

/// Returns [`None`] if dropping a value of type `T` doesn't matter, and [`Some`] value containing
/// an untyped wrapper to [`drop_in_place`](std::ptr::drop_in_place) otherwise.
#[inline]
pub const fn drop_for<T>() -> Option<unsafe fn(*mut u8)> {
    #[inline]
    unsafe fn dropper<T>(ptr: *mut u8) {
        ptr.cast::<T>().drop_in_place();
    }

    if std::mem::needs_drop::<T>() {
        Some(dropper::<T>)
    } else {
        None
    }
}

#[inline]
pub fn default<T: Default>() -> T {
    T::default()
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
