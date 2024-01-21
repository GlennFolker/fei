use fei_common::prelude::*;
use crate::world::World;

pub trait System: 'static + Send + Sync {
    type In;
    type Out;

    #[inline]
    fn call(&mut self, input: Self::In, world: &mut World) -> Self::Out {
        unsafe { self.call_unchecked(input, world) }
    }

    unsafe fn call_unchecked(&mut self, input: Self::In, world: &World) -> Self::Out;
}

pub trait SystemParam {
    type State: SystemParamState;
    type Item<'w, 's>: SystemParam<State = Self::State>;

    unsafe fn construct<'w, 's>(world: &'w World, state: &'s mut Self::State) -> Self::Item<'w, 's>;
}

macro_rules! impl_system_param {
    ($($tuple_type:ident $tuple_index:tt),*) => {
        impl<$($tuple_type: SystemParam,)*> SystemParam for ($($tuple_type,)*) {
            type State = ($($tuple_type::State,)*);
            type Item<'w, 's> = ($($tuple_type::Item<'w, 's>,)*);

            #[inline]
            #[allow(unused)]
            unsafe fn construct<'w, 's>(world: &'w World, state: &'s mut Self::State) -> Self::Item<'w, 's> {
                ($($tuple_type::construct(world, &mut state.$tuple_index),)*)
            }
        }
    }
} impl_tuples!(impl_system_param! 8);

pub trait SystemParamState: 'static + Send + Sync {
    fn construct(world: &mut World) -> Self;
}

macro_rules! impl_system_state {
    ($($tuple_type:ident $tuple_index:tt),*) => {
        impl<$($tuple_type: SystemParamState,)*> SystemParamState for ($($tuple_type,)*) {
            #[inline]
            #[allow(unused)]
            fn construct(world: &mut World) -> Self {
                ($($tuple_type::construct(world),)*)
            }
        }
    }
} impl_tuples!(impl_system_state! 8);

pub trait IntoSystem<Marker>: Sized {
    type In;
    type Out;
    type System: System<In = Self::In, Out = Self::Out>;

    fn into_system(self, world: &mut World) -> Self::System;
}

pub trait SystemFn<Marker>: 'static + Send + Sync {
    type In;
    type Out;
    type Param: SystemParam;

    unsafe fn call(&mut self, input: Self::In, world: &World, state: &mut <Self::Param as SystemParam>::State) -> Self::Out;
}

pub struct SystemFnImpl<Func: SystemFn<Marker>, Marker: 'static> {
    state: <Func::Param as SystemParam>::State,
    func: Func,
}

impl<Func: SystemFn<Marker>, Marker> System for SystemFnImpl<Func, Marker> {
    type In = Func::In;
    type Out = Func::Out;

    #[inline]
    unsafe fn call_unchecked(&mut self, input: Self::In, world: &World) -> Self::Out {
        self.func.call(input, world, &mut self.state)
    }
}

impl<Func: SystemFn<Marker>, Marker: 'static> IntoSystem<Marker> for Func {
    type In = Func::In;
    type Out = Func::Out;
    type System = SystemFnImpl<Func, Marker>;

    #[inline]
    fn into_system(self, world: &mut World) -> Self::System {
        SystemFnImpl {
            state: <Func::Param as SystemParam>::State::construct(world),
            func: self,
        }
    }
}

pub struct In<T>(pub T);

macro_rules! impl_system_fn {
    ($($tuple_type:ident $tuple_index:tt),*) => {
        impl<$($tuple_type,)* Func, Out> SystemFn<fn($($tuple_type,)*) -> Out> for Func where
            $($tuple_type: SystemParam,)*
            Func: for<'w, 's> FnMut($($tuple_type::Item<'w, 's>,)*) -> Out + 'static + Send + Sync,
        {
            type In = ();
            type Out = Out;
            type Param = ($($tuple_type,)*);

            #[inline]
            #[allow(unused)]
            unsafe fn call(&mut self, (): Self::In, world: &World, state: &mut <Self::Param as SystemParam>::State) -> Self::Out {
                (self)($($tuple_type::construct(world, &mut state.$tuple_index),)*)
            }
        }

        impl<$($tuple_type,)* Func, Input, Out> SystemFn<fn(In<Input>, $($tuple_type,)*) -> Out> for Func where
            $($tuple_type: SystemParam,)*
            Func: for<'w, 's> FnMut(In<Input>, $($tuple_type::Item<'w, 's>,)*) -> Out + 'static + Send + Sync,
        {
            type In = Input;
            type Out = Out;
            type Param = ($($tuple_type,)*);

            #[inline]
            #[allow(unused)]
            unsafe fn call(&mut self, input: Self::In, world: &World, state: &mut <Self::Param as SystemParam>::State) -> Self::Out {
                (self)(In(input), $($tuple_type::construct(world, &mut state.$tuple_index),)*)
            }
        }
    }
} impl_tuples!(impl_system_fn! 8);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn into_system() -> anyhow::Result<()> {
        fn simple_sys() {
            println!("Hello, system!");
        }

        fn consumer_sys(In(param): In<u32>) {
            println!("Received {param}!");
        }

        fn processor_sys(In(param): In<u32>) -> anyhow::Result<u32> {
            (1..=param).reduce(|a, b| a * b).ok_or_else(|| anyhow::anyhow!("number must be >0"))
        }

        let mut world = World::default();
        let mut simple = simple_sys.into_system(&mut world);
        let mut consumer = consumer_sys.into_system(&mut world);
        let mut processor = processor_sys.into_system(&mut world);

        simple.call((), &mut world);
        consumer.call(314159, &mut world);
        println!("Returned {}!", processor.call(4, &mut world)?);

        Ok(())
    }
}
