use fei_common::prelude::*;
use crate::{
    system::SystemParam,
    world::{
        World, WorldCell,
    },
    ChangeMark,
};

pub struct SystemState<Param: SystemParam> {
    state: Param::State,
    last: ChangeMark,
}

impl<Param: SystemParam> SystemState<Param> {
    #[inline]
    pub fn new(world: &mut World) -> anyhow::Result<Self> {
        Ok(Self {
            state: Param::construct_state(world)?,
            last: world.last_change_mark(),
        })
    }

    #[inline]
    pub fn get<'w, 's>(&'s mut self, world: &'w World) -> anyhow::Result<<Param::ReadOnly as SystemParam>::Item<'w, 's>> {
        let current = world.change_mark();
        let last = std::mem::replace(&mut self.last, current);
        unsafe { Param::ReadOnly::construct(world.cell(), &mut self.state, last, current) }
    }

    #[inline]
    pub fn get_mut<'w, 's>(&'s mut self, world: &'w mut World) -> anyhow::Result<Param::Item<'w, 's>> {
        unsafe { self.get_unchecked(world.cell_mut()) }
    }

    #[inline]
    pub unsafe fn get_unchecked<'w, 's>(&'s mut self, world: WorldCell<'w>) -> anyhow::Result<Param::Item<'w, 's>> {
        let current = world.get().change_mark();
        let last = std::mem::replace(&mut self.last, current);
        Param::construct(world, &mut self.state, last, current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fei_ecs_macros::Resource;
    use crate::{
        resource::ResMut,
        ChangeAware,
    };

    #[test]
    fn system_state() -> anyhow::Result<()> {
        #[derive(Resource)]
        struct Fei(String);

        let mut world = World::default();
        world.insert_res(Fei("fei".to_string()));

        let mut sys = SystemState::<ResMut<Fei>>::new(&mut world)?;

        let name = sys.get(&mut world)?;
        assert!(name.is_added());
        assert!(name.is_updated());

        let mut name = sys.get_mut(&mut world)?;
        assert!(!name.is_added());
        assert!(!name.is_updated());

        name.0 = "short".to_string();
        assert!(!name.is_added());
        assert!(name.is_updated());

        let name = sys.get(&mut world)?;
        assert!(!name.is_added());
        assert!(!name.is_updated());

        world.insert_res(Fei("fei again".to_string()));
        let name = sys.get(&mut world)?;
        assert!(name.is_added());
        assert!(name.is_updated());

        let name = sys.get(&mut world)?;
        assert!(!name.is_added());
        assert!(!name.is_updated());

        Ok(())
    }
}
