use fei_common::prelude::*;
use crate::{
    component::ArchetypeId,
    entity::Entity,
};
use std::{
    collections::VecDeque,
    mem,
    sync::atomic::{
        AtomicU32, Ordering,
    },
};

#[derive(Error, Debug)]
pub enum SpawnError {
    #[error("too many entities")]
    TooMany,
    #[error("entity reservations entities not flush()-ed yet")]
    NotFlushed,
}

#[derive(Error, Debug)]
#[error("too many entities")]
pub struct ReserveError;

#[derive(Default)]
pub struct Entities {
    /// Counter for reservations, mapped to new allocations if there are no longer freed entities.
    /// Shared across threads, as reservations may happen concurrently, but allocations may not.
    reservoir: AtomicU32,

    /// All in-use and freed contained entities. A scenario of reserving, flushing, freeing, and
    /// repeat is as follows:
    /// 1. [`reserve()`](Entities::reserve)-ing an entity returns _**`A`**_, an entity whose `id` is
    ///    [`all.len()`](Vec::len) and `generation` is `0`. This entity isn't valid yet, as it's
    ///    not contained in `all`.
    /// 2. A call to [`flush`](Entities::flush) pushes `all` with a copy of _**`A`**_; let's refer to
    ///    this copy as _**`A'`**_.
    /// 3. [`free()`](Entities::free)-ing _**`A`**_ increments the `generation` of _**`A'`**_ by `2`,
    ///    effectively invalidating _**`A`**_. A reusable entity referred as _**`A''`**_ is pushed to
    ///    `free` with the `id` of _**`A`**_`.id` and `generation` of _**`A`**_`.generation` + `1`.
    /// 4. [`reserve()`](Entities::reserve)-ing an entity now returns _**`A''`**_, which still isn't
    ///    valid due to _**`A'`**_ still having a greater `generation` by `1`.
    /// 5. A call to [`flush`](Entities::flush) decrements _**`A'`**_`.generation` by `1`, effectively
    ///    validating the reused entity while still leaving older copies invalid.
    all: Vec<EntityIndex>,
    /// All freed entities, synchronously updated.
    free: VecDeque<Entity>,
}

#[derive(Copy, Clone, Debug)]
pub struct EntityLocation {
    pub(crate) archetype_id: ArchetypeId,
    pub(crate) table_index: Option<usize>,
}

impl Entities {
    pub const MAX: usize = isize::MAX as usize / mem::align_of::<Entity>();

    #[inline]
    pub fn contains(&self, entity: Entity) -> bool {
        self.all
            .get(entity.id as usize)
            .is_some_and(|&index| entity.generation == index.generation)
    }

    pub fn spawn(&mut self) -> Result<Entity, SpawnError> {
        if *self.reservoir.get_mut() != 0 {
            Err(SpawnError::NotFlushed)
        } else if Self::MAX <= self.all.len() {
            Err(SpawnError::TooMany)
        } else {
            Ok(if let Some(entity) = self.free.pop_front() {
                let index = &mut self.all[entity.id as usize];
                index.generation -= 1;
                index.location = None;

                entity
            } else {
                self.all.push(EntityIndex {
                    generation: 0,
                    location: None,
                });

                Entity {
                    id: self.all.len() as u32 - 1,
                    generation: 0,
                }
            })
        }
    }

    /// Reserves an entity that is validated on the next [`flush`](Entities::flush).
    pub fn reserve(&self) -> Result<Entity, ReserveError> {
        let reserved = self.reservoir.fetch_add(1, Ordering::Relaxed) as usize;
        let all_len = self.all.len();
        let free_len = self.free.len();

        if reserved < Self::MAX - all_len + free_len {
            Ok(if reserved < free_len {
                // Reuse freed entities if possible.
                self.free[reserved]
            } else {
                // Otherwise, prompt a new allocation in flush().
                Entity {
                    id: (all_len + reserved - free_len) as u32,
                    generation: 0,
                }
            })
        } else {
            self.reservoir.fetch_sub(1, Ordering::Relaxed);
            Err(ReserveError)
        }
    }

    /// Reserves many entities that are validated on the next [`flush`](Entities::flush).
    pub fn reserve_many(&self, count: usize) -> Result<ReserveEntities, ReserveError> {
        let count = u32::try_from(count).map_err(|_| ReserveError)?;
        let start = self.reservoir.fetch_add(count, Ordering::Relaxed);
        let all_len = self.all.len();
        let free_len = self.free.len();

        if start + count - 1 < (Self::MAX - all_len + free_len) as u32 {
            Ok(ReserveEntities {
                start,
                end: start + count,
                all_len,
                free: &self.free,
            })
        } else {
            self.reservoir.fetch_sub(count, Ordering::Relaxed);
            Err(ReserveError)
        }
    }

    /// Frees an entity, allowing it to be reused by subsequent [`reserve`](Entities::reserve).
    pub fn free(&mut self, entity: Entity) {
        if let Some(index) = self.all.get_mut(entity.id as usize) {
            if index.generation == entity.generation {
                index.generation += 2;
                self.free.push_back(Entity {
                    id: entity.id,
                    generation: entity.generation + 1,
                });
            }
        }
    }

