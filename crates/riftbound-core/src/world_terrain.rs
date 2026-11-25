//! Deterministic, allocation-free terrain sampling for the wider explorable world.
//!
//! The arena's authored terrain remains available through `terrain_height` and
//! `terrain_surface`. These world queries provide a separate, unbounded region
//! foundation so streamed chunks can be generated from stable world coordinates.

use crate::VehicleSurface;

/// A sample of the primary winding road crossing the procedural world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldRoadSample {
    /// Road centreline X coordinate at the supplied Z coordinate.
    pub center_x: f32,
    /// Absolute lateral distance from the road centreline.
    pub lateral_distance: f32,
    /// Half-width of the drivable asphalt surface.
    pub half_width: f32,
    /// Width of the gravel shoulder beyond each asphalt edge.
    pub shoulder_width: f32,
}

impl WorldRoadSample {
    /// Returns true when the sample lies on the asphalt driving surface.
    #[must_use]
    pub fn is_on_road(self) -> bool {
        self.lateral_distance <= self.half_width
    }

    /// Returns true when the sample lies on asphalt or its gravel shoulder.
    #[must_use]
    pub fn is_on_road_corridor(self) -> bool {
        self.lateral_distance <= self.half_width + self.shoulder_width
    }
}

/// Samples the winding primary road in absolute world coordinates.
#[must_use]
pub fn world_road_sample(world_x: f32, world_z: f32) -> WorldRoadSample {
    const ROAD_HALF_WIDTH: f32 = 5.5;
    const ROAD_SHOULDER_WIDTH: f32 = 2.5;
    if !world_x.is_finite()
        || !world_z.is_finite()
        || world_x.abs() > 1_000_000.0
        || world_z.abs() > 1_000_000.0
    {
        return WorldRoadSample {
            center_x: 0.0,
            lateral_distance: f32::INFINITY,
            half_width: ROAD_HALF_WIDTH,
            shoulder_width: ROAD_SHOULDER_WIDTH,
        };
    }

    let center_x = (world_z * 0.0012).sin() * 7.0;
    WorldRoadSample {
        center_x,
        lateral_distance: (world_x - center_x).abs(),
        half_width: ROAD_HALF_WIDTH,
        shoulder_width: ROAD_SHOULDER_WIDTH,
    }
}

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
    let ground_height = broad * 15.0 + detail * 2.2 + ridge.max(0.0) * 3.5 - 2.0;

    let road = world_road_sample(world_x, world_z);
    // A short eastbound spur branches from the primary road at the origin.
    // Its fixed world-space bounds make the junction deterministic across chunks.
    let spur_distance = (world_z - 0.0).abs();
    let on_spur = (0.0..=160.0).contains(&world_x) && spur_distance <= 5.5;
    let on_spur_shoulder = (0.0..=162.5).contains(&world_x) && spur_distance <= 8.0 && !on_spur;

    // A northbound branch leaves the eastbound road at X=80 and runs
    // to Z=-128. Its fixed bounds make it stable across streamed chunks.
    let branch_distance = (world_x - 80.0).abs();
    let on_branch = (-128.0..=0.0).contains(&world_z) && branch_distance <= 5.5;
    let on_branch_shoulder =
        (-130.5..=0.0).contains(&world_z) && branch_distance <= 8.0 && !on_branch;

    // The north branch crosses a shallow ravine. A smooth 8-unit approach
    // raises the bridge deck by four units, and the same sample feeds vehicle
    // suspension so driving physics meets the rendered road surface.
    let bridge_corridor = (72.0..=88.0).contains(&world_x) && (-88.0..=-56.0).contains(&world_z);
    let bridge_distance_to_ramp = (world_z + 88.0).min(-56.0 - world_z);
    let bridge_blend = smoothstep((bridge_distance_to_ramp / 8.0).clamp(0.0, 1.0));
    // Fade the raised deck across its side edges as well as its approaches.
    // This avoids a vertical height discontinuity where the bridge meets terrain.
    let bridge_distance_to_edge = (world_x - 72.0).min(88.0 - world_x);
    let bridge_side_blend = smoothstep((bridge_distance_to_edge / 2.0).clamp(0.0, 1.0));
    let road_blend = corridor_blend(road.lateral_distance, road.half_width, road.shoulder_width);
    let road_bed_height = procedural_ground_height(road.center_x, world_z);

    let spur_blend = if (0.0..=160.0).contains(&world_x) {
        corridor_blend(spur_distance, 5.5, 2.5)
    } else {
        0.0
    };
    let spur_bed_height = procedural_ground_height(world_x, 0.0);

    let branch_blend = if (-128.0..=0.0).contains(&world_z) {
        corridor_blend(branch_distance, 5.5, 2.5)
    } else {
        0.0
    };
    let branch_bed_height = procedural_ground_height(80.0, world_z);

    // Blend each corridor independently. Their centre lines meet at matching
    // junction elevations, so streamed chunks sample the same continuous road bed.
    let road_adjusted_height = lerp(ground_height, road_bed_height, road_blend);
    let spur_adjusted_height = lerp(road_adjusted_height, spur_bed_height, spur_blend);
    let branch_adjusted_height = lerp(spur_adjusted_height, branch_bed_height, branch_blend);
    let height = branch_adjusted_height
        + if bridge_corridor {
            4.0 * bridge_blend * bridge_side_blend
        } else {
            0.0
        };

    let surface = if road.is_on_road() || on_spur || on_branch {
        VehicleSurface::Asphalt
    } else if road.is_on_road_corridor() || on_spur_shoulder || on_branch_shoulder {
        VehicleSurface::Gravel
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

/// Samples absolute world-terrain elevation at the four tyre contact points.
///
/// Corner order is front-left, front-right, rear-left, rear-right. Invalid vehicle
/// geometry returns `None`, allowing callers to preserve their last valid pose.
#[must_use]
pub fn world_terrain_vehicle_ground_heights(
    world_x: f32,
    world_z: f32,
    heading: f32,
    half_track: f32,
    half_wheelbase: f32,
) -> Option<[f32; 4]> {
    if !world_x.is_finite()
        || !world_z.is_finite()
        || !heading.is_finite()
        || !half_track.is_finite()
        || !half_wheelbase.is_finite()
        || half_track <= 0.0
        || half_wheelbase <= 0.0
    {
        return None;
    }

    let contacts =
        vehicle_contact_positions(world_x, world_z, heading, half_track, half_wheelbase)?;
    Some(contacts.map(|(contact_x, contact_z)| world_terrain_sample(contact_x, contact_z).height))
}

/// Returns the terrain surface under each tyre in suspension corner order.
///
/// Invalid vehicle geometry safely maps every contact to rock.
#[must_use]
pub fn world_terrain_vehicle_surfaces(
    world_x: f32,
    world_z: f32,
    heading: f32,
    half_track: f32,
    half_wheelbase: f32,
) -> [VehicleSurface; 4] {
    if !world_x.is_finite()
        || !world_z.is_finite()
        || !heading.is_finite()
        || !half_track.is_finite()
        || !half_wheelbase.is_finite()
        || half_track <= 0.0
        || half_wheelbase <= 0.0
    {
        return [VehicleSurface::Rock; 4];
    }

    let Some(contacts) =
        vehicle_contact_positions(world_x, world_z, heading, half_track, half_wheelbase)
    else {
        return [VehicleSurface::Rock; 4];
    };
    contacts.map(|(contact_x, contact_z)| world_terrain_sample(contact_x, contact_z).surface)
}

/// Samples the dominant world surface beneath all four tyre contact points.
///
/// The centre surface wins ties, keeping handling stable when a vehicle straddles
/// a road edge. This uses the same wheel-space convention as suspension sampling.
#[must_use]
pub fn world_terrain_vehicle_surface(
    world_x: f32,
    world_z: f32,
    heading: f32,
    half_track: f32,
    half_wheelbase: f32,
) -> VehicleSurface {
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

    let Some(contacts) =
        vehicle_contact_positions(world_x, world_z, heading, half_track, half_wheelbase)
    else {
        return VehicleSurface::Rock;
    };
    let mut counts = [0_u8; 6];
    for (contact_x, contact_z) in contacts {
        let surface = world_terrain_sample(contact_x, contact_z).surface;
        counts[surface_index(surface)] += 1;
    }

    let center_surface = world_terrain_sample(world_x, world_z).surface;
    let dominant_count = counts.iter().copied().max().unwrap_or(0);
    if counts[surface_index(center_surface)] == dominant_count {
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

fn vehicle_contact_positions(
    world_x: f32,
    world_z: f32,
    heading: f32,
    half_track: f32,
    half_wheelbase: f32,
) -> Option<[(f32, f32); 4]> {
    if !world_x.is_finite()
        || !world_z.is_finite()
        || !heading.is_finite()
        || !half_track.is_finite()
        || !half_wheelbase.is_finite()
        || half_track <= 0.0
        || half_wheelbase <= 0.0
    {
        return None;
    }

    let (sin_heading, cos_heading) = heading.sin_cos();
    let offsets = [
        (-half_track, -half_wheelbase),
        (half_track, -half_wheelbase),
        (-half_track, half_wheelbase),
        (half_track, half_wheelbase),
    ];
    let mut contacts = [(0.0, 0.0); 4];
    for (index, (local_x, local_z)) in offsets.into_iter().enumerate() {
        let contact_x = world_x + cos_heading * local_x + sin_heading * local_z;
        let contact_z = world_z - sin_heading * local_x + cos_heading * local_z;
        if !contact_x.is_finite()
            || !contact_z.is_finite()
            || contact_x.abs() > 1_000_000.0
            || contact_z.abs() > 1_000_000.0
        {
            return None;
        }
        contacts[index] = (contact_x, contact_z);
    }
    Some(contacts)
}

const fn surface_index(surface: VehicleSurface) -> usize {
    match surface {
        VehicleSurface::Asphalt => 0,
        VehicleSurface::Gravel => 1,
        VehicleSurface::Mud => 2,
        VehicleSurface::Sand => 3,
        VehicleSurface::Snow => 4,
        VehicleSurface::Rock => 5,
    }
}

fn corridor_blend(lateral_distance: f32, half_width: f32, shoulder_width: f32) -> f32 {
    if lateral_distance <= half_width {
        1.0
    } else {
        let shoulder_progress = ((lateral_distance - half_width) / shoulder_width).clamp(0.0, 1.0);
        1.0 - smoothstep(shoulder_progress)
    }
}

fn procedural_ground_height(world_x: f32, world_z: f32) -> f32 {
    let broad = value_noise(world_x * 0.004, world_z * 0.004);
    let detail = value_noise(world_x * 0.018 + 31.7, world_z * 0.018 - 19.3);
    let ridge = 1.0 - value_noise(world_x * 0.0025 - 7.1, world_z * 0.0025 + 13.9).abs();
    broad * 15.0 + detail * 2.2 + ridge.max(0.0) * 3.5 - 2.0
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
    use super::{
        WorldTerrainSample, corridor_blend, lerp, procedural_ground_height, value_noise,
        vehicle_contact_positions, world_road_sample, world_terrain_sample,
        world_terrain_vehicle_ground_heights, world_terrain_vehicle_surface,
        world_terrain_vehicle_surfaces,
    };
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
    fn winding_road_sample_has_stable_width_and_gravel_shoulders() {
        let center = world_road_sample(0.0, 0.0);
        assert!(center.is_on_road());
        assert!(center.is_on_road_corridor());
        assert_eq!(center.center_x, 0.0);
        assert!((center.half_width - 5.5).abs() < f32::EPSILON);

        let asphalt_edge = world_road_sample(center.center_x + center.half_width, 0.0);
        assert!(asphalt_edge.is_on_road());
        let shoulder = world_road_sample(center.center_x + center.half_width + 1.0, 0.0);
        assert!(!shoulder.is_on_road());
        assert!(shoulder.is_on_road_corridor());
        let outside = world_road_sample(center.center_x + 20.0, 0.0);
        assert!(!outside.is_on_road_corridor());
        assert_eq!(center, world_road_sample(0.0, 0.0));
    }

    #[test]
    fn road_surface_tracks_the_winding_centreline() {
        for world_z in [-900.0, -250.0, 12.0, 375.0, 900.0] {
            let road = world_road_sample(0.0, world_z);
            assert_eq!(
                world_terrain_sample(road.center_x, world_z).surface,
                VehicleSurface::Asphalt
            );
            assert_eq!(
                world_terrain_sample(road.center_x + road.half_width + 1.0, world_z).surface,
                VehicleSurface::Gravel
            );
        }
    }

    #[test]
    fn eastbound_spur_has_asphalt_and_gravel_shoulders() {
        assert_eq!(
            world_terrain_sample(16.0, 0.0).surface,
            VehicleSurface::Asphalt
        );
        assert_eq!(
            world_terrain_sample(16.0, 6.5).surface,
            VehicleSurface::Gravel
        );
        assert_ne!(
            world_terrain_sample(16.0, 10.0).surface,
            VehicleSurface::Asphalt
        );
        assert_eq!(
            world_terrain_sample(32.0, 0.0).surface,
            VehicleSurface::Asphalt
        );
        assert_eq!(
            world_terrain_sample(128.0, 0.0).surface,
            VehicleSurface::Asphalt
        );
        assert_eq!(
            world_terrain_sample(128.0, 6.5).surface,
            VehicleSurface::Gravel
        );
        assert_ne!(
            world_terrain_sample(164.0, 0.0).surface,
            VehicleSurface::Asphalt
        );
    }

    #[test]
    fn northbound_bridge_elevation_has_smooth_approaches() {
        let ground_height = |world_x: f32, world_z: f32| {
            let broad = value_noise(world_x * 0.004, world_z * 0.004);
            let detail = value_noise(world_x * 0.018 + 31.7, world_z * 0.018 - 19.3);
            let ridge = 1.0 - value_noise(world_x * 0.0025 - 7.1, world_z * 0.0025 + 13.9).abs();
            broad * 15.0 + detail * 2.2 + ridge.max(0.0) * 3.5 - 2.0
        };

        for world_z in [-88.0, -56.0] {
            let sample = world_terrain_sample(80.0, world_z);
            assert!((sample.height - ground_height(80.0, world_z)).abs() < 1.0e-4);
        }
        let bridge_deck = world_terrain_sample(80.0, -72.0);
        assert!((bridge_deck.height - ground_height(80.0, -72.0) - 4.0).abs() < 1.0e-4);
        let approach = world_terrain_sample(80.0, -84.0);
        assert!((approach.height - ground_height(80.0, -84.0) - 2.0).abs() < 1.0e-4);

        // The deck fades to the surrounding ground at both side edges.
        for world_x in [72.0, 88.0] {
            let edge = world_terrain_sample(world_x, -72.0);
            assert!((edge.height - ground_height(world_x, -72.0)).abs() < 1.0e-4);
        }
        let side_approach = world_terrain_sample(73.0, -72.0);
        let side_approach_base = lerp(
            ground_height(73.0, -72.0),
            ground_height(80.0, -72.0),
            corridor_blend(7.0, 5.5, 2.5),
        );
        assert!((side_approach.height - side_approach_base - 2.0).abs() < 1.0e-4);

        assert_eq!(
            world_terrain_sample(96.0, -72.0).height,
            ground_height(96.0, -72.0)
        );
    }

    #[test]
    fn northbound_branch_has_asphalt_and_gravel_shoulders() {
        for world_z in [-16.0, -64.0, -120.0, -128.0] {
            assert_eq!(
                world_terrain_sample(80.0, world_z).surface,
                VehicleSurface::Asphalt
            );
            assert_eq!(
                world_terrain_sample(86.5, world_z).surface,
                VehicleSurface::Gravel
            );
        }
        assert_ne!(
            world_terrain_sample(80.0, -136.0).surface,
            VehicleSurface::Asphalt
        );
        assert_ne!(
            world_terrain_sample(96.0, -64.0).surface,
            VehicleSurface::Asphalt
        );
    }

    #[test]
    fn world_surface_zones_include_road_and_distinct_biomes() {
        assert_eq!(
            world_terrain_sample(0.0, 0.0).surface,
            VehicleSurface::Asphalt
        );
        assert_eq!(
            world_terrain_sample(30.0, -800.0).surface,
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
    fn world_vehicle_ground_heights_follow_heading_and_corner_order() {
        let heights = world_terrain_vehicle_ground_heights(125.0, -80.0, 0.7, 0.78, 1.32)
            .expect("valid vehicle geometry should sample all four corners");
        assert!(heights.iter().all(|height| height.is_finite()));
        assert_ne!(heights[0], heights[3]);
    }

    #[test]
    fn vehicle_ground_sampling_rejects_wheel_contacts_outside_world_precision() {
        assert_eq!(
            world_terrain_vehicle_ground_heights(999_999.5, 0.0, 0.0, 1.0, 1.0),
            None
        );
    }

    #[test]
    fn invalid_world_vehicle_ground_geometry_returns_none() {
        assert_eq!(
            world_terrain_vehicle_ground_heights(0.0, 0.0, f32::NAN, 0.8, 1.3),
            None
        );
        assert_eq!(
            world_terrain_vehicle_ground_heights(0.0, 0.0, 0.0, 0.8, 0.0),
            None
        );
    }

    #[test]
    fn world_vehicle_surfaces_preserve_each_wheel_contact() {
        let contacts = world_terrain_vehicle_surfaces(2.5, 12.0, 0.0, 4.0, 1.0);
        assert_eq!(contacts[0], VehicleSurface::Asphalt);
        assert_eq!(contacts[1], VehicleSurface::Gravel);
        assert_eq!(contacts[2], VehicleSurface::Asphalt);
        assert_eq!(contacts[3], VehicleSurface::Gravel);
    }

    #[test]
    fn world_vehicle_surface_uses_four_wheel_contacts_across_biomes() {
        assert_eq!(
            world_terrain_vehicle_surface(800.0, 0.0, 0.0, 0.3, 0.5),
            VehicleSurface::Sand
        );
        assert_eq!(
            world_terrain_vehicle_surface(-800.0, 0.0, 0.0, 0.3, 0.5),
            VehicleSurface::Mud
        );
        assert_eq!(
            world_terrain_vehicle_surface(30.0, -800.0, 0.0, 0.3, 0.5),
            VehicleSurface::Snow
        );
    }

    #[test]
    fn world_vehicle_surface_ties_favor_the_center_surface() {
        assert_eq!(
            world_terrain_vehicle_surface(
                world_road_sample(0.0, 20.0).center_x + 6.0,
                20.0,
                0.0,
                0.78,
                1.32
            ),
            VehicleSurface::Gravel
        );
    }

    #[test]
    fn wheel_surface_sampling_falls_back_atomically_at_world_precision_edge() {
        let surfaces = world_terrain_vehicle_surfaces(999_999.5, 0.0, 0.0, 1.0, 1.0);
        assert_eq!(surfaces, [VehicleSurface::Rock; 4]);
    }

    #[test]
    fn invalid_world_vehicle_surface_inputs_fall_back_to_rock() {
        assert_eq!(
            world_terrain_vehicle_surface(0.0, 0.0, f32::NAN, 0.8, 1.3),
            VehicleSurface::Rock
        );
        assert_eq!(
            world_terrain_vehicle_surface(0.0, 0.0, 0.0, 0.8, f32::INFINITY),
            VehicleSurface::Rock
        );
    }

    #[test]
    fn winding_road_surface_blends_into_terrain_without_height_steps() {
        let road = world_road_sample(0.0, 240.0);
        let center = world_terrain_sample(road.center_x, 240.0).height;
        let inside_left = world_terrain_sample(road.center_x - 3.0, 240.0).height;
        let inside_right = world_terrain_sample(road.center_x + 3.0, 240.0).height;
        assert!((inside_left - center).abs() < 1.0e-5);
        assert!((inside_right - center).abs() < 1.0e-5);

        let shoulder = world_terrain_sample(
            road.center_x + road.half_width + road.shoulder_width * 0.5,
            240.0,
        )
        .height;
        let natural_shoulder = procedural_ground_height(
            road.center_x + road.half_width + road.shoulder_width * 0.5,
            240.0,
        );
        assert!((shoulder - center).abs() <= (natural_shoulder - center).abs());
    }

    #[test]
    fn eastbound_spur_and_northbound_branch_have_continuous_road_beds() {
        let spur_center = world_terrain_sample(40.0, 0.0).height;
        let spur_left = world_terrain_sample(40.0, -3.0).height;
        let spur_right = world_terrain_sample(40.0, 3.0).height;
        assert!((spur_left - spur_center).abs() < 1.0e-5);
        assert!((spur_right - spur_center).abs() < 1.0e-5);

        let branch_center = world_terrain_sample(80.0, -40.0).height;
        let branch_left = world_terrain_sample(77.0, -40.0).height;
        let branch_right = world_terrain_sample(83.0, -40.0).height;
        assert!((branch_left - branch_center).abs() < 1.0e-5);
        assert!((branch_right - branch_center).abs() < 1.0e-5);
    }

    #[test]
    fn vehicle_queries_share_identical_validated_wheel_contacts() {
        let position = (24.0, -18.0, 0.73, 0.82, 1.31);
        let heights = world_terrain_vehicle_ground_heights(
            position.0, position.1, position.2, position.3, position.4,
        )
        .expect("valid wheel contacts");
        let surfaces = world_terrain_vehicle_surfaces(
            position.0, position.1, position.2, position.3, position.4,
        );
        let direct =
            vehicle_contact_positions(position.0, position.1, position.2, position.3, position.4)
                .expect("same validated contacts");
        for index in 0..4 {
            let sample = world_terrain_sample(direct[index].0, direct[index].1);
            assert_eq!(heights[index], sample.height);
            assert_eq!(surfaces[index], sample.surface);
        }
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
