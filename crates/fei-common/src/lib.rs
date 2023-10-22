#![cfg_attr(feature = "nightly", feature(allocator_api))]

pub use fei_common_macros;

pub use anyhow;
pub use fixedbitset;

pub mod container;

pub mod prelude {
    pub use fei_common_macros::{
        self,
        impl_tuples,
    };

    pub use anyhow;
    pub use fixedbitset;
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
