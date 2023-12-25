use fei_common::{
    prelude::*,
    ptr::{
        Ptr, PtrMut, PtrOwned,
    },
};
use crate::{
    entity::{
        Entity, Entities, EntityLocation,
    },
    component::{
        Component, ComponentId, ComponentInfo, ComponentStorage,
        ComponentSet, ComponentSetId, ComponentSetInfo,
        Archetype, ArchetypeId, Table, TableId, Bitset, SparseSets,
    },
};
use fixedbitset::FixedBitSet;
use std::{
    any::TypeId,
    borrow::Cow,
    ptr::{
        self,
        NonNull,
    },
};

/// Component storages, identified by [`Entity`]s.
#[derive(Default)]
pub struct Components {
    bitsets: Bitset,
    sparse_sets: SparseSets,

    tables: Vec<Table>,
    table_ids: FxHashMap<Box<[ComponentId]>, TableId>,

    archetypes: Vec<Archetype>,
    archetype_keys: FxHashMap<Box<[ComponentId]>, ArchetypeId>,
    archetype_starts: SparseSet<ComponentSetId, ArchetypeId>,

    component_info: Vec<ComponentInfo>,
    component_ids: FxHashMap<TypeId, ComponentId>,

    component_set_info: Vec<ComponentSetInfo>,
    component_set_ids: FxHashMap<TypeId, ComponentSetId>,
}

impl Components {
    #[inline]
    pub fn register_component<T: Component>(&mut self) -> ComponentId {
        // Safety: Type ID and layout information matches.
        unsafe { Self::register_component_impl(
            &mut self.bitsets, &mut self.sparse_sets,
            &mut self.component_info, &mut self.component_ids,
            TypeId::of::<T>(), ComponentInfo::new::<T>(),
        ) }
    }

    #[inline]
    pub unsafe fn register_component_raw(&mut self, type_id: TypeId, info: ComponentInfo) -> ComponentId {
        Self::register_component_impl(
            &mut self.bitsets, &mut self.sparse_sets,
            &mut self.component_info, &mut self.component_ids,
            type_id, info,
        )
    }

    unsafe fn register_component_impl(
        bitsets: &mut Bitset,
        sparse_sets: &mut SparseSets,
        component_info: &mut Vec<ComponentInfo>,
        component_ids: &mut FxHashMap<TypeId, ComponentId>,
        type_id: TypeId, info: ComponentInfo,
    ) -> ComponentId {
        *component_ids.entry(type_id).or_insert_with(|| {
            component_info.reserve_exact(1);
            component_info.push(info);

            let id = ComponentId(component_info.len() - 1);
            if let Some(ComponentStorage::SparseSet) = info.storage() {
                sparse_sets.init(id, info);
            } else {
                bitsets.init(id, info.dropper());
            }

            id
        })
    }

    #[inline]
    pub fn get_component_id<T: Component>(&self) -> Option<ComponentId> {
        self.component_ids.get(&TypeId::of::<T>()).copied()
    }

    pub fn register_component_set<T: ComponentSet>(&mut self) -> ComponentSetId {
        *self.component_set_ids.entry(TypeId::of::<T>()).or_insert_with(|| {
            let set_info = ComponentSetInfo::new::<T>(|type_id, component_info| unsafe {
                Self::register_component_impl(
                    &mut self.bitsets, &mut self.sparse_sets,
                    &mut self.component_info, &mut self.component_ids,
                    type_id, component_info,
                )
            });
            self.component_set_info.reserve_exact(1);
            self.component_set_info.push(set_info);

            ComponentSetId(self.component_set_info.len() - 1)
        })
    }

    pub unsafe fn contains(&self, entity: Entity, location: EntityLocation, id: ComponentId) -> bool {
        let info = *self.component_info.get_unchecked(id.0);
        if let Some(storage) = info.storage() {
            match storage {
                ComponentStorage::SparseSet => self.sparse_sets.contains(entity, id),
                ComponentStorage::Table => {
                    let arch = self.archetypes.get_unchecked(location.archetype_id.0);
                    let table = self.tables.get_unchecked(arch.table_id.unwrap_unchecked().0);
                    table.component_bits.contains(id.0)
                },
            }
        } else {
            self.bitsets.contains(entity, id)
        }
    }

