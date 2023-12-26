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
use std::ptr::{
    self,
    NonNull,
};

pub(super) struct Archetype {
    pub component_bits: FixedBitSet,
    pub sparse_set_components: Box<[ComponentId]>,
    pub zst_components: Box<[ComponentId]>,

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
        let (table_components, sparse_set_components, zst_components) = components.iter().fold(
            (Vec::new(), Vec::new(), Vec::new()),
            |(mut table_components, mut sparse_set_components, mut zst_components), &id| {
                let info = get_info(id);
                match info.storage() {
                    Some(ComponentStorage::Table) => &mut table_components,
                    Some(ComponentStorage::SparseSet) => &mut sparse_set_components,
                    None => &mut zst_components,
                }.push(id);

                (table_components, sparse_set_components, zst_components)
            },
        );

        Self {
            component_bits,
            sparse_set_components: sparse_set_components.into_boxed_slice(),
            zst_components: zst_components.into_boxed_slice(),

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
        for &id in &*set_info.table_components {
            self.columns
                .get_unchecked_mut(id)
                .set_unchecked(index, ptr::read(&set).byte_add(*set_info.component_offsets.get_unchecked(id)));
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

        for &id in &*self.components {
            let to = self.columns.get_unchecked_mut(id);
            if let Some(from) = from.columns.get_mut(id) {
                if let Some(&offset) = set_info.component_offsets.get(id) {
                    from.swap_remove_unchecked_and_drop(from_index);
                    to.push(ptr::read(&set).byte_add(offset));
                } else {
                    from.swap_remove_unchecked(from_index, |ptr| to.push(ptr));
                }
            } else {
                to.push(ptr::read(&set).byte_add(*set_info.component_offsets.get_unchecked(id)));
            }
        }

        (from.entities.get(from_index).copied(), self.entities.len() - 1)
    }

    #[inline]
    #[must_use = "use the returned value as the entity's archetypal index"]
    pub unsafe fn remove_from(
        &mut self,
        from: &mut Self, from_index: usize,
    ) -> (Option<Entity>, usize) {
        let entity = from.entities.swap_remove(from_index);
        self.entities.push(entity);

        for &id in &*from.components {
            let from = from.columns.get_unchecked_mut(id);
            if let Some(to) = self.columns.get_mut(id) {
                from.swap_remove_unchecked(from_index, |ptr| to.push(ptr));
            } else {
                from.swap_remove_unchecked_and_drop(from_index);
            }
        }

        (from.entities.get(from_index).copied(), self.entities.len() - 1)
    }

    #[inline]
    #[must_use = "use the returned value as the entity's archetypal index"]
    pub unsafe fn extract_from(
        &mut self,
        from: &mut Self, from_index: usize,
        mut extract: impl FnMut(ComponentId, PtrOwned),
    ) -> (Option<Entity>, usize) {
        let entity = from.entities.swap_remove(from_index);
        self.entities.push(entity);

        for &id in &*from.components {
            from.columns
                .get_unchecked_mut(id)
                .swap_remove_unchecked(from_index, |ptr| if let Some(to) = self.columns.get_mut(id) {
                    to.push(ptr);
                } else {
                    extract(id, ptr);
                });
        }

        (from.entities.get(from_index).copied(), self.entities.len() - 1)
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

    #[inline]
    #[must_use = "use the returned value as the entity's archetypal index"]
    pub unsafe fn extract(&mut self, index: usize, mut extract: impl FnMut(ComponentId, PtrOwned)) -> Option<Entity> {
        self.entities.swap_remove(index);
        for &id in &*self.components {
            self.columns.get_unchecked_mut(id).swap_remove_unchecked(index, |ptr| extract(id, ptr));
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
    pub unsafe fn remove(&mut self, entity: Entity, components: &[ComponentId]) {
        let index = entity.id();
        for &id in components {
            self.sets
                .get_unchecked_mut(id)
                .remove_and_drop(index);
        }
    }

    #[inline]
    pub unsafe fn extract(&mut self, entity: Entity, components: &[ComponentId], mut extract: impl FnMut(ComponentId, PtrOwned)) {
        let index = entity.id();
        for &id in components {
            self.sets
                .get_unchecked_mut(id)
                .remove(index, |ptr| extract(id, ptr));
        }
    }
}

#[derive(Default)]
pub(super) struct Bitset {
    sets: SparseSet<ComponentId, (FixedBitSet, Option<unsafe fn(*mut u8)>)>,
}

impl Bitset {
    #[inline]
    pub fn init(&mut self, id: ComponentId, dropper: Option<unsafe fn(*mut u8)>) {
        self.sets.insert(id, (FixedBitSet::new(), dropper));
    }

    #[inline]
    pub unsafe fn contains(&self, entity: Entity, id: ComponentId) -> bool {
        let (set, ..) = self.sets.get_unchecked(id);
        set.contains(entity.id() as usize)
    }

    #[inline]
    pub unsafe fn insert(&mut self, entity: Entity, set_info: &ComponentSetInfo) {
        let index = entity.id() as usize;
        for &id in &*set_info.zst_components {
            let (set, dropper) = self.sets.get_unchecked_mut(id);
            set.grow(index + 1);

            if set.put(index) {
                if let Some(dropper) = *dropper {
                    dropper(NonNull::<()>::dangling().cast::<u8>().as_ptr());
                }
            }
        }
    }

    #[inline]
    pub unsafe fn remove(&mut self, entity: Entity, components: &[ComponentId]) {
        let index = entity.id() as usize;
        for &id in components {
            let (set, dropper) = self.sets.get_unchecked_mut(id);
            if set.contains(index) {
                set.set(index, false);
                if let Some(dropper) = *dropper {
                    dropper(NonNull::<()>::dangling().cast::<u8>().as_ptr());
                }
            }
        }
    }

    #[inline]
    pub unsafe fn extract(&mut self, entity: Entity, components: &[ComponentId]) {
        let index = entity.id() as usize;
        for &id in components {
            let (set, ..) = self.sets.get_unchecked_mut(id);
            set.set(index, false);
        }
    }
}

impl Drop for Bitset {
    #[inline]
    fn drop(&mut self) {
        for (set, dropper) in self.sets.iter_sparse() {
            if let Some(dropper) = dropper {
                for _ in 0..set.count_ones(..) {
                    unsafe { dropper(NonNull::<()>::dangling().cast::<u8>().as_ptr()) };
                }
            }
        }
    }
}
