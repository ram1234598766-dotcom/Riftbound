//! Simple deterministic 2D collision on the horizontal X/Z world plane.

use crate::Vec3;

/// Axis-aligned obstacle footprint in world-space X/Z coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb2 {
    /// Minimum X coordinate.
    pub min_x: f32,
    /// Maximum X coordinate.
    pub max_x: f32,
    /// Minimum Z coordinate.
    pub min_z: f32,
    /// Maximum Z coordinate.
    pub max_z: f32,
}

impl Aabb2 {
    /// Creates an axis-aligned footprint.
    #[must_use]
    pub const fn new(min_x: f32, max_x: f32, min_z: f32, max_z: f32) -> Self {
        Self {
            min_x,
            max_x,
            min_z,
            max_z,
        }
    }

    fn contains_expanded(self, x: f32, z: f32, radius: f32) -> bool {
        x > self.min_x - radius
            && x < self.max_x + radius
            && z > self.min_z - radius
            && z < self.max_z + radius
    }
}

/// Collision result after moving a circular character footprint.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CollisionResult {
    /// Resolved world-space position.
    pub position: Vec3,
    /// Whether horizontal movement was blocked on X.
    pub blocked_x: bool,
    /// Whether horizontal movement was blocked on Z.
    pub blocked_z: bool,
}


/// Collision result after moving an axis-aligned vehicle footprint.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VehicleCollisionResult {
    /// Resolved world-space position.
    pub position: Vec3,
    /// Whether movement was blocked on X.
    pub blocked_x: bool,
    /// Whether movement was blocked on Z.
    pub blocked_z: bool,
}

/// Static collision world with rectangular bounds and obstacle footprints.
#[derive(Clone, Debug, PartialEq)]
pub struct CollisionWorld {
    bounds: Aabb2,
    obstacles: Vec<Aabb2>,
}

impl CollisionWorld {
    /// Creates a world whose bounds define the walkable outer rectangle.
    #[must_use]
    pub fn new(bounds: Aabb2) -> Self {
        Self {
            bounds,
            obstacles: Vec::new(),
        }
    }

    /// Adds a solid obstacle footprint.
    pub fn add_obstacle(&mut self, obstacle: Aabb2) {
        self.obstacles.push(obstacle);
    }

    /// Returns the configured obstacle footprints.
    #[must_use]
    pub fn obstacles(&self) -> &[Aabb2] {
        &self.obstacles
    }

    /// Moves a character, resolving X and Z independently to allow wall sliding.
    ///
    /// Small substeps prevent a fast frame from tunneling through thin obstacles.
    #[must_use]
    pub fn move_character(&self, start: Vec3, displacement: Vec3, radius: f32) -> CollisionResult {
        if !radius.is_finite()
            || radius < 0.0
            || !start.x.is_finite()
            || !start.z.is_finite()
            || !displacement.x.is_finite()
            || !displacement.z.is_finite()
        {
            return CollisionResult {
                position: start,
                blocked_x: false,
                blocked_z: false,
            };
        }

        let mut remaining_x = displacement.x.clamp(-100.0, 100.0);
        let mut remaining_z = displacement.z.clamp(-100.0, 100.0);
        let mut result = CollisionResult {
            position: start,
            blocked_x: false,
            blocked_z: false,
        };

        while remaining_x.abs() > f32::EPSILON || remaining_z.abs() > f32::EPSILON {
            let step_x = remaining_x.clamp(-0.1, 0.1);
            let next_x = result.position.x + step_x;
            if self.is_walkable(next_x, result.position.z, radius) {
                result.position.x = next_x;
                remaining_x -= step_x;
            } else {
                let mut low = 0.0;
                let mut high = 1.0;
                for _ in 0..8 {
                    let middle = f32::midpoint(low, high);
                    let candidate = result.position.x + step_x * middle;
                    if self.is_walkable(candidate, result.position.z, radius) {
                        low = middle;
                    } else {
                        high = middle;
                    }
                }
                result.position.x += step_x * low;
                result.blocked_x = true;
                remaining_x = 0.0;
            }

            let step_z = remaining_z.clamp(-0.1, 0.1);
            let next_z = result.position.z + step_z;
            if self.is_walkable(result.position.x, next_z, radius) {
                result.position.z = next_z;
                remaining_z -= step_z;
            } else {
                let mut low = 0.0;
                let mut high = 1.0;
                for _ in 0..8 {
                    let middle = f32::midpoint(low, high);
                    let candidate = result.position.z + step_z * middle;
                    if self.is_walkable(result.position.x, candidate, radius) {
                        low = middle;
                    } else {
                        high = middle;
                    }
                }
                result.position.z += step_z * low;
                result.blocked_z = true;
                remaining_z = 0.0;
            }
        }
        result
    }


