//! Small math types owned by the engine.

/// A two-dimensional vector.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    /// Horizontal component.
    pub x: f32,
    /// Vertical component.
    pub y: f32,
}

impl Vec2 {
    /// Creates a vector.
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self { Self { x, y } }

    /// Returns the squared magnitude.
    #[must_use]
    pub fn length_squared(self) -> f32 { self.x * self.x + self.y * self.y }

    /// Adds another vector.
    #[must_use]
    pub const fn add(self, rhs: Self) -> Self { Self::new(self.x + rhs.x, self.y + rhs.y) }

    /// Scales the vector.
    #[must_use]
    pub const fn scale(self, scalar: f32) -> Self { Self::new(self.x * scalar, self.y * scalar) }
}

/// A three-dimensional vector.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    /// X component.
    pub x: f32,
    /// Y component.
    pub y: f32,
    /// Z component.
    pub z: f32,
}

impl Vec3 {
    /// Creates a vector.
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self { Self { x, y, z } }

    /// Adds another vector.
    #[must_use]
    pub const fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }

    /// Scales the vector.
    #[must_use]
    pub const fn scale(self, scalar: f32) -> Self {
        Self::new(self.x * scalar, self.y * scalar, self.z * scalar)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_operations_are_component_wise() {
        assert_eq!(Vec3::new(1.0, 2.0, 3.0).add(Vec3::new(4.0, 5.0, 6.0)), Vec3::new(5.0, 7.0, 9.0));
        assert_eq!(Vec2::new(3.0, 4.0).length_squared(), 25.0);
    }
}
