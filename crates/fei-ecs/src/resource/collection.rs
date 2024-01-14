use fei_common::prelude::*;
use crate::{
    component::ChangeMarks,
    resource::{
        sealed,
        Resource, ResourceId,
        ResQuery, IsSend, NoSend,
    },
};
use std::{
    any::TypeId,
    thread::ThreadId,
};

#[derive(Error, Debug, Eq, PartialEq)]
#[error("non-send resource originating from thread {:?} queried from thread {:?}", .origin, .caller)]
pub struct SendError {
    pub origin: ThreadId,
    pub caller: ThreadId,
}

impl sealed::Sealed for IsSend {}
impl ResQuery for IsSend {
    type Output<T> = T;

    #[inline]
    unsafe fn insert<T: Resource>(resources: &mut Resources, id: ResourceId, resource: T) -> Self::Output<Option<T>> {
        resources.send_containers.insert(id, BoxErased::typed(resource)).casted()
    }

    #[inline]
    unsafe fn remove<T: Resource>(resources: &mut Resources, id: ResourceId) -> Self::Output<Option<T>> {
        resources.send_containers.remove(id).casted()
    }

    #[inline]
    unsafe fn get<T: Resource>(resources: &Resources, id: Option<ResourceId>) -> Self::Output<Option<&T>> {
        let id = id?;
        resources.send_containers.get(id).map(|value| value.deref())
    }

    #[inline]
    unsafe fn get_mut<T: Resource>(resources: &mut Resources, id: ResourceId) -> Self::Output<Option<&mut T>> {
        resources.send_containers.get_mut(id).map(|value| value.deref_mut())
    }
}

impl sealed::Sealed for NoSend {}
impl ResQuery for NoSend {
    type Output<T> = Result<T, SendError>;

    #[inline]
    unsafe fn insert<T: Resource>(resources: &mut Resources, id: ResourceId, resource: T) -> Self::Output<Option<T>> {
        let caller = std::thread::current().id();
        if let Some(&origin) = resources.non_send_threads.get(id) {
            if origin == caller {
                Ok(resources.non_send_containers.insert(id, BoxErased::typed(resource)).casted())
            } else {
                Err(SendError { origin, caller, })
            }
        } else {
            resources.non_send_threads.insert(id, caller);
            Ok(resources.non_send_containers.insert(id, BoxErased::typed(resource)).casted())
        }
    }

    #[inline]
    unsafe fn remove<T: Resource>(resources: &mut Resources, id: ResourceId) -> Self::Output<Option<T>> {
        let caller = std::thread::current().id();
        let Some(&origin) = resources.non_send_threads.get(id) else { return Ok(None) };
        if origin == caller {
            let resource = resources.non_send_containers.remove(id).unwrap_unchecked();
            resources.non_send_threads.remove(id);
            Ok(Some(resource.cast()))
        } else {
            Err(SendError { origin, caller, })
        }
    }

    #[inline]
    unsafe fn get<T: Resource>(resources: &Resources, id: Option<ResourceId>) -> Self::Output<Option<&T>> {
        let Some(id) = id else { return Ok(None) };

        let caller = std::thread::current().id();
        let Some(&origin) = resources.non_send_threads.get(id) else { return Ok(None) };
        if origin == caller {
            Ok(Some(resources.non_send_containers.get_unchecked(id).deref()))
        } else {
            Err(SendError { origin, caller, })
        }
    }

    #[inline]
    unsafe fn get_mut<T: Resource>(resources: &mut Resources, id: ResourceId) -> Self::Output<Option<&mut T>> {
        let caller = std::thread::current().id();
        let Some(&origin) = resources.non_send_threads.get(id) else { return Ok(None) };
        if origin == caller {
            Ok(Some(resources.non_send_containers.get_unchecked_mut(id).deref_mut()))
        } else {
            Err(SendError { origin, caller, })
        }
    }
}

#[derive(Default)]
pub struct Resources {
    send_containers: SparseSet<ResourceId, BoxErased<'static>>,
    non_send_containers: SparseSet<ResourceId, BoxErased<'static>>,
    non_send_threads: SparseSet<ResourceId, ThreadId>,

    resource_marks: Vec<ChangeMarks>,
    resource_ids: FxHashMap<TypeId, ResourceId>,
}

unsafe impl Send for Resources {}
unsafe impl Sync for Resources {}

impl Resources {
    #[inline]
    pub fn register<T: Resource>(&mut self) -> ResourceId {
        *self.resource_ids.entry(TypeId::of::<T>()).or_insert_with(|| {
            self.resource_marks.push(default());
            ResourceId(self.resource_marks.len() - 1)
        })
    }

    #[inline]
    pub fn get_id<T: Resource>(&self) -> Option<ResourceId> {
        self.resource_ids.get(&TypeId::of::<T>()).copied()
    }

    #[inline]
    pub fn insert<T: Resource>(&mut self, resource: T) -> <T::Query as ResQuery>::Output<Option<T>> {
        let id = self.register::<T>();
        unsafe { self.insert_by_id::<T>(id, resource) }
    }

    #[inline]
    pub unsafe fn insert_by_id<T: Resource>(&mut self, id: ResourceId, resource: T) -> <T::Query as ResQuery>::Output<Option<T>> {
        T::Query::insert(self, id, resource)
    }

    #[inline]
    pub fn remove<T: Resource>(&mut self) -> <T::Query as ResQuery>::Output<Option<T>> {
        let id = self.register::<T>();
        unsafe { self.remove_by_id::<T>(id) }
    }

    #[inline]
    pub unsafe fn remove_by_id<T: Resource>(&mut self, id: ResourceId) -> <T::Query as ResQuery>::Output<Option<T>> {
        T::Query::remove(self, id)
    }

    #[inline]
    pub fn get<T: Resource>(&self) -> <T::Query as ResQuery>::Output<Option<&T>> {
        let id = self.get_id::<T>();
        unsafe { T::Query::get(self, id) }
    }

    #[inline]
    pub unsafe fn get_by_id<T: Resource>(&self, id: ResourceId) -> <T::Query as ResQuery>::Output<Option<&T>> {
        T::Query::get(self, Some(id))
    }

    #[inline]
    pub fn get_mut<T: Resource>(&mut self) -> <T::Query as ResQuery>::Output<Option<&mut T>> {
        let id = self.register::<T>();
        unsafe { T::Query::get_mut(self, id) }
    }

    #[inline]
    pub unsafe fn get_by_id_mut<T: Resource>(&mut self, id: ResourceId) -> <T::Query as ResQuery>::Output<Option<&mut T>> {
        T::Query::get_mut(self, id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fei_ecs_macros::Resource;

    #[derive(Resource, Debug, Eq, PartialEq)]
    #[resource(send = true)]
    struct Shared(u32);

    #[derive(Resource, Debug, Eq, PartialEq)]
    #[resource(send = false)]
    struct Unshared(u32);

    #[test]
    fn send_and_not() -> anyhow::Result<()> {
        let mut res = Resources::default();
        res.insert(Shared(314));

        assert_eq!(res.insert(Shared(159)), Some(Shared(314)));
        assert_eq!(res.remove::<Shared>(), Some(Shared(159)));
        assert_eq!(res.remove::<Shared>(), None);

        res.insert(Unshared(123))?;
        std::thread::scope(|scope| {
            scope.spawn(|| assert!(res.get::<Unshared>().is_err()));
        });

        assert_eq!(res.get::<Unshared>()?, Some(&Unshared(123)));
        assert_eq!(res.remove::<Unshared>()?, Some(Unshared(123)));

        Ok(())
    }
}
