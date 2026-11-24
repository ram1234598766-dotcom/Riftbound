//! Shared deterministic terrain sampling for rendering and gameplay.

/// Returns the arena terrain height at world-space X/Z coordinates.
///
/// The terrain spans -10..=10 on both horizontal axes and fades to zero at
/// its perimeter so it meets the arena boundary cleanly.
#[must_use]
pub fn terrain_height(world_x: f32, world_z: f32) -> f32 {
    let u = f32::midpoint(world_x / 10.0, 1.0);
    let v = f32::midpoint(world_z / 10.0, 1.0);
    if !(0.0..=1.0).contains(&u)
        || !(0.0..=1.0).contains(&v)
        || u <= 0.0
        || u >= 1.0
        || v <= 0.0
        || v >= 1.0
    {
        return 0.0;
    }
    let edge_fade = (std::f32::consts::PI * u).sin() * (std::f32::consts::PI * v).sin();
    let ridges = (u * std::f32::consts::TAU * 2.0).sin() * (v * std::f32::consts::TAU * 1.5).cos();
    edge_fade * ridges * 0.075
}

/// Returns the authored handling surface at a world-space position.
///
/// This first-region layout is deliberately data-only for now: renderer material
/// zones will consume the same query when terrain material blending is added.
#[must_use]
pub fn terrain_surface(world_x: f32, world_z: f32) -> crate::VehicleSurface {
    use crate::VehicleSurface;

    if !world_x.is_finite() || !world_z.is_finite() {
        return VehicleSurface::Rock;
    }
    if world_x.abs() > 10.0 || world_z.abs() > 10.0 {
        return VehicleSurface::Rock;
    }

    // A narrow north-south paved route provides the starter region's road spine.
    if world_x.abs() <= 1.2 {
        VehicleSurface::Asphalt
    } else if world_x < -4.0 && world_z > 1.5 {
        VehicleSurface::Mud
    } else if world_x > 4.0 && world_z < -1.5 {
        VehicleSurface::Sand
    } else if world_z < -7.0 {
        VehicleSurface::Snow
    } else if world_x.abs() > 6.5 || world_z.abs() > 6.5 {
        VehicleSurface::Rock
    } else {
        VehicleSurface::Gravel
    }
}

#[cfg(test)]
mod tests {
    use super::{terrain_height, terrain_surface};
    use crate::VehicleSurface;

    #[test]
    fn terrain_is_flat_at_and_beyond_the_arena_edges() {
        assert_eq!(terrain_height(-10.0, 0.0), 0.0);
        assert_eq!(terrain_height(10.0, 0.0), 0.0);
        assert_eq!(terrain_height(0.0, -10.0), 0.0);
        assert_eq!(terrain_height(0.0, 10.0), 0.0);
        assert_eq!(terrain_height(10.1, 0.0), 0.0);
    }

    #[test]
    fn terrain_surface_regions_provide_distinct_vehicle_handling() {
        assert_eq!(terrain_surface(0.0, 0.0), VehicleSurface::Asphalt);
        assert_eq!(terrain_surface(-5.0, 4.0), VehicleSurface::Mud);
        assert_eq!(terrain_surface(5.0, -4.0), VehicleSurface::Sand);
        assert_eq!(terrain_surface(4.0, -8.0), VehicleSurface::Snow);
        assert_eq!(terrain_surface(8.0, 2.0), VehicleSurface::Rock);
        assert_eq!(terrain_surface(3.0, 3.0), VehicleSurface::Gravel);
    }

    #[test]
    fn invalid_surface_coordinates_fall_back_to_rock() {
        assert_eq!(terrain_surface(f32::NAN, 0.0), VehicleSurface::Rock);
        assert_eq!(terrain_surface(0.0, f32::INFINITY), VehicleSurface::Rock);
    }

    #[test]
    fn terrain_height_is_finite_and_bounded() {
        for step in -100_i16..=100 {
            let coordinate = f32::from(step) / 10.0;
            let height = terrain_height(coordinate, -coordinate);
            assert!(height.is_finite());
            assert!(height.abs() <= 0.075);
        }
    }
}