    /// Moves a vehicle using an axis-aligned rectangular footprint.
    /// Small substeps and binary refinement prevent tunnelling through obstacles.
    #[must_use]
    pub fn move_vehicle(
        &self,
        start: Vec3,
        displacement: Vec3,
        half_width: f32,
        half_length: f32,
    ) -> VehicleCollisionResult {
        if !half_width.is_finite()
            || !half_length.is_finite()
            || half_width < 0.0
            || half_length < 0.0
            || !start.x.is_finite()
            || !start.z.is_finite()
            || !displacement.x.is_finite()
            || !displacement.z.is_finite()
        {
            return VehicleCollisionResult {
                position: start,
                blocked_x: false,
                blocked_z: false,
            };
        }

        let mut result = VehicleCollisionResult {
            position: start,
            blocked_x: false,
            blocked_z: false,
        };
        let mut remaining_x = displacement.x.clamp(-100.0, 100.0);
        let mut remaining_z = displacement.z.clamp(-100.0, 100.0);
        while remaining_x.abs() > f32::EPSILON || remaining_z.abs() > f32::EPSILON {
            let step_x = remaining_x.clamp(-0.1, 0.1);
            let next_x = result.position.x + step_x;
            if self.is_vehicle_walkable(next_x, result.position.z, half_width, half_length) {
                result.position.x = next_x;
                remaining_x -= step_x;
            } else {
                let mut low = 0.0;
                let mut high = 1.0;
                for _ in 0..8 {
                    let middle = f32::midpoint(low, high);
                    if self.is_vehicle_walkable(
                        result.position.x + step_x * middle,
                        result.position.z,
                        half_width,
                        half_length,
                    ) {
                        low = middle;
                    } else {
                        high = middle;
                    }
                }
                result.position.x += step_x * low;
                result.blocked_x = true;
                remaining_x = 0.0;
            }

            let step_z = remaining_z.clamp(-0.1, 0.1);
            let next_z = result.position.z + step_z;
            if self.is_vehicle_walkable(result.position.x, next_z, half_width, half_length) {
                result.position.z = next_z;
                remaining_z -= step_z;
            } else {
                let mut low = 0.0;
                let mut high = 1.0;
                for _ in 0..8 {
                    let middle = f32::midpoint(low, high);
                    if self.is_vehicle_walkable(
                        result.position.x,
                        result.position.z + step_z * middle,
                        half_width,
                        half_length,
                    ) {
                        low = middle;
                    } else {
                        high = middle;
                    }
                }
                result.position.z += step_z * low;
                result.blocked_z = true;
                remaining_z = 0.0;
            }
        }
        result
    }

    fn is_vehicle_walkable(
        &self,
        x: f32,
        z: f32,
        half_width: f32,
        half_length: f32,
    ) -> bool {
        if x < self.bounds.min_x + half_width
            || x > self.bounds.max_x - half_width
            || z < self.bounds.min_z + half_length
            || z > self.bounds.max_z - half_length
        {
            return false;
        }
        !self.obstacles.iter().any(|obstacle| {
            x > obstacle.min_x - half_width
                && x < obstacle.max_x + half_width
                && z > obstacle.min_z - half_length
                && z < obstacle.max_z + half_length
        })
    }

    fn is_walkable(&self, x: f32, z: f32, radius: f32) -> bool {
        if x < self.bounds.min_x + radius
            || x > self.bounds.max_x - radius
            || z < self.bounds.min_z + radius
            || z > self.bounds.max_z - radius
        {
            return false;
        }
        !self
            .obstacles
            .iter()
            .any(|obstacle| obstacle.contains_expanded(x, z, radius))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> CollisionWorld {
        let mut world = CollisionWorld::new(Aabb2::new(-5.0, 5.0, -5.0, 5.0));
        world.add_obstacle(Aabb2::new(-1.0, 1.0, -1.0, 1.0));
        world
    }

    #[test]
    fn obstacle_blocks_character_and_reports_axis() {
        let result =
            world().move_character(Vec3::new(-2.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 0.0), 0.25);
        assert!(result.blocked_x);
        assert!(!result.blocked_z);
        assert!((result.position.x + 1.25).abs() < 0.001);
    }

    #[test]
    fn movement_slides_along_obstacles() {
        let result =
            world().move_character(Vec3::new(-2.0, 0.0, 0.0), Vec3::new(2.0, 0.0, 2.0), 0.25);
        assert!(result.blocked_x);
        assert!(!result.blocked_z);
        assert!(result.position.z > 1.9);
        assert!(result.position.x <= -1.25);
    }

    #[test]
    fn outer_bounds_stop_character() {
        let result =
            world().move_character(Vec3::new(4.0, 0.0, 0.0), Vec3::new(3.0, 0.0, 0.0), 0.25);
        assert!(result.blocked_x);
        assert!((result.position.x - 4.75).abs() < 0.001);
    }

    #[test]
    fn very_large_displacement_does_not_tunnel_through_obstacle() {
        let result =
            world().move_character(Vec3::new(-4.0, 0.0, 0.0), Vec3::new(8.0, 0.0, 0.0), 0.25);
        assert!(result.blocked_x);
        assert!(result.position.x <= -1.25);
    }


    #[test]
    fn vehicle_footprint_stops_before_obstacle() {
        let result = world().move_vehicle(
            Vec3::new(-3.0, 0.0, 0.0),
            Vec3::new(3.0, 0.0, 0.0),
            0.8,
            1.5,
        );
        assert!(result.blocked_x);
        assert!(result.position.x <= -1.8);
        assert!(result.position.x > -1.9);
    }

    #[test]
    fn vehicle_footprint_respects_outer_bounds() {
        let result = world().move_vehicle(
            Vec3::new(3.0, 0.0, 0.0),
            Vec3::new(3.0, 0.0, 0.0),
            0.8,
            1.5,
        );
        assert!(result.blocked_x);
        assert!((result.position.x - 4.2).abs() < 0.01);
    }

    #[test]
    fn invalid_vehicle_footprint_leaves_position_unchanged() {
        let start = Vec3::new(0.0, 1.0, 2.0);
        let result = world().move_vehicle(start, Vec3::new(1.0, 0.0, 0.0), f32::NAN, 1.0);
        assert_eq!(result.position, start);
    }

    #[test]
    fn invalid_inputs_leave_character_at_start() {
        let start = Vec3::new(0.0, 0.0, 2.0);
        let result = world().move_character(start, Vec3::new(f32::NAN, 0.0, 0.0), 0.25);
        assert_eq!(result.position, start);
    }
}
