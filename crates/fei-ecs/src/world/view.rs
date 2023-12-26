use fei_common::ptr::{
    Ptr, PtrMut, PtrOwned,
};
use crate::{
    component::{
        Component, ComponentId,
        ComponentSet, ComponentSetId,
        Components,
    },
    entity::{
        Entity, Entities,
    },
};

pub struct EntityView<'a> {
    entity: Entity,
    entities: &'a Entities,
    components: &'a Components,
}

impl<'a> EntityView<'a> {
    #[inline]
    pub unsafe fn new(entity: Entity, entities: &'a Entities, components: &'a Components) -> Self {
        Self { entity, entities, components, }
    }

    #[inline]
    pub fn contains<T: Component>(&self) -> bool {
        self.components
            .get_component_id::<T>()
            .is_some_and(|id| self.contains_id(id))
    }

    #[inline]
    pub fn contains_id(&self, id: ComponentId) -> bool {
        unsafe {
            self.entities
                .location(self.entity)
                .is_some_and(|loc| self.components.contains(self.entity, loc, id))
        }
    }

    #[inline]
    pub fn get<T: Component>(&self) -> Option<&T> {
        self.components
            .get_component_id::<T>()
            .and_then(|id| unsafe {
                self.entities
                    .location(self.entity)
                    .and_then(|loc| self.components
                        .contains(self.entity, loc, id)
                        .then(|| self.components.get(self.entity, loc, id).deref())
                    )
            })
    }

    #[inline]
    pub unsafe fn get_by_id(&self, id: ComponentId) -> Ptr {
        let loc = self.entities.location(self.entity).unwrap_unchecked();
        self.components.get(self.entity, loc, id)
    }
}

pub struct EntityViewMut<'a> {
    entity: Entity,
    entities: &'a mut Entities,
    components: &'a mut Components,
}

impl<'a> EntityViewMut<'a> {
    #[inline]
    pub unsafe fn new(entity: Entity, entities: &'a mut Entities, components: &'a mut Components) -> Self {
        Self { entity, entities, components, }
    }

    #[inline]
    pub fn contains<T: Component>(&self) -> bool {
        self.components
            .get_component_id::<T>()
            .is_some_and(|id| self.contains_id(id))
    }

    #[inline]
    pub fn contains_id(&self, id: ComponentId) -> bool {
        unsafe {
            self.entities
                .location(self.entity)
                .is_some_and(|loc| self.components.contains(self.entity, loc, id))
        }
    }

    #[inline]
    pub fn get<T: Component>(&self) -> Option<&T> {
        self.components
            .get_component_id::<T>()
            .and_then(|id| unsafe {
                self.entities
                    .location(self.entity)
                    .and_then(|loc| self.components
                        .contains(self.entity, loc, id)
                        .then(|| self.components.get(self.entity, loc, id).deref())
                    )
            })
    }

    #[inline]
    pub unsafe fn get_by_id(&self, id: ComponentId) -> Ptr {
        let loc = self.entities.location(self.entity).unwrap_unchecked();
        self.components.get(self.entity, loc, id)
    }

    #[inline]
    pub fn get_mut<T: Component>(&mut self) -> Option<&mut T> {
        let id = self.components.register_component::<T>();
        unsafe {
            self.entities
                .location(self.entity)
                .and_then(|loc| self.components
                    .contains(self.entity, loc, id)
                    .then(|| self.components.get_mut(self.entity, loc, id).deref_mut())
                )
        }
    }

    #[inline]
    pub unsafe fn get_by_id_mut(&mut self, id: ComponentId) -> PtrMut {
        let loc = self.entities.location(self.entity).unwrap_unchecked();
        self.components.get_mut(self.entity, loc, id)
    }

    #[inline]
    pub fn insert<T: ComponentSet>(&mut self, set: T) {
        unsafe {
            let id = self.components.register_component_set::<T>();
            PtrOwned::take(set, |ptr| self.insert_by_id(ptr, id));
        }
    }

    #[inline]
    pub unsafe fn insert_by_id(&mut self, set: PtrOwned, set_id: ComponentSetId) {
        self.components.insert_set(self.entity, self.entities, set, set_id)
    }

    #[inline]
    pub fn remove<T: Component>(&mut self) {

    }

    #[inline]
    pub fn remove_all<T: ComponentSet>(&mut self) {

    }
}
