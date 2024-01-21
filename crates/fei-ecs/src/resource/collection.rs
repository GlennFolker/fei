use fei_common::prelude::*;
use crate::{
    resource::{
        Resource, ResourceId,
        ResourceLocal, ResourceLocalId,
    },
    ChangeMark, RefErased, MutErased,
};
use std::{
    any::TypeId,
    cell::UnsafeCell,
    mem::MaybeUninit,
    thread::ThreadId,
};

#[derive(Error, Debug, Eq, PartialEq)]
#[error("non-send resource originating from thread {:?} queried from thread {:?}", .origin, .caller)]
pub struct LocalError {
    pub origin: ThreadId,
    pub caller: ThreadId,
}

pub type LocalResult<T> = Result<T, LocalError>;

#[derive(Default)]
pub struct Resources {
    containers: SparseSet<ResourceId, ResourceData>,
    local_containers: SparseSet<ResourceLocalId, ResourceData>,
    local_threads: Vec<MaybeUninit<ThreadId>>,

    ids: FxHashMap<TypeId, ResourceId>,
    local_ids: FxHashMap<TypeId, ResourceLocalId>,
}

pub struct ResourceData {
    inner: BoxErased<'static>,
    added: UnsafeCell<ChangeMark>,
    updated: UnsafeCell<ChangeMark>,
}

impl ResourceData {
    #[inline]
    fn new(inner: BoxErased<'static>, mark: ChangeMark) -> Self {
        Self {
            inner,
            added: UnsafeCell::new(mark),
            updated: UnsafeCell::new(mark),
        }
    }

    #[inline]
    pub fn as_ref(&self, last: ChangeMark) -> RefErased {
        unsafe { RefErased::new(self.inner.borrow(), *self.added.get(), *self.updated.get(), last) }
    }

    #[inline]
    pub fn as_mut(&mut self, last: ChangeMark, current: ChangeMark) -> MutErased {
        unsafe { MutErased::new(self.inner.borrow_mut(), &self.added, &self.updated, last, current) }
    }

    #[inline]
    pub fn as_mut_unique(&self, last: ChangeMark, current: ChangeMark) -> MutErased {
        unsafe { MutErased::new(self.inner.borrow().unique(), &self.added, &self.updated, last, current) }
    }
}

unsafe impl Send for Resources {}
unsafe impl Sync for Resources {}

impl Resources {
    #[inline]
    pub fn register<T: Resource>(&mut self) -> ResourceId {
        let id = self.ids.len();
        *self.ids.entry(TypeId::of::<T>()).or_insert(ResourceId(id))
    }

    #[inline]
    pub fn register_local<T: ResourceLocal>(&mut self) -> ResourceLocalId {
        let id = self.local_ids.len();
        *self.local_ids.entry(TypeId::of::<T>()).or_insert_with(|| {
            self.local_threads.push(MaybeUninit::uninit());
            ResourceLocalId(id)
        })
    }

    #[inline]
    pub fn get_id<T: Resource>(&self) -> Option<ResourceId> {
        self.ids.get(&TypeId::of::<T>()).copied()
    }

    #[inline]
    pub fn get_local_id<T: ResourceLocal>(&self) -> Option<ResourceLocalId> {
        self.local_ids.get(&TypeId::of::<T>()).copied()
    }

    #[inline]
    pub unsafe fn insert(&mut self, id: ResourceId, resource: BoxErased<'static>, current: ChangeMark) -> Option<BoxErased<'static>> {
        let ResourceData { inner, .. } = self.containers.insert(id, ResourceData::new(resource, current))?;
        Some(inner)
    }

    #[inline]
    pub unsafe fn remove(&mut self, id: ResourceId) -> Option<BoxErased<'static>> {
        let ResourceData { inner, .. } = self.containers.remove(id)?;
        Some(inner)
    }

