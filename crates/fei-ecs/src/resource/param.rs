use fei_common::prelude::*;
use crate::{
    resource::{
        Resource, ResourceId,
        ResourceLocal, ResourceLocalId,
    },
    system::{
        SystemParam, ReadOnlySystemParam,
    },
    world::{
        World, WorldCell,
    },
    ChangeMark, ChangeAware, ChangeAwareMut,
    Ref, Mut,
};
use std::{
    any::type_name,
    fmt::{
        Debug, Formatter,
    },
    marker::PhantomData,
    ops::{
        Deref, DerefMut,
    },
};

#[derive(Error)]
#[error("resource `{}` not present on the World", type_name::<T>())]
pub struct NoResource<T: Resource>(PhantomData<fn() -> T>);
impl<T: Resource> Debug for NoResource<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "NoResource<{}>", type_name::<T>())
    }
}

#[derive(Error)]
#[error("local resource `{}` not present on the World", type_name::<T>())]
pub struct NoResourceLocal<T: ResourceLocal>(PhantomData<fn() -> T>);
impl<T: ResourceLocal> Debug for NoResourceLocal<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "NoResourceLocal<{}>", type_name::<T>())
    }
}

pub struct Res<'world, T: Resource>(Ref<'world, T>);
unsafe impl<'world, T: Resource> ReadOnlySystemParam for Res<'world, T> {}
impl<'world, T: Resource> SystemParam for Res<'world, T> {
    type State = ResourceId;
    type Item<'w, 's> = Res<'w, T>;
    type ReadOnly = Self;

    #[inline]
    unsafe fn construct<'w, 's>(world: WorldCell<'w>, state: &'s mut Self::State, last: ChangeMark, _: ChangeMark) -> anyhow::Result<Self::Item<'w, 's>> {
        Ok(Res(world
            .res_by_id(*state, last).ok_or(NoResource::<T>(PhantomData))?
            .casted()
        ))
    }

    #[inline]
    fn construct_state(world: &mut World) -> anyhow::Result<Self::State> {
        Ok(world.register_res::<T>())
    }
}

pub struct ResMut<'world, T: Resource>(Mut<'world, T>);
impl<'world, T: Resource> SystemParam for ResMut<'world, T> {
    type State = ResourceId;
    type Item<'w, 's> = ResMut<'w, T>;
    type ReadOnly = Res<'world, T>;

    #[inline]
    unsafe fn construct<'w, 's>(world: WorldCell<'w>, state: &'s mut Self::State, last: ChangeMark, current: ChangeMark) -> anyhow::Result<Self::Item<'w, 's>> {
        Ok(ResMut(world
            .res_by_id_mut(*state, last, current).ok_or(NoResource::<T>(PhantomData))?
            .casted()
        ))
    }

    #[inline]
    fn construct_state(world: &mut World) -> anyhow::Result<Self::State> {
        Ok(world.register_res::<T>())
    }
}

pub struct ResLocal<'world, T: ResourceLocal>(Ref<'world, T>);
unsafe impl<'world, T: ResourceLocal> ReadOnlySystemParam for ResLocal<'world, T> {}
impl<'world, T: ResourceLocal> SystemParam for ResLocal<'world, T> {
    type State = ResourceLocalId;
    type Item<'w, 's> = ResLocal<'w, T>;
    type ReadOnly = Self;

    #[inline]
    unsafe fn construct<'w, 's>(world: WorldCell<'w>, state: &'s mut Self::State, last: ChangeMark, _: ChangeMark) -> anyhow::Result<Self::Item<'w, 's>> {
        Ok(ResLocal(world
            .res_local_by_id(*state, last)?.ok_or(NoResourceLocal::<T>(PhantomData))?
            .casted()
        ))
    }

    #[inline]
    fn construct_state(world: &mut World) -> anyhow::Result<Self::State> {
        Ok(world.register_res_local::<T>())
    }
}

pub struct ResLocalMut<'world, T: ResourceLocal>(Mut<'world, T>);
impl<'world, T: ResourceLocal> SystemParam for ResLocalMut<'world, T> {
    type State = ResourceLocalId;
    type Item<'w, 's> = ResLocalMut<'w, T>;
    type ReadOnly = ResLocal<'world, T>;

    #[inline]
    unsafe fn construct<'w, 's>(world: WorldCell<'w>, state: &'s mut Self::State, last: ChangeMark, current: ChangeMark) -> anyhow::Result<Self::Item<'w, 's>> {
        Ok(ResLocalMut(world
            .res_local_by_id_mut(*state, last, current)?.ok_or(NoResourceLocal::<T>(PhantomData))?
            .casted()
        ))
    }

    #[inline]
    fn construct_state(world: &mut World) -> anyhow::Result<Self::State> {
        Ok(world.register_res_local::<T>())
    }
}

macro_rules! impl_res {
    ($name:ident, $target:ident, ref) => {
        impl<'w, T: $target> ChangeAware<'w> for $name<'w, T> {
            type Target<'t> = &'t T where 'w: 't, Self: 't;

            #[inline]
            fn is_added(&self) -> bool {
                self.0.is_added()
            }

            #[inline]
            fn is_updated(&self) -> bool {
                self.0.is_updated()
            }

            #[inline]
            fn get(&self) -> Self::Target<'_> {
                self.0.get()
            }
        }

        impl<'w, T: $target> AsRef<T> for $name<'w, T> {
            #[inline]
            fn as_ref(&self) -> &T {
                self.get()
            }
        }

        impl<'w, T: $target> Deref for $name<'w, T> {
            type Target = T;

            #[inline]
            fn deref(&self) -> &Self::Target {
                self.get()
            }
        }
    };
    ($name:ident, $target:ident, mut) => {
        impl_res!($name, $target, ref);
        impl<'w, T: $target> ChangeAwareMut<'w> for $name<'w, T> {
            type TargetMut<'t> = &'t mut T where 'w: 't, Self: 't;

            #[inline]
            fn update(&mut self) {
                self.0.update()
            }

            #[inline]
            fn bypass(&mut self) -> Self::TargetMut<'_> {
                self.0.bypass()
            }

            #[inline]
            fn get_mut(&mut self) -> Self::TargetMut<'_> {
                self.0.get_mut()
            }
        }

        impl<'w, T: $target> AsMut<T> for $name<'w, T> {
            #[inline]
            fn as_mut(&mut self) -> &mut T {
                self.get_mut()
            }
        }

        impl<'w, T: $target> DerefMut for $name<'w, T> {
            #[inline]
            fn deref_mut(&mut self) -> &mut Self::Target {
                self.get_mut()
            }
        }
    }
}

impl_res!(Res, Resource, ref);
impl_res!(ResMut, Resource, mut);
impl_res!(ResLocal, ResourceLocal, ref);
impl_res!(ResLocalMut, ResourceLocal, mut);
