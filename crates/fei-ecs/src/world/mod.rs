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
        Resource, ResourceLocal, LocalResult,
    },
    world::{
        EntityView, EntityViewMut,
    },
    ChangeMark, Mut,
};

mod cell;
mod view;

pub use cell::*;
pub use view::*;

#[derive(Error, Debug)]
#[error("entity does not exist")]
pub struct NonexistentError;

#[derive(Default)]
pub struct World {
    components: Components,
    resources: Resources,
    entities: Entities,

    mark: ChangeMark,
}

impl World {
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
        unsafe { self.resources.insert(id, BoxErased::typed(resource)).casted() }
    }

    #[inline]
    pub fn insert_res_local<T: ResourceLocal>(&mut self, resource: T) -> LocalResult<Option<T>> {
        let id = self.resources.register_local::<T>();
        unsafe { self.resources.insert_local(id, BoxErased::typed(resource)).map(|opt| opt.casted()) }
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
    pub fn res<T: Resource>(&self) -> Option<&T> {
        let id = self.resources.get_id::<T>()?;
        Some(unsafe { self.resources.get(id, self.mark)?.casted::<T>() }.into_inner())
    }

    #[inline]
    pub fn res_local<T: ResourceLocal>(&self) -> LocalResult<Option<&T>> {
        let Some(id) = self.resources.get_local_id::<T>() else { return Ok(None) };
        unsafe { self.resources.get_local(id, self.mark).map(|opt| opt.map(|opt| opt.casted::<T>().into_inner())) }
    }

    #[inline]
    pub fn res_mut<T: Resource>(&mut self) -> Option<Mut<T>> {
        let id = self.resources.register::<T>();
        Some(unsafe { self.resources.get_mut(id, self.mark, self.mark)?.casted() })
    }

    #[inline]
    pub fn res_local_mut<T: ResourceLocal>(&mut self) -> LocalResult<Option<Mut<T>>> {
        let id = self.resources.register_local::<T>();
        unsafe { self.resources.get_local_mut(id, self.mark, self.mark).map(|opt| opt.map(|opt| opt.casted())) }
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

        let mut ecs = World::default();
        let fei = {
            let mut fei = ecs.spawn((Name("fei".to_string()), Height(-100.0)))?;
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
            let mut who_knows = ecs.spawn_empty()?;
            who_knows.insert((Name("oh wouldn't you like to know :^)".to_string()), LoveInterest(fei)));

            let who_knows = who_knows.id();
            ecs.view_mut(fei)?.insert(LoveInterest(who_knows));
            who_knows
        };

        assert_eq!(ecs.view(fei)?.get::<LoveInterest>(), Some(&LoveInterest(who_knows)));
        assert_eq!(ecs.view(who_knows)?.get::<LoveInterest>(), Some(&LoveInterest(fei)));
        Ok(())
    }
}
