//! Bounded CPU-side planning for the native world's terrain chunk window.
//!
//! The streamer deliberately owns only chunk coordinates and LOD decisions. GPU
//! creation/destruction remains a renderer responsibility, while this module keeps
//! the active window deterministic and bounded for low-memory targets.

/// World terrain chunk width in world units.
pub const WORLD_CHUNK_SIZE: f32 = 64.0;

/// Maximum supported chunk radius, keeping the active window predictably bounded.
pub const MAX_WORLD_CHUNK_RADIUS: i32 = 8;

/// Detail tier requested for a world terrain chunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldChunkLod {
    /// Full 9x9 terrain sample grid.
    Detailed,
    /// Compact 5x5 terrain sample grid.
    Distant,
}

/// Integer coordinate of a 64-by-64 world terrain chunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WorldChunkCoord {
    /// Absolute chunk coordinate on the world's X axis.
    pub x: i32,
    /// Absolute chunk coordinate on the world's Z axis.
    pub z: i32,
}

/// One chunk requested by the current streaming window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldChunkRequest {
    /// Absolute coordinate of the requested chunk.
    pub coord: WorldChunkCoord,
    /// Mesh detail tier selected for this chunk.
    pub lod: WorldChunkLod,
    /// Squared Chebyshev distance from the focus chunk.
    pub distance_squared: u32,
}

/// Validated radius settings for a bounded terrain window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldChunkConfig {
    /// Chunks within this radius use the detailed grid.
    pub detailed_radius: i32,
    /// Chunks beyond the outer radius are evicted.
    pub active_radius: i32,
}

impl Default for WorldChunkConfig {
    fn default() -> Self {
        Self {
            detailed_radius: 1,
            active_radius: 2,
        }
    }
}

impl WorldChunkConfig {
    /// Creates a configuration when both radii are within the supported bounds.
    #[must_use]
    pub const fn new(detailed_radius: i32, active_radius: i32) -> Option<Self> {
        if detailed_radius < 0
            || active_radius < detailed_radius
            || active_radius > MAX_WORLD_CHUNK_RADIUS
        {
            return None;
        }
        Some(Self {
            detailed_radius,
            active_radius,
        })
    }

    /// Maximum number of chunks in the square active window.
    #[must_use]
    #[allow(clippy::cast_sign_loss)] // Constructor guarantees a non-negative radius.
    pub const fn max_active_chunks(self) -> usize {
        let side = (self.active_radius * 2 + 1) as usize;
        side * side
    }
}

/// CPU-side active chunk window with retained capacity and center-cell updates.
///
/// Call `update_focus` when the player or vehicle moves. If the focus remains in
/// the same chunk, the method returns `false` and leaves the active window intact.
/// The vector is allocated once to its configured maximum and reused thereafter.
pub struct WorldChunkStreamer {
    config: WorldChunkConfig,
    focus: Option<WorldChunkCoord>,
    active: Vec<WorldChunkRequest>,
}

impl WorldChunkStreamer {
    /// Creates a streamer with a bounded, pre-reserved active list.
    #[must_use]
    pub fn new(config: WorldChunkConfig) -> Self {
        let config =
            WorldChunkConfig::new(config.detailed_radius, config.active_radius).unwrap_or_default();
        let mut active = Vec::with_capacity(config.max_active_chunks());
        active.clear();
        Self {
            config,
            focus: None,
            active,
        }
    }

    /// Uses the default 5x5 active window (3x3 detailed, outer ring distant).
    #[must_use]
    pub fn standard() -> Self {
        Self::new(WorldChunkConfig::default())
    }

    /// Returns the currently active chunk requests in deterministic near-first order.
    #[must_use]
    pub fn active_chunks(&self) -> &[WorldChunkRequest] {
        &self.active
    }

    /// Returns the current focus chunk, if a valid focus has been supplied.
    #[must_use]
    pub const fn focus_chunk(&self) -> Option<WorldChunkCoord> {
        self.focus
    }

