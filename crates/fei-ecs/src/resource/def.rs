use fei_common::prelude::*;

pub trait Resource: 'static + Send + Sync + Sized {}
pub trait ResourceLocal: 'static + Sized {}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct ResourceId(pub(crate) usize);
impl SparseIndex for ResourceId {
    #[inline]
    fn into_index(self) -> usize {
        self.0
    }

    #[inline]
    fn from_index(index: usize) -> Self {
        Self(index)
    }
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub struct ResourceLocalId(pub(crate) usize);
impl SparseIndex for ResourceLocalId {
    #[inline]
    fn into_index(self) -> usize {
        self.0
    }

    #[inline]
    fn from_index(index: usize) -> Self {
        Self(index)
    }
}