    /// Frees many entities, allowing it to be reused by subsequent [`reserve`](Entities::reserve).
    #[inline]
    pub fn free_many(&mut self, entities: impl IntoIterator<Item = Entity>) {
        for entity in entities {
            self.free(entity);
        }
    }

    /// Re-uses freed entities and allocates new ones if necessary, resetting the reservation count.
    pub fn flush(&mut self) {
        let reserved = mem::replace(self.reservoir.get_mut(), 0) as usize;
        if reserved == 0 { return };

        let free_len = self.free.len();
        for freed in self.free.drain(0..reserved.min(free_len)) {
            let reused = &mut self.all[freed.id as usize];
            reused.generation -= 1;
            reused.location = None;
        }

        if reserved > free_len {
            let add = reserved - free_len;
            self.all.reserve(add);

            unsafe {
                let base = self.all.as_mut_ptr().add(self.all.len());
                for i in 0..add {
                    base.add(i).write(EntityIndex {
                        generation: 0,
                        location: None,
                    });
                }

                self.all.set_len(self.all.len() + add);
            }
        }
    }

    #[inline]
    pub unsafe fn location(&self, entity: Entity) -> Option<EntityLocation> {
        self.all.get_unchecked(entity.id as usize).location
    }

    #[inline]
    pub unsafe fn location_mut(&mut self, entity: Entity) -> &mut Option<EntityLocation> {
        &mut self.all.get_unchecked_mut(entity.id as usize).location
    }
}

#[derive(Copy, Clone)]
struct EntityIndex {
    generation: u32,
    location: Option<EntityLocation>,
}

#[derive(Copy, Clone)]
pub struct ReserveEntities<'a> {
    start: u32,
    end: u32,
    all_len: usize,
    free: &'a VecDeque<Entity>,
}

impl<'a> Iterator for ReserveEntities<'a> {
    type Item = Entity;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let reserved = self.start;
        if reserved < self.end {
            self.start += 1;
            let free_len = self.free.len();

            Some(if (reserved as usize) < free_len {
                // Reuse freed entities if possible.
                self.free[reserved as usize]
            } else {
                // Otherwise, prompt a new allocation in flush().
                Entity {
                    id: (self.all_len + reserved as usize - free_len) as u32,
                    generation: 0,
                }
            })
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle() -> anyhow::Result<()> {
        let mut entities = Entities::default();
        let a = entities.reserve()?;
        let b = entities.reserve()?;

        // Newly allocated entities.
        assert_eq!(a.id, 0);
        assert_eq!(b.id, 1);
        assert_eq!(a.generation, 0);
        assert_eq!(b.generation, 0);
        // Not flush()-ed, so they don't exist yet.
        assert!(!entities.contains(a));
        assert!(!entities.contains(b));

        entities.flush();

        // flush()-ed, so they exist now.
        assert!(entities.contains(a));
        assert!(entities.contains(b));

        entities.free_many([a, b]);
        // free()-ed, so they don't exist anymore.
        assert!(!entities.contains(a));
        assert!(!entities.contains(b));

        let re_a = entities.reserve()?;
        let re_b = entities.reserve()?;

        // Reused entities.
        assert_eq!(a.id, re_a.id);
        assert_eq!(b.id, re_b.id);
        assert_ne!(a.generation, re_a.generation);
        assert_ne!(b.generation, re_b.generation);
        assert_eq!(re_a.generation(), 1);
        assert_eq!(re_b.generation(), 1);
        // Not flush()-ed, so they don't exist yet.
        assert!(!entities.contains(re_a));
        assert!(!entities.contains(re_b));

        entities.flush();

        // flush()-ed, so they exist now.
        assert!(entities.contains(re_a));
        assert!(entities.contains(re_b));
        // Even though they have the same ID, they're an older entity instance, hence non-existent.
        assert!(!entities.contains(a));
        assert!(!entities.contains(b));

        Ok(())
    }

    #[test]
    fn reserve_many() -> anyhow::Result<()> {
        let mut entities = Entities::default();
        // Generate [0, 100] entities.
        for (e, i) in entities.reserve_many(100)?.zip(0..100) {
            // Not flush()-ed, so they don't exist yet.
            assert_eq!(e.id, i);
            assert_eq!(e.generation, 0);
            assert!(!entities.contains(e));
        }

        entities.flush();
        for id in 0..50 {
            // flush()-ed, so they exist now.
            assert!(entities.contains(Entity {
                id,
                generation: 0,
            }));
        }

        entities.free_many((0..50).map(|id| Entity {
            id,
            generation: 0,
        }));

        for id in 0..50 {
            // [0, 50] don't exist anymore.
            assert!(!entities.contains(Entity {
                id,
                generation: 0,
            }));
        }

        for (e, i) in entities.reserve_many(100)?.zip(0..100) {
            // [0, 50] are reused, [50, 100] are allocated as [100, 150].
            assert_eq!(e.id, if i < 50 { i } else { i + 50 });
            assert_eq!(e.generation, if i < 50 { 1 } else { 0 });
            assert!(!entities.contains(e));
        }

        entities.flush();
        for i in 0..100 {
            assert!(entities.contains(Entity {
                id: if i < 50 { i } else { i + 50 },
                generation: if i < 50 { 1 } else { 0 },
            }));
        }

        Ok(())
    }
}
