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
        let current = world.get().change_mark();
        let last = std::mem::replace(&mut self.last, current);
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
            last: world.last_change_mark(),
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
    use fei_ecs_macros::Resource;
    use crate::{
        resource::{
            Res, ResMut,
        },
        ChangeAware,
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
    fn change_detection() -> anyhow::Result<()> {
        #[derive(Resource)]
        struct Fei;

        fn a_sys(In(change): In<bool>, mut fei: ResMut<Fei>) -> anyhow::Result<(bool, bool)> {
            if change { *fei = Fei; }
            Ok((fei.is_added(), fei.is_updated()))
        }

        fn b_sys(fei: Res<Fei>) -> anyhow::Result<(bool, bool)> {
            Ok((fei.is_added(), fei.is_updated()))
        }

        let mut world = World::default();
        world.insert_res(Fei);

        let mut a = a_sys.into_system(&mut world)?;
        world.sync_change_mark();
        let mut b = b_sys.into_system(&mut world)?;

        assert_eq!(a.call(false, &mut world)?, (true, true));
        assert_eq!(b.call((), &mut world)?, (false, false));

        world.sync_change_mark();
        assert_eq!(a.call(false, &mut world)?, (false, false));
        assert_eq!(b.call((), &mut world)?, (false, false));

        world.sync_change_mark();
        assert_eq!(a.call(true, &mut world)?, (false, true));
        assert_eq!(b.call((), &mut world)?, (false, true));

        world.sync_change_mark();
        assert_eq!(a.call(false, &mut world)?, (false, false));
        assert_eq!(b.call((), &mut world)?, (false, false));

        Ok(())
    }
}
