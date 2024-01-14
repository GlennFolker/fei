use fei_common::prelude::*;
use crate::resource::Resources;

pub(super) mod sealed {
    pub trait Sealed {}
}

pub unsafe trait Resource: 'static + Sized {
    type Query: ResQuery;
}

pub trait ResQuery: sealed::Sealed {
    type Output<T>;

    unsafe fn insert<T: Resource>(resources: &mut Resources, id: ResourceId, resource: T) -> Self::Output<Option<T>>;

    unsafe fn remove<T: Resource>(resources: &mut Resources, id: ResourceId) -> Self::Output<Option<T>>;

    unsafe fn get<T: Resource>(resources: &Resources, id: Option<ResourceId>) -> Self::Output<Option<&T>>;

    unsafe fn get_mut<T: Resource>(resources: &mut Resources, id: ResourceId) -> Self::Output<Option<&mut T>>;
}

pub struct IsSend;
pub struct NoSend;

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
