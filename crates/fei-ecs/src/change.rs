use fei_common::{
    ptr::{
        Ptr, PtrMut,
    },
};
use std::{
    marker::PhantomData,
    ops::{
        Deref, DerefMut,
    },
    ptr::NonNull,
};

#[derive(Default, Copy, Clone, Eq, PartialEq)]
pub struct ChangeMark {
    // TODO doesn't deal with integer space wraparounds yet.
    tick: u32,
}

impl ChangeMark {
    #[inline]
    pub fn newer_than(self, other: Self) -> bool {
        self.tick > other.tick
    }
}

#[derive(Default, Copy, Clone, Eq, PartialEq)]
pub struct ChangeMarks {
    pub(crate) added: ChangeMark,
    pub(crate) updated: ChangeMark,
}

pub trait ChangeAware<'a> {
    type Target<'t> where 'a: 't, Self: 't;

    fn is_added(&self) -> bool;

    fn is_updated(&self) -> bool;

    fn get(&self) -> Self::Target<'_>;
}

pub trait ChangeAwareMut<'a>: ChangeAware<'a> {
    type TargetMut<'t> where 'a: 't, Self: 't;

    fn update(&mut self);

    fn bypass(&mut self) -> Self::TargetMut<'_>;

    fn get_mut(&mut self) -> Self::TargetMut<'_>;
}

pub struct RefErased<'a> {
    inner: Ptr<'a>,
    current_marks: ChangeMarks,
    last_mark: ChangeMark,
}

impl<'a> RefErased<'a> {
    #[inline]
    pub unsafe fn new(inner: Ptr<'a>, current: ChangeMarks, last: ChangeMark) -> Self {
        Self { inner, current_marks: current, last_mark: last, }
    }

    #[inline]
    pub unsafe fn casted<T: 'a>(self) -> Ref<'a, T> {
        Ref {
            inner: self,
            _marker: PhantomData,
        }
    }
}

impl<'a> ChangeAware<'a> for RefErased<'a> {
    type Target<'t> = Ptr<'t> where 'a: 't, Self: 't;

    #[inline]
    fn is_added(&self) -> bool {
        self.current_marks.added.newer_than(self.last_mark)
    }

    #[inline]
    fn is_updated(&self) -> bool {
        self.current_marks.updated.newer_than(self.last_mark)
    }

    #[inline]
    fn get(&self) -> Self::Target<'_> {
        self.inner
    }
}

pub struct Ref<'a, T> {
    inner: RefErased<'a>,
    _marker: PhantomData<&'a T>,
}

impl<'a, T> Ref<'a, T> {
    #[inline]
    pub fn erased(self) -> RefErased<'a> {
        self.inner
    }

    #[inline]
    pub fn into_inner(self) -> &'a T {
        unsafe { self.inner.inner.deref() }
    }
}

impl<'a, T> ChangeAware<'a> for Ref<'a, T> {
    type Target<'t> = &'t T where 'a: 't, Self: 't;

    #[inline]
    fn is_added(&self) -> bool {
        self.inner.is_added()
    }

    #[inline]
    fn is_updated(&self) -> bool {
        self.inner.is_updated()
    }

    #[inline]
    fn get(&self) -> Self::Target<'_> {
        unsafe { self.inner.get().deref() }
    }
}

impl<'a, T> AsRef<T> for Ref<'a, T> {
    #[inline]
    fn as_ref(&self) -> &T {
        self.get()
    }
}

impl<'a, T> Deref for Ref<'a, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.get()
    }
}

pub struct MutErased<'a> {
    inner: PtrMut<'a>,
    current_marks: NonNull<ChangeMarks>,
    last_mark: ChangeMark,
    current_mark: ChangeMark,
}

impl<'a> MutErased<'a> {
    #[inline]
    pub unsafe fn new(inner: PtrMut<'a>, current: NonNull<ChangeMarks>, last: ChangeMark, caller: ChangeMark) -> Self {
        Self { inner, current_marks: current, last_mark: last, current_mark: caller, }
    }

    #[inline]
    pub unsafe fn casted<T: 'a>(self) -> Mut<'a, T> {
        Mut {
            inner: self,
            _marker: PhantomData,
        }
    }
}

impl<'a> ChangeAware<'a> for MutErased<'a> {
    type Target<'t> = Ptr<'t> where 'a: 't, Self: 't;

    #[inline]
    fn is_added(&self) -> bool {
        unsafe { self.current_marks.as_ref() }.added.newer_than(self.last_mark)
    }

    #[inline]
    fn is_updated(&self) -> bool {
        unsafe { self.current_marks.as_ref() }.updated.newer_than(self.last_mark)
    }

    #[inline]
    fn get(&self) -> Self::Target<'_> {
        self.inner.borrow()
    }
}

impl<'a> ChangeAwareMut<'a> for MutErased<'a> {
    type TargetMut<'t> = PtrMut<'t> where 'a: 't, Self: 't;

    #[inline]
    fn update(&mut self) {
        unsafe { self.current_marks.as_mut() }.updated = self.current_mark;
    }

    #[inline]
    fn bypass(&mut self) -> Self::TargetMut<'_> {
        self.inner.borrow_mut()
    }

    #[inline]
    fn get_mut(&mut self) -> Self::TargetMut<'_> {
        self.update();
        self.bypass()
    }
}

pub struct Mut<'a, T> {
    inner: MutErased<'a>,
    _marker: PhantomData<&'a T>,
}

impl<'a, T> Mut<'a, T> {
    #[inline]
    pub fn erased(self) -> MutErased<'a> {
        self.inner
    }

    #[inline]
    pub fn into_inner(mut self) -> &'a mut T {
        self.update();
        unsafe { self.inner.inner.deref_mut() }
    }
}

impl<'a, T> ChangeAware<'a> for Mut<'a, T> {
    type Target<'t> = &'t T where 'a: 't, Self: 't;

    #[inline]
    fn is_added(&self) -> bool {
        self.inner.is_added()
    }

    #[inline]
    fn is_updated(&self) -> bool {
        self.inner.is_updated()
    }

    #[inline]
    fn get(&self) -> Self::Target<'_> {
        unsafe { self.inner.get().deref() }
    }
}

impl<'a, T> ChangeAwareMut<'a> for Mut<'a, T> {
    type TargetMut<'t> = &'t mut T where 'a: 't, Self: 't;

    #[inline]
    fn update(&mut self) {
        self.inner.update();
    }

    #[inline]
    fn bypass(&mut self) -> Self::TargetMut<'_> {
        unsafe { self.inner.bypass().deref_mut() }
    }

    #[inline]
    fn get_mut(&mut self) -> Self::TargetMut<'_> {
        self.update();
        self.bypass()
    }
}

impl<'a, T> AsRef<T> for Mut<'a, T> {
    #[inline]
    fn as_ref(&self) -> &T {
        self.get()
    }
}

impl<'a, T> AsMut<T> for Mut<'a, T> {
    #[inline]
    fn as_mut(&mut self) -> &mut T {
        self.get_mut()
    }
}

impl<'a, T> Deref for Mut<'a, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.get()
    }
}

impl<'a, T> DerefMut for Mut<'a, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.get_mut()
    }
}
