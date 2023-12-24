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
    world::{
        EntityView, EntityViewMut,
    },
};

mod view;

pub use view::*;

#[derive(Error, Debug)]
#[error("entity does not exist")]
pub struct NonexistentError;

#[derive(Default)]
pub struct World {
    components: Components,
    entities: Entities,
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
}
