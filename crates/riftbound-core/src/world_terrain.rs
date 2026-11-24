//! Deterministic, allocation-free terrain sampling for the wider explorable world.
//!
//! The arena's authored terrain remains available through `terrain_height` and
//! `terrain_surface`. These world queries provide a separate, unbounded region
//! foundation so streamed chunks can be generated from stable world coordinates.

use crate::VehicleSurface;

/// A deterministic terrain sample suitable for both world mesh generation and driving.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldTerrainSample {
    /// Ground elevation in world units.
    pub height: f32,
    /// Dominant gameplay surface for vehicle handling.
    pub surface: VehicleSurface,
}

/// Samples the procedural world terrain at absolute world-space X/Z coordinates.
///
/// The result is deterministic across calls and independent of chunk load order.
/// Coordinates outside the supported precision range return a safe rock sample.
#[must_use]
pub fn world_terrain_sample(world_x: f32, world_z: f32) -> WorldTerrainSample {
    if !world_x.is_finite()
        || !world_z.is_finite()
        || world_x.abs() > 1_000_000.0
        || world_z.abs() > 1_000_000.0
    {
        return WorldTerrainSample {
            height: 0.0,
            surface: VehicleSurface::Rock,
        };
    }

    let broad = value_noise(world_x * 0.004, world_z * 0.004);
    let detail = value_noise(world_x * 0.018 + 31.7, world_z * 0.018 - 19.3);
    let ridge = 1.0 - value_noise(world_x * 0.0025 - 7.1, world_z * 0.0025 + 13.9).abs();
    let height = broad * 15.0 + detail * 2.2 + ridge.max(0.0) * 3.5 - 2.0;

    let road_center = (world_z * 0.0012).sin() * 7.0;
    let surface = if (world_x - road_center).abs() <= 2.4 {
        VehicleSurface::Asphalt
    } else if world_z < -650.0 {
        VehicleSurface::Snow
    } else if world_x > 650.0 {
        VehicleSurface::Sand
    } else if world_x < -650.0 {
        VehicleSurface::Mud
    } else if detail > 0.42 || ridge > 0.82 {
        VehicleSurface::Rock
    } else {
        VehicleSurface::Gravel
    };

    WorldTerrainSample { height, surface }
}

// Input coordinates are bounded by world_terrain_sample before reaching this helper.
#[allow(clippy::cast_possible_truncation)]
fn value_noise(sample_x: f32, sample_z: f32) -> f32 {
    let grid_x = sample_x.floor() as i32;
    let grid_z = sample_z.floor() as i32;
    let fraction_x = smoothstep(sample_x - sample_x.floor());
    let fraction_z = smoothstep(sample_z - sample_z.floor());

    let northwest = lattice_value(grid_x, grid_z);
    let northeast = lattice_value(grid_x.saturating_add(1), grid_z);
    let southwest = lattice_value(grid_x, grid_z.saturating_add(1));
    let southeast = lattice_value(grid_x.saturating_add(1), grid_z.saturating_add(1));

    lerp(
        lerp(northwest, northeast, fraction_x),
        lerp(southwest, southeast, fraction_x),
        fraction_z,
    )
}

fn lattice_value(grid_x: i32, grid_z: i32) -> f32 {
    let mut hash = grid_x.cast_unsigned().wrapping_mul(0x9E37_79B9);
    hash ^= grid_z.cast_unsigned().wrapping_mul(0x85EB_CA6B);
    hash ^= hash >> 16;
    hash = hash.wrapping_mul(0x7FEB_352D);
    hash ^= hash >> 15;
    hash = hash.wrapping_mul(0x846C_A68B);
    hash ^= hash >> 16;

    // Split into 16-bit pieces so each integer-to-float conversion is exact.
    let high = u16::try_from(hash >> 16).expect("upper hash half fits u16");
    let low = u16::try_from(hash & 0xFFFF).expect("lower hash half fits u16");
    let normalized = (f32::from(high) * 65_536.0 + f32::from(low)) / 4_294_967_295.0;
    normalized * 2.0 - 1.0
}

fn smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

fn lerp(start: f32, end: f32, amount: f32) -> f32 {
    start + (end - start) * amount
}

#[cfg(test)]
mod tests {
    use super::{WorldTerrainSample, world_terrain_sample};
    use crate::VehicleSurface;

    #[test]
    fn world_samples_are_repeatable_and_finite() {
        for (x, z) in [(0.0, 0.0), (125.5, -72.25), (-900.0, 840.0)] {
            let first = world_terrain_sample(x, z);
            assert_eq!(first, world_terrain_sample(x, z));
            assert!(first.height.is_finite());
            assert!(first.height.abs() < 30.0);
        }
    }

    #[test]
    fn world_surface_zones_include_road_and_distinct_biomes() {
        assert_eq!(
            world_terrain_sample(0.0, 0.0).surface,
            VehicleSurface::Asphalt
        );
        assert_eq!(
            world_terrain_sample(0.0, -800.0).surface,
            VehicleSurface::Snow
        );
        assert_eq!(
            world_terrain_sample(800.0, 0.0).surface,
            VehicleSurface::Sand
        );
        assert_eq!(
            world_terrain_sample(-800.0, 0.0).surface,
            VehicleSurface::Mud
        );
    }

    #[test]
    fn adjacent_world_samples_change_smoothly() {
        let first = world_terrain_sample(123.0, 456.0).height;
        let second = world_terrain_sample(123.1, 456.0).height;
        assert!((first - second).abs() < 0.5);
    }

    #[test]
    fn invalid_or_imprecise_coordinates_return_safe_fallback() {
        let fallback = WorldTerrainSample {
            height: 0.0,
            surface: VehicleSurface::Rock,
        };
        assert_eq!(world_terrain_sample(f32::NAN, 0.0), fallback);
        assert_eq!(world_terrain_sample(1_000_001.0, 0.0), fallback);
    }
}
