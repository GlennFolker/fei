use fei_common::prelude::*;
use crate::{
    component::{
        ComponentSet,
        Components,
    },
    entity::{
        Entity,
        Entities, SpawnError,
    },
    resource::{
        Resources,
        Resource, ResourceId,
        ResourceLocal, ResourceLocalId, LocalResult,
    },
    world::{
        EntityView, EntityViewMut,
    },
    ChangeMark, Ref, Mut,
};
use std::sync::atomic::{
    AtomicU32, Ordering,
};

mod cell;
mod view;

pub use cell::*;
pub use view::*;

#[derive(Error, Debug)]
#[error("entity does not exist")]
pub struct NonexistentError;

pub struct World {
    components: Components,
    resources: Resources,
    entities: Entities,

    last: ChangeMark,
    current: AtomicU32,
}

impl Default for World {
    #[inline]
    fn default() -> Self {
        Self {
            components: default(),
            resources: default(),
            entities: default(),

            last: ChangeMark::new(0),
            current: AtomicU32::new(1),
        }
    }
}

impl World {
    #[inline]
    pub fn change_mark(&self) -> ChangeMark {
        ChangeMark::new(self.current.fetch_add(1, Ordering::Relaxed))
    }

    #[inline]
    pub fn change_mark_mut(&mut self) -> ChangeMark {
        let last = self.current.get_mut();
        ChangeMark::new(std::mem::replace(last, last.wrapping_add(1)))
    }

    #[inline]
    pub fn read_change_mark(&self) -> ChangeMark {
        ChangeMark::new(self.current.load(Ordering::Relaxed))
    }

    #[inline]
    pub fn last_change_mark(&self) -> ChangeMark {
        self.last
    }

    #[inline]
    pub fn sync_change_mark(&mut self) {
        self.last = self.change_mark_mut();
    }

    #[inline]
    pub fn spawn<T: ComponentSet>(&mut self, set: T) -> Result<EntityViewMut, SpawnError> {
        let mut view = self.spawn_empty()?;
        view.insert(set);
        Ok(view)
    }

    #[inline]
    pub fn spawn_empty(&mut self) -> Result<EntityViewMut, SpawnError> {
        let entity = self.entities.spawn()?;
        Ok(unsafe { EntityViewMut::new(entity, &mut self.entities, &mut self.components) })
    }

    #[inline]
    pub fn view(&self, entity: Entity) -> Result<EntityView, NonexistentError> {
        self.entities
            .contains(entity)
            .then(|| unsafe { EntityView::new(entity, &self.entities, &self.components) })
            .ok_or(NonexistentError)
    }

    #[inline]
    pub fn view_mut(&mut self, entity: Entity) -> Result<EntityViewMut, NonexistentError> {
        self.entities
            .contains(entity)
            .then(|| unsafe { EntityViewMut::new(entity, &mut self.entities, &mut self.components) })
            .ok_or(NonexistentError)
    }

    #[inline]
    pub fn register_res<T: Resource>(&mut self) -> ResourceId {
        self.resources.register::<T>()
    }

    #[inline]
    pub fn register_res_local<T: ResourceLocal>(&mut self) -> ResourceLocalId {
        self.resources.register_local::<T>()
    }

    #[inline]
    pub fn init_res<T: Resource + FromWorld>(&mut self) -> Option<T> {
        let resource = T::from_world(self);
        self.insert_res(resource)
    }

    #[inline]
    pub fn init_res_local<T: ResourceLocal + FromWorld>(&mut self) -> LocalResult<Option<T>> {
        let resource = T::from_world(self);
        self.insert_res_local(resource)
    }

    #[inline]
    pub fn insert_res<T: Resource>(&mut self, resource: T) -> Option<T> {
        let id = self.resources.register::<T>();
        let current = self.change_mark_mut();
        unsafe { self.resources.insert(id, BoxErased::typed(resource), current).casted() }
    }

