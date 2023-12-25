use fei_common::{
    prelude::*,
    drop_for,
};
use fixedbitset::FixedBitSet;
use std::{
    any::{
        TypeId,
        type_name,
    },
    alloc::Layout,
    mem::{
        self,
        MaybeUninit,
    },
    ptr::addr_of,
};

/// Kinds of component storages, each with their own benefits. Note that [zero-sized types](
/// https://doc.rust-lang.org/nomicon/exotic-sizes.html#zero-sized-types-zsts) always use bitsets as
/// the container, indexed by [`crate::entity::Entity::id`].
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ComponentStorage {
    /// A table storage stores archetypes (i.e., the set of all components that belongs to an
    /// [`Entity`](crate::entity::Entity)) as a structure of arrays, offering faster iteration and cheaper memory requirements.
    Table,
    /// A sparse set storage stores each component types separately in a sparse set indexed by
    /// [`Entity::id`](crate::entity::Entity::id), offering faster addition and removal.
    SparseSet,
}

pub trait Component: 'static + Send + Sync {
    /// Storage type for this component type. Setting this is no-op for [zero-sized types](
    /// https://doc.rust-lang.org/nomicon/exotic-sizes.html#zero-sized-types-zsts), as the storages
    /// for those will always be bitsets indexed by [`crate::entity::Entity::id`].
    const STORAGE: ComponentStorage = ComponentStorage::Table;
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
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

#[derive(Copy, Clone)]
pub struct ComponentInfo {
    layout: Layout,
    storage: ComponentStorage,
    dropper: Option<unsafe fn(*mut u8)>,
}

impl ComponentInfo {
    #[inline]
    pub const fn new<T: Component>() -> Self {
        Self {
            layout: Layout::new::<T>(),
            storage: T::STORAGE,
            dropper: drop_for::<T>(),
        }
    }

    #[inline]
    pub const fn is_zst(&self) -> bool {
        self.layout.size() == 0
    }

    #[inline]
    pub const fn layout(&self) -> Layout {
        self.layout
    }

    #[inline]
    pub const fn storage(&self) -> Option<ComponentStorage> {
        if self.is_zst() {
            None
        } else {
            Some(self.storage)
        }
    }

    #[inline]
    pub const fn dropper(&self) -> Option<unsafe fn(*mut u8)> {
        self.dropper
    }
}

pub unsafe trait ComponentSet: 'static + Send + Sync {
    fn metadata(base_offset: usize, callback: &mut impl FnMut(usize, TypeId, ComponentInfo));
}

unsafe impl<T: Component> ComponentSet for T {
    #[inline]
    fn metadata(base_offset: usize, callback: &mut impl FnMut(usize, TypeId, ComponentInfo)) {
        callback(base_offset, TypeId::of::<T>(), ComponentInfo::new::<T>());
    }
}

macro_rules! impl_component_set {
    ($($tuple_type:ident $tuple_index:tt),*) => {
        unsafe impl<$($tuple_type: ComponentSet,)*> ComponentSet for ($($tuple_type,)*) {
            #[inline]
            fn metadata(base_offset: usize, callback: &mut impl FnMut(usize, TypeId, ComponentInfo)) {
                let uninit = MaybeUninit::<Self>::uninit();
                let base = uninit.as_ptr();

                unsafe { $(
                    let addr = addr_of!((*base).$tuple_index);
                    assert_eq!(
                        addr.align_offset(mem::align_of::<$tuple_type>()), 0,
                        "field number {} of type {} isn't aligned",
                        stringify!($tuple_index), type_name::<$tuple_type>(),
                    );

                    $tuple_type::metadata(base_offset + (addr as usize - base as usize), callback);
                )* }
            }
        }
    }
} impl_tuples!(impl_component_set! 1 8);

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct ComponentSetId(pub(crate) usize);
impl SparseIndex for ComponentSetId {
    #[inline]
    fn into_index(self) -> usize {
        self.0
    }

    #[inline]
    fn from_index(index: usize) -> Self {
        Self(index)
    }
}

#[derive(Clone)]
pub struct ComponentSetInfo {
    pub(super) components: Box<[ComponentId]>,
    pub(super) component_bits: FixedBitSet,
    pub(super) component_offsets: SparseSet<ComponentId, usize>,

    pub(super) sparse_set_components: Box<[ComponentId]>,
    pub(super) zst_components: Box<[ComponentId]>,
}

impl ComponentSetInfo {
    pub fn new<T: ComponentSet>(mut register_component: impl FnMut(TypeId, ComponentInfo) -> ComponentId) -> Self {
        let mut offsets = Vec::new();
        let mut sparse_set_components = Vec::new();
        let mut zst_components = Vec::new();

        T::metadata(0, &mut |offset, type_id, info| {
            let id = register_component(type_id, info);
            offsets.push((offset, id));

            if info.is_zst() {
                zst_components.push(id);
            } else if info.storage == ComponentStorage::SparseSet {
                sparse_set_components.push(id);
            }
        });

        offsets.sort_unstable_by_key(|&(.., ComponentId(id))| id);
        sparse_set_components.sort_unstable();
        zst_components.sort_unstable();

        let id_len = unsafe { offsets.last().unwrap_unchecked() }.1.0 + 1;
        let mut components = Vec::with_capacity(offsets.len());
        let mut component_bits = FixedBitSet::with_capacity(id_len);
        let mut component_offsets = SparseSet::with_capacity(id_len);

        for (offset, id) in offsets {
            if component_offsets.insert(id, offset).is_some() {
                panic!("duplicate component for set `{}`", type_name::<T>());
            } else {
                components.push(id);
                component_bits.insert(id.0);
            }
        }

        Self {
            components: components.into_boxed_slice(),
            component_bits,
            component_offsets,

            sparse_set_components: sparse_set_components.into_boxed_slice(),
            zst_components: zst_components.into_boxed_slice(),
        }
    }
}