    pub unsafe fn get(&self, entity: Entity, location: EntityLocation, id: ComponentId) -> Ptr {
        let info = *self.component_info.get_unchecked(id.0);
        if let Some(storage) = info.storage() {
            match storage {
                ComponentStorage::SparseSet => self.sparse_sets.get(entity, id),
                ComponentStorage::Table => {
                    let arch = self.archetypes.get_unchecked(location.archetype_id.0);
                    let table = self.tables.get_unchecked(arch.table_id.unwrap_unchecked().0);
                    table.get(location.table_index.unwrap_unchecked(), id)
                },
            }
        } else {
            Ptr::new(NonNull::dangling())
        }
    }

    pub unsafe fn get_mut(&mut self, entity: Entity, location: EntityLocation, id: ComponentId) -> PtrMut {
        let info = *self.component_info.get_unchecked(id.0);
        if let Some(storage) = info.storage() {
            match storage {
                ComponentStorage::SparseSet => self.sparse_sets.get_mut(entity, id),
                ComponentStorage::Table => {
                    let arch = self.archetypes.get_unchecked(location.archetype_id.0);
                    let table = self.tables.get_unchecked_mut(arch.table_id.unwrap_unchecked().0);
                    table.get_mut(location.table_index.unwrap_unchecked(), id)
                },
            }
        } else {
            PtrMut::new(NonNull::dangling())
        }
    }

    pub unsafe fn insert(&mut self, entity: Entity, entities: &mut Entities, set: PtrOwned, set_id: ComponentSetId) {
        let location = entities.location_mut(entity);
        let set_info = self.component_set_info.get_unchecked(set_id.0);

        let (from_id, to_id) = if let Some(location) = location.as_mut() {
            let arch_id = location.archetype_id;
            let arch = self.archetypes.get_unchecked_mut(arch_id.0);

            if let Some(&target_id) = arch.insertions.get(set_id) {
                (Some(arch_id), target_id)
            } else {
                if set_info.component_bits.is_subset(&arch.component_bits) {
                    arch.insertions.insert(set_id, arch_id);
                    (Some(arch_id), arch_id)
                } else {
                    let component_bits = &arch.component_bits | &set_info.component_bits;
                    let key = component_bits
                        .ones().fold(Vec::with_capacity(component_bits.count_ones(..)), |mut accum, id| {
                            accum.push(ComponentId(id));
                            accum
                        });

                    let target_id = Self::get_archetype(
                        &mut self.tables, &mut self.table_ids,
                        &mut self.archetypes, &mut self.archetype_keys,
                        &self.component_info,
                        Cow::Owned(key), Cow::Owned(component_bits),
                    );

                    self.archetypes.get_unchecked_mut(arch_id.0).insertions.insert(set_id, target_id);
                    (Some(arch_id), target_id)
                }
            }
        } else {
            (None, if let Some(&arch_id) = self.archetype_starts.get(set_id) {
                arch_id
            } else {
                let arch_id = Self::get_archetype(
                    &mut self.tables, &mut self.table_ids,
                    &mut self.archetypes, &mut self.archetype_keys,
                    &self.component_info,
                    Cow::Borrowed(&set_info.components), Cow::Borrowed(&set_info.component_bits),
                );

                self.archetype_starts.insert(set_id, arch_id);
                arch_id
            })
        };

        self.sparse_sets.insert(entity, ptr::read(&set), set_info);
        self.bitsets.insert(entity, set_info);
        if let Some(from_id) = from_id {
            let loc = location.as_mut().unwrap_unchecked();
            loc.archetype_id = to_id;

            if from_id != to_id {
                let [from_arch, to_arch] = self.archetypes.many_unchecked_mut([from_id.0, to_id.0]);
                if let Some(to_table_id) = to_arch.table_id {
                    if let Some(from_table_id) = from_arch.table_id {
                        if from_table_id != to_table_id {
                            let [from_table, to_table] = self.tables.many_unchecked_mut([from_table_id.0, to_table_id.0]);
                            let from_index = loc.table_index.unwrap_unchecked();

                            let (swapped, table_index) = to_table.insert_from(
                                from_table, from_index,
                                set, set_info,
                            );

                            loc.table_index = Some(table_index);
                            if let Some(swapped) = swapped {
                                let swapped_loc = entities.location_mut(swapped).as_mut().unwrap_unchecked();
                                swapped_loc.table_index = Some(from_index);
                            }
                        } else {
                            let table = self.tables.get_unchecked_mut(to_table_id.0);
                            table.update(loc.table_index.unwrap_unchecked(), set, set_info);
                        }
                    } else {
                        let table = self.tables.get_unchecked_mut(to_table_id.0);
                        loc.table_index = Some(table.insert(entity, set, set_info));
                    }
                }
            } else {
                let arch = self.archetypes.get_unchecked_mut(from_id.0);
                if let Some(table_id) = arch.table_id {
                    let table = self.tables.get_unchecked_mut(table_id.0);
                    table.update(loc.table_index.unwrap_unchecked(), set, set_info);
                }
            }
        } else {
            let arch = self.archetypes.get_unchecked_mut(to_id.0);
            let mut new_loc = EntityLocation {
                archetype_id: to_id,
                table_index: None,
            };

            if let Some(table_id) = arch.table_id {
                let table = self.tables.get_unchecked_mut(table_id.0);
                new_loc.table_index = Some(table.insert(entity, set, set_info));
            }

            *location = Some(new_loc);
        }
    }

