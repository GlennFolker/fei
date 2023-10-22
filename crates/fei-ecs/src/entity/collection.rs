use crate::entity::{
    Entity, EntityIndex,
};
use fei_common::prelude::*;
use std::{
    collections::VecDeque,
    iter,
    mem,
    sync::atomic::{
        AtomicUsize, Ordering,
    },
};

#[derive(Default)]
pub struct Entities {
    /// Counter for reservations, mapped to new allocations if there are no longer freed entities.
    /// Shared across threads, as reservations may happen concurrently, but allocations may not.
    reservoir: AtomicUsize,

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

impl Entities {
    pub const MAX: usize = isize::MAX as usize / mem::size_of::<Entity>();

    #[inline]
    pub fn contains(&self, entity: Entity) -> bool {
        self.all
            .get(entity.id)
            .is_some_and(|&index| entity.generation == index.generation)
    }

    /// Reserves an entity that is validated on the next [`flush`](Entities::flush).
    pub fn reserve(&self) -> anyhow::Result<Entity> {
        let reserved = self.reservoir.fetch_add(1, Ordering::Relaxed);
        let all_len = self.all.len();
        let free_len = self.free.len();

        if reserved < Self::MAX - all_len + free_len {
            Ok(if reserved < free_len {
                // Reuse freed entities if possible.
                self.free[reserved]
            } else {
                // Otherwise, prompt a new allocation in flush().
                Entity {
                    id: all_len + reserved - free_len,
                    generation: 0,
                }
            })
        } else {
            self.reservoir.fetch_sub(1, Ordering::Relaxed);
            anyhow::bail!("too many entities");
        }
    }

    /// Reserves many entities that are validated on the next [`flush`](Entities::flush).
    pub fn reserve_many(&self, count: usize) -> anyhow::Result<ReserveEntities> {
        let start = self.reservoir.fetch_add(count, Ordering::Relaxed);
        let all_len = self.all.len();
        let free_len = self.free.len();

        if start + count - 1 < Self::MAX - all_len + free_len {
            Ok(ReserveEntities {
                start,
                end: start + count,
                all_len,
                free: &self.free,
            })
        } else {
            self.reservoir.fetch_sub(count, Ordering::Relaxed);
            anyhow::bail!("too many entities");
        }
    }

    /// Frees an entity, allowing it to be reused by subsequent [`reserve`](Entities::reserve).
    pub fn free(&mut self, entity: Entity) {
        if let Some(index) = self.all.get_mut(entity.id) {
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
        let reserved = mem::replace(self.reservoir.get_mut(), 0);
        if reserved == 0 { return };

        let free_len = self.free.len();
        for freed in self.free.drain(0..reserved.min(free_len)) {
            self.all[freed.id].generation -= 1;
        }

        if reserved > free_len {
            self.all.extend(iter::repeat(EntityIndex {
                generation: 0,
            }).take(reserved - free_len));
        }
    }
}

#[derive(Copy, Clone)]
pub struct ReserveEntities<'a> {
    start: usize,
    end: usize,
    all_len: usize,
    free: &'a VecDeque<Entity>,
}

impl<'a> Iterator for ReserveEntities<'a> {
    type Item = Entity;

    fn next(&mut self) -> Option<Self::Item> {
        let reserved = self.start;
        if reserved < self.end {
            self.start += 1;
            let free_len = self.free.len();

            Some(if reserved < free_len {
                // Reuse freed entities if possible.
                self.free[reserved]
            } else {
                // Otherwise, prompt a new allocation in flush().
                Entity {
                    id: self.all_len + reserved - free_len,
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
        // Not flush()ed, so they don't exist yet.
        assert!(!entities.contains(a));
        assert!(!entities.contains(b));

        entities.flush();

        // flush()ed, so they exist now.
        assert!(entities.contains(a));
        assert!(entities.contains(b));

        entities.free_many([a, b]);
        // free()ed, so they don't exist anymore.
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
        // Not flush()ed, so they don't exist yet.
        assert!(!entities.contains(re_a));
        assert!(!entities.contains(re_b));

        entities.flush();

        // flush()ed, so they exist now.
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
            // Not flush()ed, so they don't exist yet.
            assert_eq!(e.id, i);
            assert_eq!(e.generation, 0);
            assert!(!entities.contains(e));
        }

        entities.flush();
        for id in 0..50 {
            // flush()ed, so they exist now.
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
