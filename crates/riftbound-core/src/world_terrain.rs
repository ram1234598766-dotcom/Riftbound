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

fn value_noise(x: f32, z: f32) -> f32 {
    let x0 = x.floor() as i32;
    let z0 = z.floor() as i32;
    let tx = smoothstep(x - x.floor());
    let tz = smoothstep(z - z.floor());

    let a = lattice_value(x0, z0);
    let b = lattice_value(x0.saturating_add(1), z0);
    let c = lattice_value(x0, z0.saturating_add(1));
    let d = lattice_value(x0.saturating_add(1), z0.saturating_add(1));

    lerp(lerp(a, b, tx), lerp(c, d, tx), tz)
}

fn lattice_value(x: i32, z: i32) -> f32 {
    let mut hash = (x as u32).wrapping_mul(0x9E37_79B9);
    hash ^= (z as u32).wrapping_mul(0x85EB_CA6B);
    hash ^= hash >> 16;
    hash = hash.wrapping_mul(0x7FEB_352D);
    hash ^= hash >> 15;
    hash = hash.wrapping_mul(0x846C_A68B);
    hash ^= hash >> 16;
    (hash as f32 / u32::MAX as f32) * 2.0 - 1.0
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
        assert_eq!(world_terrain_sample(0.0, 0.0).surface, VehicleSurface::Asphalt);
        assert_eq!(world_terrain_sample(0.0, -800.0).surface, VehicleSurface::Snow);
        assert_eq!(world_terrain_sample(800.0, 0.0).surface, VehicleSurface::Sand);
        assert_eq!(world_terrain_sample(-800.0, 0.0).surface, VehicleSurface::Mud);
    }

    #[test]
    fn adjacent_world_samples_change_smoothly() {
        let first = world_terrain_sample(123.0, 456.0).height;
        let second = world_terrain_sample(123.1, 456.0).height;
        assert!((first - second).abs() < 0.5);
    }

    #[test]
    fn invalid_or_imprecise_coordinates_return_safe_fallback() {
        let fallback = WorldTerrainSample { height: 0.0, surface: VehicleSurface::Rock };
        assert_eq!(world_terrain_sample(f32::NAN, 0.0), fallback);
        assert_eq!(world_terrain_sample(1_000_001.0, 0.0), fallback);
    }
}
