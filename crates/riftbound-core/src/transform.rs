//! World-space transform primitives.

use crate::Vec3;

/// Position, Euler rotation (radians), and scale for a 3D object.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform3 {
    /// World-space position.
    pub translation: Vec3,
    /// Rotation around the X, Y, and Z axes, in radians.
    pub rotation_radians: Vec3,
    /// Per-axis scale.
    pub scale: Vec3,
}

impl Transform3 {
    /// Identity transform: zero translation and rotation, unit scale.
    pub const IDENTITY: Self = Self {
        translation: Vec3::new(0.0, 0.0, 0.0),
        rotation_radians: Vec3::new(0.0, 0.0, 0.0),
        scale: Vec3::new(1.0, 1.0, 1.0),
    };

    /// Creates a transform from its position, rotation, and scale.
    #[must_use]
    pub const fn new(translation: Vec3, rotation_radians: Vec3, scale: Vec3) -> Self {
        Self {
            translation,
            rotation_radians,
            scale,
        }
    }
}

impl Default for Transform3 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_transform_is_identity() {
        assert_eq!(Transform3::default(), Transform3::IDENTITY);
    }

    #[test]
    fn stores_position_rotation_and_scale_independently() {
        let transform = Transform3::new(
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::new(0.1, 0.2, 0.3),
            Vec3::new(2.0, 2.0, 2.0),
        );
        assert_eq!(transform.translation, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(transform.rotation_radians, Vec3::new(0.1, 0.2, 0.3));
        assert_eq!(transform.scale, Vec3::new(2.0, 2.0, 2.0));
    }
}
