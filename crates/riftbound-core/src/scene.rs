//! Runtime scene storage for entities and their world transforms.

use crate::{EntityArena, EntityId, Transform3};

/// Stable index identifying a renderer mesh asset.
///
/// Mesh IDs are assigned by the renderer's asset registry. The core crate stores
/// the identifier without depending on GPU resources.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct MeshId(u32);

impl MeshId {
    /// Built-in cube mesh used by ordinary scene spawns.
    pub const CUBE: Self = Self(0);
    /// Built-in pyramid mesh used by the starter scene.
    pub const PYRAMID: Self = Self(1);

    /// Creates an ID for an entry in the renderer's mesh registry.
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// Returns the registry index represented by this ID.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.0
    }
}

/// One live scene object, its mesh, local transform, and optional parent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneObject {
    /// Generational identity of this object.
    pub id: EntityId,
    /// Transform relative to its parent (or world space when unparented).
    pub transform: Transform3,
    /// Renderer mesh asset used to draw this object.
    pub mesh: MeshId,
    /// Optional parent object.
    pub parent: Option<EntityId>,
}

/// Owns scene object lifetimes, local transforms, and parent relationships.
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

    /// Adds an unparented object with the supplied local transform.
    pub fn spawn(&mut self, transform: Transform3) -> Option<EntityId> {
        self.spawn_with_mesh(transform, MeshId::CUBE)
    }

    /// Adds an unparented object using an explicitly selected mesh asset.
    pub fn spawn_with_mesh(&mut self, transform: Transform3, mesh: MeshId) -> Option<EntityId> {
        let id = self.entities.spawn()?;
        self.objects.push(SceneObject {
            id,
            transform,
            mesh,
            parent: None,
        });
        Some(id)
    }

    /// Changes the mesh asset assigned to a live object.
    pub fn set_mesh(&mut self, id: EntityId, mesh: MeshId) -> bool {
        let Some(object) = self.objects.iter_mut().find(|object| object.id == id) else {
            return false;
        };
        object.mesh = mesh;
        true
    }

    /// Removes an object and detaches its direct children.
    pub fn despawn(&mut self, id: EntityId) -> bool {
        if !self.entities.despawn(id) {
            return false;
        }
        self.objects.retain(|object| object.id != id);
        for object in &mut self.objects {
            if object.parent == Some(id) {
                object.parent = None;
            }
        }
        true
    }

    /// Returns an object's local transform when its ID is live.
    #[must_use]
    pub fn get(&self, id: EntityId) -> Option<&Transform3> {
        self.objects
            .iter()
            .find(|object| object.id == id)
            .map(|object| &object.transform)
    }

    /// Returns a mutable local transform when its ID is live.
    pub fn get_mut(&mut self, id: EntityId) -> Option<&mut Transform3> {
        self.objects
            .iter_mut()
            .find(|object| object.id == id)
            .map(|object| &mut object.transform)
    }

    /// Assigns a parent, rejecting stale IDs and relationships that create cycles.
    ///
    /// Passing `None` detaches the child from its current parent.
    pub fn set_parent(&mut self, child: EntityId, parent: Option<EntityId>) -> bool {
        let Some(child_index) = self.objects.iter().position(|object| object.id == child) else {
            return false;
        };
        if let Some(parent_id) = parent {
            if !self.objects.iter().any(|object| object.id == parent_id) || parent_id == child {
                return false;
            }
            let mut ancestor = Some(parent_id);
            while let Some(id) = ancestor {
                if id == child {
                    return false;
                }
                ancestor = self
                    .objects
                    .iter()
                    .find(|object| object.id == id)
                    .and_then(|object| object.parent);
            }
        }
        self.objects[child_index].parent = parent;
        true
    }

    /// Computes the object's world matrix by composing its ancestor transforms.
    #[must_use]
    pub fn world_matrix(&self, id: EntityId) -> Option<[f32; 16]> {
        let mut chain = Vec::new();
        let mut current = Some(id);
        while let Some(current_id) = current {
            let object = self.objects.iter().find(|object| object.id == current_id)?;
            chain.push(local_matrix(object.transform));
            current = object.parent;
        }

        let mut world = identity_matrix();
        for local in chain.iter().rev() {
            world = multiply_matrices(world, *local);
        }
        Some(world)
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

fn identity_matrix() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn local_matrix(transform: Transform3) -> [f32; 16] {
    let rotation = transform.rotation_radians;
    let (sx, cx) = rotation.x.sin_cos();
    let (sy, cy) = rotation.y.sin_cos();
    let (sz, cz) = rotation.z.sin_cos();
    let scale = transform.scale;
    let r00 = cz * cy;
    let r01 = cz * sy * sx - sz * cx;
    let r02 = cz * sy * cx + sz * sx;
    let r10 = sz * cy;
    let r11 = sz * sy * sx + cz * cx;
    let r12 = sz * sy * cx - cz * sx;
    let r20 = -sy;
    let r21 = cy * sx;
    let r22 = cy * cx;
    [
        r00 * scale.x,
        r10 * scale.x,
        r20 * scale.x,
        0.0,
        r01 * scale.y,
        r11 * scale.y,
        r21 * scale.y,
        0.0,
        r02 * scale.z,
        r12 * scale.z,
        r22 * scale.z,
        0.0,
        transform.translation.x,
        transform.translation.y,
        transform.translation.z,
        1.0,
    ]
}

fn multiply_matrices(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    let mut result = [0.0; 16];
    for column in 0..4 {
        for row in 0..4 {
            for inner in 0..4 {
                result[column * 4 + row] += a[inner * 4 + row] * b[column * 4 + inner];
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vec3;

    fn translation(x: f32, y: f32, z: f32) -> Transform3 {
        Transform3::new(
            Vec3::new(x, y, z),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(1.0, 1.0, 1.0),
        )
    }

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
    fn objects_default_to_cube_and_can_select_another_mesh() {
        let mut scene = Scene::new();
        let cube = scene.spawn(Transform3::IDENTITY).expect("cube ID");
        let pyramid = scene
            .spawn_with_mesh(Transform3::IDENTITY, MeshId::PYRAMID)
            .expect("pyramid ID");
        assert_eq!(scene.iter().find(|object| object.id == cube).expect("cube remains live").mesh, MeshId::CUBE);
        assert_eq!(scene.iter().find(|object| object.id == pyramid).expect("pyramid remains live").mesh, MeshId::PYRAMID);
        assert!(scene.set_mesh(cube, MeshId::PYRAMID));
        assert_eq!(scene.iter().find(|object| object.id == cube).unwrap().mesh, MeshId::PYRAMID);
        assert!(scene.despawn(cube));
        assert!(!scene.set_mesh(cube, MeshId::CUBE));
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

    #[test]
    fn child_world_matrix_inherits_parent_translation() {
        let mut scene = Scene::new();
        let parent = scene.spawn(translation(3.0, 0.0, -2.0)).expect("parent ID");
        let child = scene.spawn(translation(1.0, 2.0, 0.0)).expect("child ID");
        assert!(scene.set_parent(child, Some(parent)));
        let world = scene.world_matrix(child).expect("child is live");
        assert_eq!([world[12], world[13], world[14]], [4.0, 2.0, -2.0]);
    }

    #[test]
    fn parent_assignment_rejects_cycles_and_stale_handles() {
        let mut scene = Scene::new();
        let first = scene.spawn(Transform3::IDENTITY).expect("first ID");
        let second = scene.spawn(Transform3::IDENTITY).expect("second ID");
        assert!(scene.set_parent(second, Some(first)));
        assert!(!scene.set_parent(first, Some(second)));
        assert!(!scene.set_parent(first, Some(first)));
        assert!(scene.despawn(second));
        assert!(!scene.set_parent(first, Some(second)));
        assert_eq!(
            scene
                .iter()
                .find(|object| object.id == first)
                .expect("first live")
                .parent,
            None
        );
    }

    #[test]
    fn despawning_parent_detaches_children_without_deleting_them() {
        let mut scene = Scene::new();
        let parent = scene.spawn(translation(5.0, 0.0, 0.0)).expect("parent ID");
        let child = scene.spawn(translation(1.0, 0.0, 0.0)).expect("child ID");
        assert!(scene.set_parent(child, Some(parent)));
        assert!(scene.despawn(parent));
        let world = scene.world_matrix(child).expect("child remains live");
        assert_eq!(world[12], 1.0);
    }
}
