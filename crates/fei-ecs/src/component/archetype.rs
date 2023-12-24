use fei_common::{
    prelude::*,
    ptr::{
        Ptr, PtrMut, PtrOwned,
    },
};
use crate::{
    component::{
        ComponentId, ComponentInfo, ComponentStorage,
        ComponentSetId, ComponentSetInfo,
    },
    entity::Entity,
};
use fixedbitset::FixedBitSet;
use std::ptr;

pub(super) struct Archetype {
    pub component_bits: FixedBitSet,
    pub table_id: Option<TableId>,

    /// Always has a target ID: same ID as self if the inserter is a subset, new ID otherwise.
    pub insertions: SparseSet<ComponentSetId, ArchetypeId>,
    /// [`None`] if the remover is a superset, [`Some`] of the same ID if the remover is disjoint,
    /// and [`Some`] of a new ID otherwise.
    pub removals: SparseSet<ComponentSetId, Option<ArchetypeId>>,
}

impl Archetype {
    pub unsafe fn new(
        component_bits: FixedBitSet,
        components: &[ComponentId], mut get_info: impl FnMut(ComponentId) -> ComponentInfo,
        mut get_table: impl FnMut(&[ComponentId]) -> TableId,
    ) -> Self {
        let table_components = components
            .iter().filter(|&&id| get_info(id).storage() == Some(ComponentStorage::Table))
            .fold(Vec::new(), |mut accum, &id| {
                accum.push(id);
                accum
            });

        Self {
            component_bits,
            table_id: (!table_components.is_empty()).then(|| get_table(&table_components)),

            insertions: SparseSet::new(),
            removals: SparseSet::new(),
        }
    }
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub(crate) struct ArchetypeId(pub usize);
impl SparseIndex for ArchetypeId {
    #[inline]
    fn into_index(self) -> usize {
        self.0
    }

    #[inline]
    fn from_index(index: usize) -> Self {
        Self(index)
    }
}

pub(super) struct Table {
    components: Box<[ComponentId]>,
    pub component_bits: FixedBitSet,
    entities: Vec<Entity>,
    columns: SparseSet<ComponentId, DynVec>,
}

impl Table {
    pub unsafe fn new(components: &[ComponentId], mut get_info: impl FnMut(ComponentId) -> ComponentInfo) -> Self {
        let id_len = components.last().unwrap_unchecked().0 + 1;
        let mut columns = SparseSet::with_capacity(id_len);
        let mut component_bits = FixedBitSet::with_capacity(id_len);

        for &id in components {
            let info = get_info(id);
            columns.insert(id, DynVec::new(info.layout(), info.dropper().into()));
            component_bits.insert(id.0);
        }

        Self {
            components: components.into(),
            component_bits,
            entities: Vec::new(),
            columns,
        }
    }

    #[inline]
    pub unsafe fn get(&self, index: usize, id: ComponentId) -> Ptr {
        self.columns
            .get_unchecked(id)
            .get_unchecked(index)
    }

    #[inline]
    pub unsafe fn get_mut(&mut self, index: usize, id: ComponentId) -> PtrMut {
        self.columns
            .get_unchecked_mut(id)
            .get_unchecked_mut(index)
    }

    #[inline]
    #[must_use = "use the returned value as the entity's archetypal index"]
    pub unsafe fn insert(&mut self, entity: Entity, set: PtrOwned, set_info: &ComponentSetInfo) -> usize {
        self.entities.push(entity);
        for &id in &*self.components {
            self.columns
                .get_unchecked_mut(id)
                .push(ptr::read(&set).byte_add(*set_info.component_offsets.get_unchecked(id)));
        }

        self.entities.len() - 1
    }

    #[inline]
    pub unsafe fn update(&mut self, index: usize, set: PtrOwned, set_info: &ComponentSetInfo) {
        for &id in &*set_info.components {
            self.columns
                .get_unchecked_mut(id)
                .set_unchecked(index, ptr::read(&set).byte_add(*set_info.component_offsets.get_unchecked(id)))
        }
    }

