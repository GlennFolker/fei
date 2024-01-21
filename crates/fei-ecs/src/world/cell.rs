use crate::{
    world::World,
    resource::{
        ResourceId, ResourceLocalId,
        LocalResult,
    },
    ChangeMark,
    RefErased,
    MutErased,
};
use std::{
    cell::UnsafeCell,
    marker::PhantomData,
};

#[derive(Copy, Clone)]
pub struct WorldCell<'a> {
    inner: *mut World,
    _marker: PhantomData<(&'a World, &'a UnsafeCell<World>)>,
}

impl<'a> WorldCell<'a> {
    #[inline]
    pub unsafe fn read(world: &'a World) -> Self {
        Self {
            inner: world as *const World as *mut World,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub unsafe fn write(world: &'a mut World) -> Self {
        Self {
            inner: world as *mut World,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub unsafe fn res_by_id(self, id: ResourceId, last: ChangeMark) -> Option<RefErased<'a>> {
        (*self.inner).resources.get(id).map(|data| data.as_ref(last))
    }

    #[inline]
    pub unsafe fn res_by_id_mut(self, id: ResourceId, last: ChangeMark, current: ChangeMark) -> Option<MutErased<'a>> {
        (*self.inner).resources.get(id).map(|data| data.as_mut_unique(last, current))
    }

    #[inline]
    pub unsafe fn res_local_by_id(self, id: ResourceLocalId, last: ChangeMark) -> LocalResult<Option<RefErased<'a>> >{
        (*self.inner).resources.get_local(id).map(|opt| opt.map(|data| data.as_ref(last)))
    }

    #[inline]
    pub unsafe fn res_local_by_id_mut(self, id: ResourceLocalId, last: ChangeMark, current: ChangeMark) -> LocalResult<Option<MutErased<'a>> >{
        (*self.inner).resources.get_local(id).map(|opt| opt.map(|data| data.as_mut_unique(last, current)))
    }
}
