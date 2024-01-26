use fei_common::prelude::*;
use crate::{
    world::{
        World, WorldCell,
    },
    ChangeMark,
};

pub trait System: 'static + Send + Sync {
    type In;
    type Out;

    #[inline]
    fn call(&mut self, input: Self::In, world: &mut World) -> anyhow::Result<Self::Out> {
        unsafe { self.call_unchecked(input, world.cell_mut()) }
    }

    unsafe fn call_unchecked(&mut self, input: Self::In, world: WorldCell) -> anyhow::Result<Self::Out>;
}

pub trait SystemParam: Sized {
    type State: 'static + Send + Sync;
    type Item<'w, 's>: SystemParam<State = Self::State>;
    type ReadOnly: ReadOnlySystemParam<State = Self::State>;

    unsafe fn construct<'w, 's>(world: WorldCell<'w>, state: &'s mut Self::State, last: ChangeMark, current: ChangeMark) -> anyhow::Result<Self::Item<'w, 's>>;

    fn construct_state(world: &mut World) -> anyhow::Result<Self::State>;
}

pub unsafe trait ReadOnlySystemParam: SystemParam {}

macro_rules! impl_system_param {
    ($($tuple_type:ident $tuple_index:tt),*) => {
        impl<$($tuple_type: SystemParam,)*> SystemParam for ($($tuple_type,)*) {
            type State = ($($tuple_type::State,)*);
            type Item<'w, 's> = ($($tuple_type::Item<'w, 's>,)*);
            type ReadOnly = ($($tuple_type::ReadOnly,)*);

            #[inline]
            #[allow(unused)]
            unsafe fn construct<'w, 's>(world: WorldCell<'w>, state: &'s mut Self::State, last: ChangeMark, current: ChangeMark) -> anyhow::Result<Self::Item<'w, 's>> {
                Ok(($($tuple_type::construct(world, &mut state.$tuple_index, last, current)?,)*))
            }

            #[inline]
            #[allow(unused)]
            fn construct_state(world: &mut World) -> anyhow::Result<Self::State> {
                Ok(($($tuple_type::construct_state(world)?,)*))
            }
        }

        unsafe impl<$($tuple_type: ReadOnlySystemParam,)*> ReadOnlySystemParam for ($($tuple_type,)*) {}
    }
} impl_tuples!(impl_system_param! 8);

pub trait IntoSystem<Marker>: Sized {
    type In;
    type Out;
    type System: System<In = Self::In, Out = Self::Out>;

    fn into_system(self, world: &mut World) -> anyhow::Result<Self::System>;
}

pub trait SystemFn<Marker>: 'static + Send + Sync + Sized {
    type In;
    type Out;
    type Param: SystemParam;

    unsafe fn call(&mut self, input: Self::In, world: WorldCell, state: &mut <Self::Param as SystemParam>::State, last: ChangeMark, current: ChangeMark) -> anyhow::Result<Self::Out>;
}

pub struct SystemFnImpl<Func: SystemFn<Marker>, Marker: 'static> {
    state: <Func::Param as SystemParam>::State,
    func: Func,
    last: ChangeMark,
}

impl<Func: SystemFn<Marker>, Marker> System for SystemFnImpl<Func, Marker> {
    type In = Func::In;
    type Out = Func::Out;

    #[inline]
    unsafe fn call_unchecked(&mut self, input: Self::In, world: WorldCell) -> anyhow::Result<Self::Out> {
        let (last, current) = world.get().change_mark();
        let last = std::mem::replace(&mut self.last, last);
        self.func.call(input, world, &mut self.state, last, current)
    }
}

impl<Func: SystemFn<Marker>, Marker: 'static> IntoSystem<Marker> for Func {
    type In = Func::In;
    type Out = Func::Out;
    type System = SystemFnImpl<Func, Marker>;

    #[inline]
    fn into_system(self, world: &mut World) -> anyhow::Result<Self::System> {
        Ok(SystemFnImpl {
            state: Func::Param::construct_state(world)?,
            func: self,
            last: default(),
        })
    }
}

pub struct In<T>(pub T);

