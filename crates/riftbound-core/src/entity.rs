//! Generational entity identifiers and lifetime management.

/// Stable identifier for one entity lifetime.
///
/// The generation changes whenever a slot is reused, so stale identifiers
/// cannot accidentally refer to a newly spawned entity.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EntityId {
    index: u32,
    generation: u32,
}

impl EntityId {
    /// Returns the entity's slot index.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }

    /// Returns the generation for this entity lifetime.
    #[must_use]
    pub const fn generation(self) -> u32 {
        self.generation
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Slot {
    generation: u32,
    alive: bool,
    retired: bool,
}

/// Owns entity lifetimes and safely reuses vacant slots.
#[derive(Clone, Debug, Default)]
pub struct EntityArena {
    slots: Vec<Slot>,
    free: Vec<u32>,
    live_count: usize,
}

impl EntityArena {
    /// Creates an empty entity arena.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            live_count: 0,
        }
    }

    /// Spawns an entity, returning None only if the identifier space is exhausted.
    pub fn spawn(&mut self) -> Option<EntityId> {
        if let Some(index) = self.free.pop() {
            let slot_index = usize::try_from(index).ok()?;
            let slot = self.slots.get_mut(slot_index)?;
            if slot.alive || slot.retired {
                return None;
            }
            slot.alive = true;
            self.live_count += 1;
            return Some(EntityId {
                index,
                generation: slot.generation,
            });
        }

        let index = u32::try_from(self.slots.len()).ok()?;
        self.slots.push(Slot {
            generation: 0,
            alive: true,
            retired: false,
        });
        self.live_count += 1;
        Some(EntityId {
            index,
            generation: 0,
        })
    }

    /// Despawns an entity if the identifier refers to its current lifetime.
    ///
    /// Returns true only when a live entity was removed.
    pub fn despawn(&mut self, entity: EntityId) -> bool {
        let Ok(slot_index) = usize::try_from(entity.index) else {
            return false;
        };
        let Some(slot) = self.slots.get_mut(slot_index) else {
            return false;
        };
        if !slot.alive || slot.generation != entity.generation {
            return false;
        }

        slot.alive = false;
        self.live_count -= 1;
        if let Some(next_generation) = slot.generation.checked_add(1) {
            slot.generation = next_generation;
            self.free.push(entity.index);
        } else {
            slot.retired = true;
        }
        true
    }

    /// Checks whether an identifier refers to a currently live entity.
    #[must_use]
    pub fn is_alive(&self, entity: EntityId) -> bool {
        let Ok(slot_index) = usize::try_from(entity.index) else {
            return false;
        };
        self.slots
            .get(slot_index)
            .is_some_and(|slot| slot.alive && slot.generation == entity.generation)
    }

    /// Returns the number of currently live entities.
    #[must_use]
    pub const fn live_count(&self) -> usize {
        self.live_count
    }

    /// Returns the number of allocated slots, including vacant and retired slots.
    #[must_use]
    pub fn slot_count(&self) -> usize {
        self.slots.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reuses_slots_without_reviving_stale_identifiers() {
        let mut arena = EntityArena::new();
        let first = arena.spawn().expect("first entity should fit");
        assert!(arena.is_alive(first));
        assert!(arena.despawn(first));
        assert!(!arena.is_alive(first));

        let second = arena.spawn().expect("vacated slot should be reused");
        assert_eq!(first.index(), second.index());
        assert_ne!(first.generation(), second.generation());
        assert!(!arena.is_alive(first));
        assert!(arena.is_alive(second));
        assert_eq!(arena.live_count(), 1);
        assert_eq!(arena.slot_count(), 1);
    }

    #[test]
    fn despawning_stale_or_unknown_identifiers_is_a_no_op() {
        let mut arena = EntityArena::new();
        let entity = arena.spawn().expect("entity should fit");
        assert!(!arena.despawn(EntityId {
            index: entity.index(),
            generation: entity.generation().wrapping_add(1),
        }));
        assert!(!arena.despawn(EntityId {
            index: u32::MAX,
            generation: 0,
        }));
        assert_eq!(arena.live_count(), 1);
    }
}
