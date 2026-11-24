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

/// Samples all four tyre contact locations and selects their dominant surface.
///
/// Ties fall back to the chassis-center surface, keeping narrow roads stable
/// when the vehicle straddles two equally represented zones.
#[must_use]
pub fn terrain_vehicle_surface(
    world_x: f32,
    world_z: f32,
    heading: f32,
    half_track: f32,
    half_wheelbase: f32,
) -> crate::VehicleSurface {
    use crate::VehicleSurface;

    if !world_x.is_finite()
        || !world_z.is_finite()
        || !heading.is_finite()
        || !half_track.is_finite()
        || !half_wheelbase.is_finite()
        || half_track <= 0.0
        || half_wheelbase <= 0.0
    {
        return VehicleSurface::Rock;
    }

    let (sin_heading, cos_heading) = heading.sin_cos();
    let mut counts = [0_u8; 6];
    for local_x in [-half_track, half_track] {
        for local_z in [-half_wheelbase, half_wheelbase] {
            let contact_x = world_x + cos_heading * local_x + sin_heading * local_z;
            let contact_z = world_z - sin_heading * local_x + cos_heading * local_z;
            let surface = terrain_surface(contact_x, contact_z);
            counts[match surface {
                VehicleSurface::Asphalt => 0,
                VehicleSurface::Gravel => 1,
                VehicleSurface::Mud => 2,
                VehicleSurface::Sand => 3,
                VehicleSurface::Snow => 4,
                VehicleSurface::Rock => 5,
            }] += 1;
        }
    }

    let center_surface = terrain_surface(world_x, world_z);
    let center_index = match center_surface {
        VehicleSurface::Asphalt => 0,
        VehicleSurface::Gravel => 1,
        VehicleSurface::Mud => 2,
        VehicleSurface::Sand => 3,
        VehicleSurface::Snow => 4,
        VehicleSurface::Rock => 5,
    };
    let dominant_count = counts.iter().copied().max().unwrap_or(0);
    if counts[center_index] == dominant_count {
        return center_surface;
    }
    match counts.iter().position(|count| *count == dominant_count) {
        Some(0) => VehicleSurface::Asphalt,
        Some(1) => VehicleSurface::Gravel,
        Some(2) => VehicleSurface::Mud,
        Some(3) => VehicleSurface::Sand,
        Some(4) => VehicleSurface::Snow,
        Some(_) | None => VehicleSurface::Rock,
    }
}

#[cfg(test)]
mod vehicle_surface_tests {
    use super::terrain_vehicle_surface;
    use crate::VehicleSurface;

    #[test]
    fn vehicle_surface_uses_all_four_wheel_contacts() {
        assert_eq!(
            terrain_vehicle_surface(-5.0, 4.0, 0.0, 0.3, 0.5),
            VehicleSurface::Mud
        );
        assert_eq!(
            terrain_vehicle_surface(5.0, -4.0, 0.0, 0.3, 0.5),
            VehicleSurface::Sand
        );
        assert_eq!(
            terrain_vehicle_surface(4.0, -8.0, 0.0, 0.3, 0.5),
            VehicleSurface::Snow
        );
    }

    #[test]
    fn vehicle_surface_ties_remain_stable_at_road_edges() {
        assert_eq!(
            terrain_vehicle_surface(0.9, 0.0, 0.0, 0.78, 1.32),
            VehicleSurface::Asphalt
        );
    }

    #[test]
    fn invalid_vehicle_surface_samples_fall_back_to_rock() {
        assert_eq!(
            terrain_vehicle_surface(0.0, 0.0, f32::NAN, 0.8, 1.3),
            VehicleSurface::Rock
        );
        assert_eq!(
            terrain_vehicle_surface(0.0, 0.0, 0.0, -0.8, 1.3),
            VehicleSurface::Rock
        );
    }
}
