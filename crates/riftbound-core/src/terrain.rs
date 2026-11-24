//! Shared deterministic terrain sampling for rendering and gameplay.

/// Returns the arena terrain height at world-space X/Z coordinates.
///
/// The terrain spans -10..=10 on both horizontal axes and fades to zero at
/// its perimeter so it meets the arena boundary cleanly.
#[must_use]
pub fn terrain_height(world_x: f32, world_z: f32) -> f32 {
    let u = f32::midpoint(world_x / 10.0, 1.0);
    let v = f32::midpoint(world_z / 10.0, 1.0);
    if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
        return 0.0;
    }
    let edge_fade = (std::f32::consts::PI * u).sin() * (std::f32::consts::PI * v).sin();
    let ridges = (u * std::f32::consts::TAU * 2.0).sin() * (v * std::f32::consts::TAU * 1.5).cos();
    edge_fade * ridges * 0.075
}

#[cfg(test)]
mod tests {
    use super::terrain_height;

    #[test]
    fn terrain_is_flat_at_and_beyond_the_arena_edges() {
        assert_eq!(terrain_height(-10.0, 0.0), 0.0);
        assert_eq!(terrain_height(10.0, 0.0), 0.0);
        assert_eq!(terrain_height(0.0, -10.0), 0.0);
        assert_eq!(terrain_height(0.0, 10.0), 0.0);
        assert_eq!(terrain_height(10.1, 0.0), 0.0);
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