    pub unsafe fn remove_set(&mut self, entity: Entity, entities: &mut Entities, set_id: ComponentSetId) {
        let location = entities.location_mut(entity);
        let set_info = self.component_set_info.get_unchecked(set_id.0);

        let Some(loc) = location.as_mut() else { return };
        let (from_id, to_id) = {
            let arch_id = loc.archetype_id;
            let arch = self.archetypes.get_unchecked_mut(arch_id.0);

            if let Some(&target_id) = arch.removals.get(set_id) {
                (arch_id, target_id)
            } else {
                if arch.component_bits.is_subset(&set_info.component_bits) {
                    arch.removals.insert(set_id, None);
                    (arch_id, None)
                } else if arch.component_bits.is_disjoint(&set_info.component_bits) {
                    arch.removals.insert(set_id, Some(arch_id));
                    (arch_id, Some(arch_id))
                } else {
                    let mut component_bits = arch.component_bits.clone();
                    component_bits.difference_with(&set_info.component_bits);
                    let key = component_bits
                        .ones().fold(Vec::with_capacity(component_bits.count_ones(..)), |mut accum, id| {
                            accum.push(ComponentId(id));
                            accum
                        });

                    let target_id = Self::get_archetype(
                        &mut self.tables, &mut self.table_ids,
                        &mut self.archetypes, &mut self.archetype_keys,
                        &self.component_info,
                        Cow::Owned(key), Cow::Owned(component_bits),
                    );

                    self.archetypes.get_unchecked_mut(arch_id.0).removals.insert(set_id, Some(target_id));
                    (arch_id, Some(target_id))
                }
            }
        };

        self.sparse_sets.remove(entity, &set_info.sparse_set_components);
        self.bitsets.remove(entity, &set_info.zst_components);
        if let Some(to_id) = to_id {
            loc.archetype_id = to_id;
            if from_id != to_id {
                let [from_arch, to_arch] = self.archetypes.many_unchecked_mut([from_id.0, to_id.0]);
                if let Some(from_table_id) = from_arch.table_id {
                    let from_index = loc.table_index.unwrap_unchecked();
                    if let Some(to_table_id) = to_arch.table_id {
                        if from_table_id != to_table_id {
                            let [from_table, to_table] = self.tables.many_unchecked_mut([from_table_id.0, to_table_id.0]);
                            let (swapped, table_index) = to_table.remove_from(
                                from_table, from_index,
                            );

                            loc.table_index = Some(table_index);
                            if let Some(swapped) = swapped {
                                let swapped_loc = entities.location_mut(swapped).as_mut().unwrap_unchecked();
                                swapped_loc.table_index = Some(from_index);
                            }
                        }
                    } else {
                        loc.table_index = None;

                        let table = self.tables.get_unchecked_mut(from_table_id.0);
                        if let Some(swapped) = table.remove(from_index) {
                            let swapped_loc = entities.location_mut(swapped).as_mut().unwrap_unchecked();
                            swapped_loc.table_index = Some(from_index);
                        }
                    }
                }
            }
        } else {
            let loc = location.take().unwrap_unchecked();
            let arch = self.archetypes.get_unchecked_mut(from_id.0);
            if let Some(table_id) = arch.table_id {
                let table = self.tables.get_unchecked_mut(table_id.0);
                let index = loc.table_index.unwrap_unchecked();
                if let Some(swapped) = table.remove(index) {
                    let swapped_loc = entities.location_mut(swapped).as_mut().unwrap_unchecked();
                    swapped_loc.table_index = Some(index);
                }
            }
        }
    }

