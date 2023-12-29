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
        #[derive(Component, Debug, Eq, PartialOrd, PartialEq)]
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