    #[inline]
    pub fn insert_res_local<T: ResourceLocal>(&mut self, resource: T) -> LocalResult<Option<T>> {
        let id = self.resources.register_local::<T>();
        let current = self.change_mark_mut();
        unsafe { self.resources.insert_local(id, BoxErased::typed(resource), current).map(|opt| opt.casted()) }
    }

    #[inline]
    pub fn remove_res<T: Resource>(&mut self) -> Option<T> {
        let id = self.resources.register::<T>();
        unsafe { self.resources.remove(id).casted() }
    }

    #[inline]
    pub fn remove_res_local<T: ResourceLocal>(&mut self) -> LocalResult<Option<T>> {
        let id = self.resources.register_local::<T>();
        unsafe { self.resources.remove_local(id).map(|opt| opt.casted()) }
    }

    #[inline]
    pub fn res<T: Resource>(&self) -> Option<Ref<T>> {
        let id = self.resources.get_id::<T>()?;
        unsafe { self.cell().res_by_id(id, self.read_change_mark()).map(|value| value.casted()) }
    }

    #[inline]
    pub fn res_mut<T: Resource>(&mut self) -> Option<Mut<T>> {
        let id = self.resources.register::<T>();
        let current = self.change_mark_mut();
        let last = self.last;
        unsafe { self.cell_mut().res_by_id_mut(id, last, current).map(|value| value.casted()) }
    }

    #[inline]
    pub fn res_local<T: ResourceLocal>(&self) -> LocalResult<Option<Ref<T>>> {
        let Some(id) = self.resources.get_local_id::<T>() else { return Ok(None) };
        unsafe { self.cell().res_local_by_id(id, self.read_change_mark()).map(|opt| opt.map(|value| value.casted())) }
    }

    #[inline]
    pub fn res_local_mut<T: ResourceLocal>(&mut self) -> LocalResult<Option<Mut<T>>> {
        let id = self.resources.register_local::<T>();
        let current = self.change_mark_mut();
        let last = self.last;
        unsafe { self.cell_mut().res_local_by_id_mut(id, last, current).map(|opt| opt.map(|value| value.casted())) }
    }

    #[inline]
    pub fn cell(&self) -> WorldCell {
        unsafe { WorldCell::read(self) }
    }

    #[inline]
    pub fn cell_mut(&mut self) -> WorldCell {
        unsafe { WorldCell::read(self) }
    }
}

pub trait FromWorld {
    fn from_world(world: &mut World) -> Self;
}

impl<T: Default> FromWorld for T {
    #[inline]
    fn from_world(_: &mut World) -> Self {
        default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fei_ecs_macros::Component;

    #[test]
    fn viewing() -> anyhow::Result<()> {
        #[derive(Component, Debug, Eq, PartialEq)]
        struct Name(String);
        #[derive(Component, Debug, PartialEq)]
        struct Height(f32);
        #[derive(Component, Debug, Eq, PartialEq)]
        struct LoveInterest(Entity);

        let mut world = World::default();
        let fei = {
            let mut fei = world.spawn((Name("fei".to_string()), Height(-100.0)))?;
            assert_eq!(fei.get::<Name>(), Some(&Name("fei".to_string())));
            assert_eq!(fei.get_mut::<Height>(), Some(&mut Height(-100.0)));
            assert_eq!(fei.get::<LoveInterest>(), None);

            let Some((name, height)) = fei.extract::<(Name, Height)>() else { anyhow::bail!("Invalid components") };
            assert_eq!(name.0, "fei");
            assert_eq!(height.0, -100.0);

            fei.insert((name, height));
            fei.id()
        };

        let who_knows = {
            let mut who_knows = world.spawn_empty()?;
            who_knows.insert((Name("oh wouldn't you like to know :^)".to_string()), LoveInterest(fei)));

            let who_knows = who_knows.id();
            world.view_mut(fei)?.insert(LoveInterest(who_knows));
            who_knows
        };

        assert_eq!(world.view(fei)?.get::<LoveInterest>(), Some(&LoveInterest(who_knows)));
        assert_eq!(world.view(who_knows)?.get::<LoveInterest>(), Some(&LoveInterest(fei)));
        Ok(())
    }
}