    pub unsafe fn remove_all(&mut self, entity: Entity, entities: &mut Entities) {
        let Some(loc) = entities.location_mut(entity).take() else { return };
        let arch = self.archetypes.get_unchecked(loc.archetype_id.0);

        self.sparse_sets.remove(entity, &arch.sparse_set_components);
        self.bitsets.remove(entity, &arch.zst_components);
        if let Some(table_id) = arch.table_id {
            let table = self.tables.get_unchecked_mut(table_id.0);
            let index = loc.table_index.unwrap_unchecked();

            if let Some(swapped) = table.remove(index) {
                let swapped_loc = entities.location_mut(swapped).as_mut().unwrap_unchecked();
                swapped_loc.table_index = Some(index);
            }
        }
    }

    unsafe fn get_archetype(
        tables: &mut Vec<Table>,
        table_ids: &mut FxHashMap<Box<[ComponentId]>, TableId>,

        archetypes: &mut Vec<Archetype>,
        archetype_keys: &mut FxHashMap<Box<[ComponentId]>, ArchetypeId>,

        component_info: &Vec<ComponentInfo>,
        components: Cow<'_, [ComponentId]>, component_bits: Cow<'_, FixedBitSet>,
    ) -> ArchetypeId {
        let closure = |key: &[ComponentId]| {
            let new_arch = Archetype::new(
                component_bits.into_owned(),
                key, |id| *component_info.get_unchecked(id.0),
                |table_components| *table_ids.entry_ref(table_components).or_insert_with_key(|key| {
                    let new_table = Table::new(
                        key,
                        |id| *component_info.get_unchecked(id.0),
                    );

                    tables.reserve_exact(1);
                    tables.push(new_table);
                    TableId(tables.len() - 1)
                }),
            );

            archetypes.reserve_exact(1);
            archetypes.push(new_arch);
            ArchetypeId(archetypes.len() - 1)
        };

        *match components {
            Cow::Borrowed(key) => archetype_keys.entry_ref(key).or_insert_with_key(closure),
            Cow::Owned(key) => archetype_keys.entry(key.into_boxed_slice()).or_insert_with_key(|key| closure(key)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fei_ecs_macros::{
        Component, ComponentSet,
    };

    #[derive(Component)]
    #[component(storage = "Table")]
    struct TableStored(String);
    impl Drop for TableStored {
        #[inline]
        fn drop(&mut self) {
            println!("Dropped `{:?}`: {}", Self::STORAGE, self.0);
        }
    }

    #[derive(Component)]
    #[component(storage = "SparseSet")]
    struct SetStored(f32);
    impl Drop for SetStored {
        #[inline]
        fn drop(&mut self) {
            println!("Dropped `{:?}`: {}", Self::STORAGE, self.0);
        }
    }

    #[derive(Component)]
    pub struct BitStored;
    impl Drop for BitStored {
        #[inline]
        fn drop(&mut self) {
            println!("Dropped `BitSet`");
        }
    }

    #[derive(ComponentSet)]
    pub struct AllSet {
        table: TableStored,
        set: SetStored,
        bit: BitStored,
    }

    #[test]
    fn insert_remove() -> anyhow::Result<()> {
        let mut components = Components::default();
        let tabs_id = components.register_component_set::<TableStored>();
        let sets_id = components.register_component_set::<SetStored>();
        let bits_id = components.register_component_set::<BitStored>();
        let all_id = components.register_component_set::<AllSet>();

        let mut entities = Entities::default();
        let a = entities.spawn()?;
        let b = entities.spawn()?;

        unsafe {
            println!("===> Insert table/'fei' to A");
            PtrOwned::take(TableStored("fei".to_string()), |ptr| components.insert(a, &mut entities, ptr, tabs_id));
            println!("===> Insert table/'is' to A");
            PtrOwned::take(TableStored("is".to_string()), |ptr| components.insert(a, &mut entities, ptr, tabs_id));
            println!("===> Insert table/'short' to A");
            PtrOwned::take(TableStored("short".to_string()), |ptr| components.insert(a, &mut entities, ptr, tabs_id));

            println!("===> Insert set/6.942 to A");
            PtrOwned::take(SetStored(6.942), |ptr| components.insert(a, &mut entities, ptr, sets_id));

            println!("===> Insert bit to A");
            components.insert(a, &mut entities, PtrOwned::new(NonNull::dangling()), bits_id);

            println!("===> Insert table/'fei' to B");
            PtrOwned::take(TableStored("fei".to_string()), |ptr| components.insert(b, &mut entities, ptr, tabs_id));
            println!("===> Insert table/'is' to B");
            PtrOwned::take(TableStored("is".to_string()), |ptr| components.insert(b, &mut entities, ptr, tabs_id));

            println!("===> Remove A");
            components.remove_all(a, &mut entities);
            entities.free(a);
        }

        println!("===> Drop all.");
        Ok(())
    }
}
