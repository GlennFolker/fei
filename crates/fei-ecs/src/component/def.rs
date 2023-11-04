use fei_common::prelude::*;
use std::{
    any::type_name,
    alloc::Layout,
    mem,
};

pub trait Component {
    /// Storage type for this component type. Setting this is no-op for [zero-sized types](
    /// https://doc.rust-lang.org/nomicon/exotic-sizes.html#zero-sized-types-zsts), as the storages
    /// for those will always be bitsets indexed by [`crate::entity::Entity::id`].
    const STORAGE: ComponentStorage = ComponentStorage::Table;
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct ComponentId(pub(crate) usize);
impl SparseIndex for ComponentId {
    #[inline]
    fn into_index(self) -> usize {
        self.0
    }

    #[inline]
    fn from_index(index: usize) -> Self {
        Self(index)
    }
}

#[derive(Debug, Clone)]
pub struct ComponentInfo {
    type_name: String,
    layout: Layout,
    storage: Option<ComponentStorage>,
}

impl ComponentInfo {
    #[inline]
    pub fn new<T: Component>() -> Self {
        Self {
            type_name: type_name::<T>().into(),
            layout: Layout::new::<T>(),
            storage: (mem::size_of::<T>() != 0).then_some(T::STORAGE),
        }
    }

    #[inline]
    pub fn is_zst(&self) -> bool {
        self.storage.is_none()
    }
}

/// Kinds of component storages, each with their own benefits. Note that [zero-sized types](
/// https://doc.rust-lang.org/nomicon/exotic-sizes.html#zero-sized-types-zsts) always use bitsets as
/// the container, indexed by [`crate::entity::Entity::id`].
#[derive(Copy, Clone, Debug)]
pub enum ComponentStorage {
    /// A table storage stores archetypes (i.e., the set of all components that belongs to an
    /// [`Entity`](crate::entity::Entity)) as a structure of arrays, offering faster iteration and cheaper memory requirements.
    Table,
    /// A sparse set storage stores each component types separately in a sparse set indexed by
    /// [`Entity::id`](crate::entity::Entity::id), offering faster addition and removal.
    SparseSet,
}
