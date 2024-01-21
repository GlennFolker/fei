use crate::world::World;
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

}
