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
            last: default(),
        })
    }

    #[inline]
    pub fn get<'w, 's>(&'s mut self, world: &'w World) -> anyhow::Result<<Param::ReadOnly as SystemParam>::Item<'w, 's>> {
        let (last, current) = world.change_mark();
        let last = std::mem::replace(&mut self.last, last);
        unsafe { Param::ReadOnly::construct(world.cell(), &mut self.state, last, current) }
    }

    #[inline]
    pub fn get_mut<'w, 's>(&'s mut self, world: &'w mut World) -> anyhow::Result<Param::Item<'w, 's>> {
        unsafe { self.get_unchecked(world.cell_mut()) }
    }

    #[inline]
    pub unsafe fn get_unchecked<'w, 's>(&'s mut self, world: WorldCell<'w>) -> anyhow::Result<Param::Item<'w, 's>> {
        let (last, current) = world.get().change_mark();
        let last = std::mem::replace(&mut self.last, last);
        Param::construct(world, &mut self.state, last, current)
    }
}
