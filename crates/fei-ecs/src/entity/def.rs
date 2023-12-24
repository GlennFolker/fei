#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct Entity {
    /// Collection identifier that this entity resides in.
    pub(super) id: u32,
    /// Per-copy generation state. States older than the one stored in the collection means the held
    /// entity is already freed in the collection.
    pub(super) generation: u32,
}

impl Entity {
    /// Returns the collection identifier that this entity resides in.
    #[inline]
    pub fn id(self) -> u32 {
        self.id
    }

    /// Returns the per-copy generation state. States older than the one stored in the collection
    /// means the held entity is already freed in the collection.
    #[inline]
    pub fn generation(self) -> u32 {
        self.generation
    }
}
