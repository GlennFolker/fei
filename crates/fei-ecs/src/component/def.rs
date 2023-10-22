use crate::entity::Entity;

pub trait Component {
    /// Storage type for this component type. Setting this is no-op for [zero-sized types](
    /// https://doc.rust-lang.org/nomicon/exotic-sizes.html#zero-sized-types-zsts), as the storages
    /// for those will always be bitsets indexed by [`Entity::id`].
    const STORAGE: ComponentStorage = ComponentStorage::Table;
}

pub struct ComponentId(pub(crate) usize);

/// Kinds of component storages, each with their own benefits. Note that [zero-sized types](
/// https://doc.rust-lang.org/nomicon/exotic-sizes.html#zero-sized-types-zsts) always use bitsets as
/// the container, indexed by [`Entity::id`].
pub enum ComponentStorage {
    /// A table storage stores archetypes (i.e., the set of all components that belongs to an
    /// [`Entity`]) as a structure of arrays, offering faster iteration and cheaper memory requirements.
    Table,
    /// A sparse set storage stores each component types separately in a sparse set indexed by
    /// [`Entity::id`], offering faster addition and removal.
    SparseSet,
}