    #[inline]
    pub unsafe fn insert_local(&mut self, id: ResourceLocalId, resource: BoxErased<'static>, current: ChangeMark) -> LocalResult<Option<BoxErased<'static>>> {
        let caller = std::thread::current().id();
        if let Some(prev) = self.local_containers.get_mut(id) {
            let origin = self.local_threads.get_unchecked(id.0).assume_init();
            if origin == caller {
                let ResourceData { inner, .. } = std::mem::replace(prev, ResourceData::new(resource, current));
                Ok(Some(inner))
            } else {
                Err(LocalError { origin, caller, })
            }
        } else {
            self.local_containers.insert(id, ResourceData::new(resource, current));
            self.local_threads.get_unchecked_mut(id.0).write(caller);
            Ok(None)
        }
    }

    #[inline]
    pub unsafe fn remove_local(&mut self, id: ResourceLocalId) -> LocalResult<Option<BoxErased<'static>>> {
        let caller = std::thread::current().id();
        if self.local_containers.contains(id) {
            let origin = self.local_threads.get_unchecked(id.0).assume_init();
            if origin == caller {
                let ResourceData { inner, .. } = self.local_containers.remove(id).unwrap_unchecked();
                Ok(Some(inner))
            } else {
                Err(LocalError { origin, caller, })
            }
        } else {
            Ok(None)
        }
    }

    #[inline]
    pub unsafe fn get(&self, id: ResourceId) -> Option<&ResourceData> {
        self.containers.get(id)
    }

    #[inline]
    pub unsafe fn get_mut(&mut self, id: ResourceId) -> Option<&mut ResourceData> {
        self.containers.get_mut(id)
    }

    #[inline]
    pub unsafe fn get_local(&self, id: ResourceLocalId) -> LocalResult<Option<&ResourceData>> {
        let caller = std::thread::current().id();
        match self.local_containers.get(id) {
            Some(value) => {
                let origin = self.local_threads.get_unchecked(id.0).assume_init();
                if origin == caller {
                    Ok(Some(value))
                } else {
                    Err(LocalError { origin, caller, })
                }
            },
            None => Ok(None),
        }
    }

    #[inline]
    pub unsafe fn get_local_mut(&mut self, id: ResourceLocalId) -> LocalResult<Option<&mut ResourceData>> {
        let caller = std::thread::current().id();
        match self.local_containers.get_mut(id) {
            Some(value) => {
                let origin = self.local_threads.get_unchecked(id.0).assume_init();
                if origin == caller {
                    Ok(Some(value))
                } else {
                    Err(LocalError { origin, caller, })
                }
            },
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fei_ecs_macros::{
        Resource, ResourceLocal,
    };

    #[derive(Resource, Debug, Eq, PartialEq)]
    struct Shared(u32);
    #[derive(ResourceLocal, Debug, Eq, PartialEq)]
    struct Local(u32);

    #[test]
    fn shared_and_local() -> anyhow::Result<()> {
        const TICK: ChangeMark = ChangeMark::new(0);

        let mut resources = Resources::default();
        let shared_id = resources.register::<Shared>();
        let local_id = resources.register_local::<Local>();

        unsafe {
            assert_eq!(resources.insert(shared_id, BoxErased::typed(Shared(314)), TICK).casted::<Shared>(), None);
            assert_eq!(resources.insert(shared_id, BoxErased::typed(Shared(159)), TICK).casted::<Shared>(), Some(Shared(314)));
            assert_eq!(resources.remove(shared_id).casted::<Shared>(), Some(Shared(159)));
            assert_eq!(resources.remove(shared_id).casted::<Shared>(), None);

            assert_eq!(resources.insert_local(local_id, BoxErased::typed(Local(123)), TICK)?.casted::<Local>(), None);
            assert_eq!(resources.insert_local(local_id, BoxErased::typed(Local(456)), TICK)?.casted::<Local>(), Some(Local(123)));

            std::thread::scope(|scope| {
                scope.spawn(|| assert!(resources.remove_local(local_id).is_err()));
            });

            assert_eq!(resources.remove_local(local_id)?.casted::<Local>(), Some(Local(456)));
            assert_eq!(resources.remove_local(local_id)?.casted::<Local>(), None);
        }

        Ok(())
    }
}