    /// Rebuilds the bounded window when the focus crosses a chunk boundary.
    ///
    /// Non-finite and out-of-range positions are rejected without changing the
    /// previous window. Returns `true` only when the active window was rebuilt.
    #[allow(clippy::cast_possible_truncation)] // Range-checked to i16 before conversion.
    pub fn update_focus(&mut self, world_x: f32, world_z: f32) -> bool {
        if !world_x.is_finite() || !world_z.is_finite() {
            return false;
        }
        let chunk_x = (world_x / WORLD_CHUNK_SIZE).floor();
        let chunk_z = (world_z / WORLD_CHUNK_SIZE).floor();
        let margin = match self.config.active_radius {
            0 => 0.0,
            1 => 1.0,
            2 => 2.0,
            3 => 3.0,
            4 => 4.0,
            5 => 5.0,
            6 => 6.0,
            7 => 7.0,
            8 => 8.0,
            _ => return false,
        };
        if chunk_x < f32::from(i16::MIN) + margin
            || chunk_x > f32::from(i16::MAX) - margin
            || chunk_z < f32::from(i16::MIN) + margin
            || chunk_z > f32::from(i16::MAX) - margin
        {
            return false;
        }
        let focus = WorldChunkCoord {
            x: chunk_x as i32,
            z: chunk_z as i32,
        };
        if self.focus == Some(focus) {
            return false;
        }

        self.focus = Some(focus);
        self.active.clear();
        let radius = self.config.active_radius;
        for z_offset in -radius..=radius {
            for x_offset in -radius..=radius {
                let distance = x_offset.abs().max(z_offset.abs());
                let lod = if distance <= self.config.detailed_radius {
                    WorldChunkLod::Detailed
                } else {
                    WorldChunkLod::Distant
                };
                self.active.push(WorldChunkRequest {
                    coord: WorldChunkCoord {
                        x: focus.x + x_offset,
                        z: focus.z + z_offset,
                    },
                    lod,
                    distance_squared: u32::try_from(distance * distance).unwrap_or_default(),
                });
            }
        }
        self.active.sort_unstable_by_key(|request| {
            (request.distance_squared, request.coord.z, request.coord.x)
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_window_is_bounded_and_uses_two_detail_tiers() {
        let mut streamer = WorldChunkStreamer::standard();
        assert!(streamer.update_focus(0.0, 0.0));
        let chunks = streamer.active_chunks();
        assert_eq!(chunks.len(), 25);
        assert_eq!(
            chunks
                .iter()
                .filter(|chunk| chunk.lod == WorldChunkLod::Detailed)
                .count(),
            9
        );
        assert_eq!(
            chunks
                .iter()
                .filter(|chunk| chunk.lod == WorldChunkLod::Distant)
                .count(),
            16
        );
        assert_eq!(chunks[0].coord, WorldChunkCoord { x: 0, z: 0 });
    }

    #[test]
    fn window_rebuilds_only_after_crossing_chunk_boundary() {
        let mut streamer = WorldChunkStreamer::standard();
        assert!(streamer.update_focus(1.0, 1.0));
        assert!(!streamer.update_focus(63.9, 12.0));
        assert!(streamer.update_focus(64.0, 12.0));
        assert_eq!(streamer.focus_chunk(), Some(WorldChunkCoord { x: 1, z: 0 }));
    }

    #[test]
    fn invalid_focus_does_not_destroy_previous_window() {
        let mut streamer = WorldChunkStreamer::standard();
        assert!(streamer.update_focus(0.0, 0.0));
        let before = streamer.active_chunks().to_vec();
        assert!(!streamer.update_focus(f32::NAN, 0.0));
        assert!(!streamer.update_focus(2_000_000.0, 0.0));
        assert_eq!(streamer.active_chunks(), before);
    }

    #[test]
    fn focus_reserves_room_for_the_entire_window_at_sampler_limits() {
        let mut streamer = WorldChunkStreamer::standard();
        let max_safe = f32::from(i16::MAX - 2) * WORLD_CHUNK_SIZE;
        let min_safe = f32::from(i16::MIN + 2) * WORLD_CHUNK_SIZE;
        assert!(streamer.update_focus(max_safe, min_safe));
        let chunks = streamer.active_chunks();
        assert!(chunks.iter().all(|chunk| {
            (i32::from(i16::MIN)..=i32::from(i16::MAX)).contains(&chunk.coord.x)
                && (i32::from(i16::MIN)..=i32::from(i16::MAX)).contains(&chunk.coord.z)
        }));
        let before = chunks.to_vec();
        assert!(!streamer.update_focus(f32::from(i16::MAX - 1) * WORLD_CHUNK_SIZE, 0.0));
        assert_eq!(streamer.active_chunks(), before);
    }

    #[test]
    fn streamer_sanitizes_directly_constructed_invalid_configuration() {
        let mut streamer = WorldChunkStreamer::new(WorldChunkConfig {
            detailed_radius: 4,
            active_radius: 99,
        });
        assert!(streamer.update_focus(0.0, 0.0));
        assert_eq!(streamer.active_chunks().len(), 25);
    }

    #[test]
    fn configuration_rejects_unbounded_or_inverted_radii() {
        assert!(WorldChunkConfig::new(2, 1).is_none());
        assert!(WorldChunkConfig::new(0, MAX_WORLD_CHUNK_RADIUS + 1).is_none());
        assert!(WorldChunkConfig::new(-1, 2).is_none());
        assert_eq!(WorldChunkConfig::default().max_active_chunks(), 25);
    }
}