macro_rules! impl_system_fn {
    ($($tuple_type:ident $tuple_index:tt),*) => {
        impl<$($tuple_type,)* Func, Out> SystemFn<fn($($tuple_type,)*) -> anyhow::Result<Out>> for Func where
            $($tuple_type: SystemParam,)*
            Func: for<'w, 's> FnMut($($tuple_type,)*) -> anyhow::Result<Out> + 'static + Send + Sync,
            Func: for<'w, 's> FnMut($($tuple_type::Item<'w, 's>,)*) -> anyhow::Result<Out> + 'static + Send + Sync,
        {
            type In = ();
            type Out = Out;
            type Param = ($($tuple_type,)*);

            #[inline]
            #[allow(unused)]
            unsafe fn call(&mut self, (): Self::In, world: WorldCell, state: &mut <Self::Param as SystemParam>::State, last: ChangeMark, current: ChangeMark) -> anyhow::Result<Self::Out> {
                (self)($($tuple_type::construct(world, &mut state.$tuple_index, last, current)?,)*)
            }
        }

        impl<$($tuple_type,)* Func, Input, Out> SystemFn<fn(In<Input>, $($tuple_type,)*) -> anyhow::Result<Out>> for Func where
            $($tuple_type: SystemParam,)*
            Func: for<'w, 's> FnMut(In<Input>, $($tuple_type,)*) -> anyhow::Result<Out> + 'static + Send + Sync,
            Func: for<'w, 's> FnMut(In<Input>, $($tuple_type::Item<'w, 's>,)*) -> anyhow::Result<Out> + 'static + Send + Sync,
        {
            type In = Input;
            type Out = Out;
            type Param = ($($tuple_type,)*);

            #[inline]
            #[allow(unused)]
            unsafe fn call(&mut self, input: Self::In, world: WorldCell, state: &mut <Self::Param as SystemParam>::State, last: ChangeMark, current: ChangeMark) -> anyhow::Result<Self::Out> {
                (self)(In(input), $($tuple_type::construct(world, &mut state.$tuple_index, last, current)?,)*)
            }
        }
    }
} impl_tuples!(impl_system_fn! 8);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        resource::{
            Resource, ResourceId,
        },
        ChangeAware,
        Ref,
    };

    #[test]
    fn into_system() -> anyhow::Result<()> {
        fn simple_sys() -> anyhow::Result<()> {
            Ok(println!("Hello, system!"))
        }

        fn consumer_sys(In(param): In<u32>) -> anyhow::Result<()> {
            Ok(println!("Received {param}!"))
        }

        fn processor_sys(In(param): In<u32>) -> anyhow::Result<u32> {
            (1..=param).reduce(|a, b| a * b).ok_or_else(|| anyhow::anyhow!("number must be >0"))
        }

        let mut world = World::default();
        let mut simple = simple_sys.into_system(&mut world)?;
        let mut consumer = consumer_sys.into_system(&mut world)?;
        let mut processor = processor_sys.into_system(&mut world)?;

        simple.call((), &mut world)?;
        consumer.call(314159, &mut world)?;
        println!("Returned {}!", processor.call(4, &mut world)?);

        Ok(())
    }

    #[test]
    fn system_param() -> anyhow::Result<()> {
        struct Param<'world, T: Resource>(Ref<'world, T>);
        impl Resource for u32 {}

        unsafe impl<'world, T: Resource> ReadOnlySystemParam for Param<'world, T> {}
        impl<'world, T: Resource> SystemParam for Param<'world, T> {
            type State = ResourceId;
            type Item<'w, 's> = Param<'w, T>;
            type ReadOnly = Self;

            #[inline]
            unsafe fn construct<'w, 's>(world: WorldCell<'w>, state: &'s mut Self::State, last: ChangeMark, _: ChangeMark) -> anyhow::Result<Self::Item<'w, 's>> {
                world
                    .res_by_id(*state, last).ok_or_else(|| anyhow::anyhow!("resource doesn't exist"))
                    .map(|res| Param(res.casted()))
            }

            #[inline]
            fn construct_state(world: &mut World) -> anyhow::Result<Self::State> {
                Ok(world.register_res::<T>())
            }
        }

        fn param_sys(In(check): In<u32>, param: Param<u32>) -> anyhow::Result<()> {
            if check == 314 {
                assert!(param.0.is_added());
            } else {
                assert!(!param.0.is_added());
            }

            assert!(param.0.is_updated());
            assert_eq!(check, *param.0);

            Ok(())
        }

        let mut world = World::default();
        let mut sys = param_sys.into_system(&mut world)?;

        world.insert_res(314);
        sys.call(314, &mut world)?;
        *world.res_mut::<u32>().unwrap() = 159;
        sys.call(159, &mut world)?;

        Ok(())
    }
}
