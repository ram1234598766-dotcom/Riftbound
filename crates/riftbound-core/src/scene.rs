//! Runtime scene storage for entities and their world transforms.

use crate::{EntityArena, EntityId, Transform3};

/// One live scene object and its transform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneObject {
    /// Generational identity of this object.
    pub id: EntityId,
    /// World-space transform.
    pub transform: Transform3,
}

/// Owns scene object lifetimes and their transforms.
#[derive(Clone, Debug, Default)]
pub struct Scene {
    entities: EntityArena,
    objects: Vec<SceneObject>,
}

impl Scene {
    /// Creates an empty scene.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entities: EntityArena::new(),
            objects: Vec::new(),
        }
    }

    /// Adds an object with the supplied transform.
    pub fn spawn(&mut self, transform: Transform3) -> Option<EntityId> {
        let id = self.entities.spawn()?;
        self.objects.push(SceneObject { id, transform });
        Some(id)
    }

    /// Removes an object if its ID still refers to the same entity lifetime.
    pub fn despawn(&mut self, id: EntityId) -> bool {
        if !self.entities.despawn(id) {
            return false;
        }
        self.objects.retain(|object| object.id != id);
        true
    }

    /// Returns an object's transform when its ID is live.
    #[must_use]
    pub fn get(&self, id: EntityId) -> Option<&Transform3> {
        self.objects
            .iter()
            .find(|object| object.id == id)
            .map(|object| &object.transform)
    }

    /// Returns a mutable transform when its ID is live.
    pub fn get_mut(&mut self, id: EntityId) -> Option<&mut Transform3> {
        self.objects
            .iter_mut()
            .find(|object| object.id == id)
            .map(|object| &mut object.transform)
    }

    /// Iterates over live scene objects in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &SceneObject> {
        self.objects.iter()
    }

    /// Returns the number of live objects.
    #[must_use]
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// Returns whether the scene has no live objects.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vec3;

    #[test]
    fn scene_spawns_updates_and_despawns_objects() {
        let mut scene = Scene::new();
        let id = scene.spawn(Transform3::IDENTITY).expect("ID available");
        assert_eq!(scene.len(), 1);
        assert_eq!(scene.get(id), Some(&Transform3::IDENTITY));
        let moved = Transform3::new(
            Vec3::new(2.0, 0.0, -3.0),
            Vec3::new(0.0, 0.5, 0.0),
            Vec3::new(1.0, 2.0, 1.0),
        );
        *scene.get_mut(id).expect("object remains live") = moved;
        assert_eq!(scene.get(id), Some(&moved));
        assert!(scene.despawn(id));
        assert!(scene.is_empty());
        assert_eq!(scene.get(id), None);
        assert!(!scene.despawn(id));
    }

    #[test]
    fn reused_scene_slots_reject_stale_object_ids() {
        let mut scene = Scene::new();
        let stale = scene.spawn(Transform3::IDENTITY).expect("ID available");
        assert!(scene.despawn(stale));
        let current = scene.spawn(Transform3::IDENTITY).expect("slot reusable");
        assert_eq!(stale.index(), current.index());
        assert_ne!(stale.generation(), current.generation());
        assert_eq!(scene.get(stale), None);
        assert!(scene.get(current).is_some());
    }
}