    #[inline]
    #[must_use = "use the returned value as the entity's archetypal index"]
    pub unsafe fn insert_from(
        &mut self,
        from: &mut Self, from_index: usize,
        set: PtrOwned, set_info: &ComponentSetInfo,
    ) -> (Option<Entity>, usize) {
        let entity = from.entities.swap_remove(from_index);
        self.entities.push(entity);

        for &id in &*from.components {
            let from = from.columns.get_unchecked_mut(id);
            let to = self.columns.get_unchecked_mut(id);

            if let Some(&offset) = set_info.component_offsets.get(id) {
                from.swap_remove_unchecked_and_drop(from_index);
                to.push(ptr::read(&set).byte_add(offset));
            } else {
                from.swap_remove_unchecked(from_index, |ptr| to.push(ptr));
            }
        }

        (from.entities.get(from_index).copied(), self.entities.len() - 1)
    }

    #[inline]
    #[must_use = "use the returned value as the entity's archetypal index"]
    pub unsafe fn remove_from(
        &mut self,
        from: &mut Self, from_index: usize,
        set_info: &ComponentSetInfo,
    ) -> (Option<Entity>, usize) {

    }

    #[inline]
    #[must_use = "use the returned value as the entity's archetypal index"]
    pub unsafe fn remove(&mut self, index: usize) -> Option<Entity> {
        self.entities.swap_remove(index);
        for &id in &*self.components {
            self.columns.get_unchecked_mut(id).swap_remove_unchecked_and_drop(index);
        }

        self.entities.get(index).copied()
    }
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub(super) struct TableId(pub usize);
impl SparseIndex for TableId {
    #[inline]
    fn into_index(self) -> usize {
        self.0
    }

    #[inline]
    fn from_index(index: usize) -> Self {
        Self(index)
    }
}

#[derive(Default)]
pub(super) struct SparseSets {
    sets: SparseSet<ComponentId, DynSparseSet<u32>>,
}

impl SparseSets {
    #[inline]
    pub fn init(&mut self, id: ComponentId, info: ComponentInfo) {
        self.sets.insert(id, unsafe { DynSparseSet::new(info.layout(), info.dropper()) });
    }

    #[inline]
    pub unsafe fn contains(&self, entity: Entity, id: ComponentId) -> bool {
        self.sets.get_unchecked(id).contains(entity.id())
    }

    #[inline]
    pub unsafe fn get(&self, entity: Entity, id: ComponentId) -> Ptr {
        self.sets.get_unchecked(id).get_unchecked(entity.id())
    }

    #[inline]
    pub unsafe fn get_mut(&mut self, entity: Entity, id: ComponentId) -> PtrMut {
        self.sets.get_unchecked_mut(id).get_unchecked_mut(entity.id())
    }

    #[inline]
    pub unsafe fn insert(&mut self, entity: Entity, set: PtrOwned, set_info: &ComponentSetInfo) {
        let index = entity.id();
        for &id in &*set_info.sparse_set_components {
            self.sets
                .get_unchecked_mut(id)
                .insert_and_drop(index, ptr::read(&set).byte_add(*set_info.component_offsets.get_unchecked(id)));
        }
    }

    #[inline]
    pub unsafe fn remove(&mut self, entity: Entity, set_info: &ComponentSetInfo) {
        let index = entity.id();
        for &id in &*set_info.sparse_set_components {
            self.sets
                .get_unchecked_mut(id)
                .remove_and_drop(index);
        }
    }
}

#[derive(Default)]
pub(super) struct Bitsets {
    sets: SparseSet<ComponentId, FixedBitSet>,
}

impl Bitsets {
    #[inline]
    pub fn init(&mut self, id: ComponentId) {
        self.sets.insert(id, FixedBitSet::new());
    }

    #[inline]
    pub unsafe fn contains(&self, entity: Entity, id: ComponentId) -> bool {
        self.sets.get_unchecked(id).contains(entity.id() as usize)
    }

    #[inline]
    pub unsafe fn insert(&mut self, entity: Entity, set_info: &ComponentSetInfo) {
        let index = entity.id() as usize;
        for &id in &*set_info.zst_components {
            let set = self.sets.get_unchecked_mut(id);
            set.grow(index + 1);
            set.insert(index);
        }
    }

    #[inline]
    pub unsafe fn remove(&mut self, entity: Entity, set_info: &ComponentSetInfo) {
        let index = entity.id() as usize;
        for &id in &*set_info.zst_components {
            let set = self.sets.get_unchecked_mut(id);
            if set.contains(index) {
                set.set(index, false);
            }
        }
    }
}
