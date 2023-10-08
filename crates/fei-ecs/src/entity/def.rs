#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct Entity {
    /// Collection identifier that this entity resides in.
    pub(crate) id: usize,
    /// Per-copy generation state. States older than the one stored in the collection means the held
    /// entity is already freed in the collection.
    pub(crate) generation: usize,
}

impl Entity {
    /// Returns the collection identifier that this entity resides in.
    #[inline]
    pub fn id(self) -> usize {
        self.id
    }

    /// Returns the per-copy generation state. States older than the one stored in the collection
    /// means the held entity is already freed in the collection.
    #[inline]
    pub fn generation(self) -> usize {
        self.generation
    }
}

#[derive(Copy, Clone, Debug)]
pub struct EntityIndex {
    pub(crate) generation: usize,
}
