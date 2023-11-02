use fei_common::prelude::*;
use crate::component::ComponentId;
use fixedbitset::FixedBitSet;
use std::{
    any::TypeId,
};

/// Component storages, identified by [`crate::entity::Entity`]s.
#[derive(Default)]
pub struct Components {
    /// Storage for zero-sized type components.
    bitsets: SparseSet<ComponentId, FixedBitSet>,
    component_ids: FxHashMap<TypeId, ComponentId>,
}
