use fei_common::prelude::*;
use crate::{
    resource::{
        Resource, ResourceId,
        ResourceLocal, ResourceLocalId,
    },
    ChangeMark, ChangeMarks, RefErased, MutErased,
};
use std::{
    any::TypeId,
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
    containers: SparseSet<ResourceId, BoxErased<'static>>,
    local_containers: SparseSet<ResourceLocalId, BoxErased<'static>>,
    local_threads: Vec<MaybeUninit<ThreadId>>,

    changes: Vec<ChangeMarks>,
    local_changes: Vec<ChangeMarks>,

    ids: FxHashMap<TypeId, ResourceId>,
    local_ids: FxHashMap<TypeId, ResourceLocalId>,
}

unsafe impl Send for Resources {}
unsafe impl Sync for Resources {}

impl Resources {
    #[inline]
    pub fn register<T: Resource>(&mut self) -> ResourceId {
        let id = self.ids.len();
        *self.ids.entry(TypeId::of::<T>()).or_insert_with(|| {
            self.changes.push(default());
            ResourceId(id)
        })
    }

    #[inline]
    pub fn register_local<T: ResourceLocal>(&mut self) -> ResourceLocalId {
        let id = self.ids.len();
        *self.local_ids.entry(TypeId::of::<T>()).or_insert_with(|| {
            self.local_threads.push(MaybeUninit::uninit());
            self.local_changes.push(default());
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
    pub unsafe fn insert(&mut self, id: ResourceId, resource: BoxErased<'static>) -> Option<BoxErased<'static>> {
        self.containers.insert(id, resource)
    }

    #[inline]
    pub unsafe fn insert_local(&mut self, id: ResourceLocalId, resource: BoxErased<'static>) -> LocalResult<Option<BoxErased<'static>>> {
        let caller = std::thread::current().id();
        if let Some(prev) = self.local_containers.get_mut(id) {
            let origin = self.local_threads.get_unchecked(id.0).assume_init();
            if origin == caller {
                Ok(Some(std::mem::replace(prev, resource)))
            } else {
                Err(LocalError { origin, caller, })
            }
        } else {
            self.local_containers.insert(id, resource);
            self.local_threads.get_unchecked_mut(id.0).write(caller);
            Ok(None)
        }
    }

    #[inline]
    pub unsafe fn remove(&mut self, id: ResourceId) -> Option<BoxErased<'static>> {
        self.containers.remove(id)
    }

    #[inline]
    pub unsafe fn remove_local(&mut self, id: ResourceLocalId) -> LocalResult<Option<BoxErased<'static>>> {
        let caller = std::thread::current().id();
        if self.local_containers.contains(id) {
            let origin = self.local_threads.get_unchecked(id.0).assume_init();
            if origin == caller {
                Ok(Some(self.local_containers.remove(id).unwrap_unchecked()))
            } else {
                Err(LocalError { origin, caller, })
            }
        } else {
            Ok(None)
        }
    }

    #[inline]
    pub unsafe fn get(&self, id: ResourceId, last_mark: ChangeMark) -> Option<RefErased> {
        self.containers.get(id).map(|value| RefErased::new(
            value.borrow(), *self.changes.get_unchecked(id.0), last_mark,
        ))
    }

    #[inline]
    pub unsafe fn get_mut(&mut self, id: ResourceId, last_mark: ChangeMark, current_mark: ChangeMark) -> Option<MutErased> {
        self.containers.get_mut(id).map(|value| MutErased::new(
            value.borrow_mut(), self.changes.get_unchecked_mut(id.0).into(), last_mark, current_mark,
        ))
    }

    #[inline]
    pub unsafe fn get_local(&self, id: ResourceLocalId, last_mark: ChangeMark) -> LocalResult<Option<RefErased>> {
        let caller = std::thread::current().id();
        match self.local_containers.get(id) {
            Some(value) => {
                let origin = self.local_threads.get_unchecked(id.0).assume_init();
                if origin == caller {
                    Ok(Some(RefErased::new(value.borrow(), *self.changes.get_unchecked(id.0), last_mark)))
                } else {
                    Err(LocalError { origin, caller, })
                }
            },
            None => Ok(None),
        }
    }

    #[inline]
    pub unsafe fn get_local_mut(&mut self, id: ResourceLocalId, last_mark: ChangeMark, current_mark: ChangeMark) -> LocalResult<Option<MutErased>> {
        let caller = std::thread::current().id();
        match self.local_containers.get_mut(id) {
            Some(value) => {
                let origin = self.local_threads.get_unchecked(id.0).assume_init();
                if origin == caller {
                    Ok(Some(MutErased::new(value.borrow_mut(), self.changes.get_unchecked_mut(id.0).into(), last_mark, current_mark)))
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
        let mut resources = Resources::default();
        let shared_id = resources.register::<Shared>();
        let local_id = resources.register_local::<Local>();

        unsafe {
            assert_eq!(resources.insert(shared_id, BoxErased::typed(Shared(314))).casted::<Shared>(), None);
            assert_eq!(resources.insert(shared_id, BoxErased::typed(Shared(159))).casted::<Shared>(), Some(Shared(314)));
            assert_eq!(resources.remove(shared_id).casted::<Shared>(), Some(Shared(159)));
            assert_eq!(resources.remove(shared_id).casted::<Shared>(), None);

            assert_eq!(resources.insert_local(local_id, BoxErased::typed(Local(123)))?.casted::<Local>(), None);
            assert_eq!(resources.insert_local(local_id, BoxErased::typed(Local(456)))?.casted::<Local>(), Some(Local(123)));

            std::thread::scope(|scope| {
                scope.spawn(|| assert!(resources.remove_local(local_id).is_err()));
            });

            assert_eq!(resources.remove_local(local_id)?.casted::<Local>(), Some(Local(456)));
            assert_eq!(resources.remove_local(local_id)?.casted::<Local>(), None);
        }

        Ok(())
    }
}