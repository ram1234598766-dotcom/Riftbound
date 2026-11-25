//! Bounded CPU-side planning for the native world's terrain chunk window.
//!
//! The streamer deliberately owns only chunk coordinates and LOD decisions. GPU
//! creation/destruction remains a renderer responsibility, while this module keeps
//! the active window deterministic and bounded for low-memory targets.

/// World terrain chunk width in world units.
pub const WORLD_CHUNK_SIZE: f32 = 64.0;

/// Maximum supported chunk radius, keeping the active window predictably bounded.
pub const MAX_WORLD_CHUNK_RADIUS: i32 = 8;

/// Largest absolute chunk index whose full 64-unit mesh remains inside the sampler.
const MAX_SAMPLED_CHUNK: i32 = 15_624;

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

/// Bounded differences produced by the most recent successful focus update.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorldChunkChanges {
    added: Vec<WorldChunkRequest>,
    removed: Vec<WorldChunkCoord>,
    updated: Vec<WorldChunkRequest>,
}

impl WorldChunkChanges {
    /// Chunks that entered the active window.
    #[must_use]
    pub fn added(&self) -> &[WorldChunkRequest] {
        &self.added
    }

    /// Chunk coordinates that left the active window.
    #[must_use]
    pub fn removed(&self) -> &[WorldChunkCoord] {
        &self.removed
    }

    /// Existing chunks whose requested LOD changed.
    #[must_use]
    pub fn updated(&self) -> &[WorldChunkRequest] {
        &self.updated
    }

    /// Returns true when no GPU-side chunk changes are required.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.updated.is_empty()
    }
}

/// Stable renderer-slot assignment for a bounded set of active world chunks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldChunkSlot {
    /// Reusable zero-based slot in the renderer's fixed mesh registry.
    pub slot_index: usize,
    /// Chunk coordinate and detail tier currently occupying this slot.
    pub request: WorldChunkRequest,
}

/// Fixed-capacity mapping from active chunk coordinates to reusable renderer slots.
///
/// Removed chunks release their slots, LOD changes retain the existing slot, and
/// newly-added chunks reuse the first free slot. Applying a malformed or oversized
/// change set fails without partially mutating the mapping.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldChunkSlotMap {
    slots: Vec<Option<WorldChunkRequest>>,
}

impl WorldChunkSlotMap {
    /// Creates an empty slot map with the requested fixed capacity.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            slots: vec![None; capacity],
        }
    }

    /// Creates a slot map sized for a validated chunk window.
    #[must_use]
    pub fn for_config(config: WorldChunkConfig) -> Self {
        let config =
            WorldChunkConfig::new(config.detailed_radius, config.active_radius).unwrap_or_default();
        Self::new(config.max_active_chunks())
    }

    /// Returns all slots, including empty entries available for reuse.
    #[must_use]
    pub fn slots(&self) -> &[Option<WorldChunkRequest>] {
        &self.slots
    }

    /// Returns the number of occupied slots.
    #[must_use]
    pub fn occupied(&self) -> usize {
        self.slots.iter().filter(|slot| slot.is_some()).count()
    }

    /// Returns the stable renderer slot currently assigned to a chunk coordinate.
    #[must_use]
    pub fn slot_for(&self, coord: WorldChunkCoord) -> Option<WorldChunkSlot> {
        self.slots
            .iter()
            .enumerate()
            .find_map(|(slot_index, request)| {
                request
                    .filter(|request| request.coord == coord)
                    .map(|request| WorldChunkSlot {
                        slot_index,
                        request,
                    })
            })
    }

    /// Applies one streamer's delta while retaining slots for unchanged coordinates.
    ///
    /// Returns false and leaves the map unchanged if the delta is inconsistent or
    /// cannot fit in the fixed slot capacity.
    pub fn apply_changes(&mut self, changes: &WorldChunkChanges) -> bool {
        for (index, removed) in changes.removed.iter().enumerate() {
            if !self
                .slots
                .iter()
                .any(|slot| slot.is_some_and(|request| request.coord == *removed))
                || changes.removed[..index].contains(removed)
            {
                return false;
            }
        }
        for (index, updated) in changes.updated.iter().enumerate() {
            if !self.slots.iter().any(|slot| {
                slot.is_some_and(|request| {
                    request.coord == updated.coord && !changes.removed.contains(&request.coord)
                })
            }) || changes.updated[..index]
                .iter()
                .any(|request| request.coord == updated.coord)
            {
                return false;
            }
        }
        for (index, added) in changes.added.iter().enumerate() {
            if self
                .slots
                .iter()
                .flatten()
                .any(|request| request.coord == added.coord)
                && !changes.removed.contains(&added.coord)
                || changes.added[..index]
                    .iter()
                    .any(|request| request.coord == added.coord)
            {
                return false;
            }
        }

        let free_after_removals =
            self.slots.iter().filter(|slot| slot.is_none()).count() + changes.removed.len();
        if changes.added.len() > free_after_removals {
            return false;
        }

        for removed in &changes.removed {
            let Some(slot) = self
                .slots
                .iter_mut()
                .find(|slot| slot.is_some_and(|request| request.coord == *removed))
            else {
                return false;
            };
            *slot = None;
        }
        for updated in &changes.updated {
            let Some(slot) = self
                .slots
                .iter_mut()
                .find(|slot| slot.is_some_and(|request| request.coord == updated.coord))
            else {
                return false;
            };
            *slot = Some(*updated);
        }
        for added in &changes.added {
            let Some(slot) = self.slots.iter_mut().find(|slot| slot.is_none()) else {
                return false;
            };
            *slot = Some(*added);
        }
        true
    }
}

/// CPU-side active chunk window with retained capacity and incremental change tracking.
///
/// Call `update_focus` when the player or vehicle moves. If the focus remains in
/// the same chunk, the method returns false and leaves the active window intact.
/// All active and change vectors reserve their maximum capacity at construction.
pub struct WorldChunkStreamer {
    config: WorldChunkConfig,
    focus: Option<WorldChunkCoord>,
    active: Vec<WorldChunkRequest>,
    previous: Vec<WorldChunkRequest>,
    changes: WorldChunkChanges,
}

impl WorldChunkStreamer {
    /// Creates a streamer with a bounded, pre-reserved active list.
    #[must_use]
    pub fn new(config: WorldChunkConfig) -> Self {
        let config =
            WorldChunkConfig::new(config.detailed_radius, config.active_radius).unwrap_or_default();
        let capacity = config.max_active_chunks();
        Self {
            config,
            focus: None,
            active: Vec::with_capacity(capacity),
            previous: Vec::with_capacity(capacity),
            changes: WorldChunkChanges {
                added: Vec::with_capacity(capacity),
                removed: Vec::with_capacity(capacity),
                updated: Vec::with_capacity(capacity),
            },
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

    /// Returns the incremental chunk changes from the last successful update.
    #[must_use]
    pub const fn changes(&self) -> &WorldChunkChanges {
        &self.changes
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
        let max_chunk = f32::from(
            i16::try_from(MAX_SAMPLED_CHUNK - self.config.active_radius).unwrap_or_default(),
        );
        if chunk_x < -max_chunk
            || chunk_x > max_chunk
            || chunk_z < -max_chunk
            || chunk_z > max_chunk
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

        self.previous.clear();
        self.previous.extend_from_slice(&self.active);
        self.changes.added.clear();
        self.changes.removed.clear();
        self.changes.updated.clear();
        self.focus = Some(focus);
        self.active.clear();
        let radius = self.config.active_radius;
        for z_offset in -radius..=radius {
            for x_offset in -radius..=radius {
                let distance = x_offset.abs().max(z_offset.abs());
                let coord = WorldChunkCoord {
                    x: focus.x + x_offset,
                    z: focus.z + z_offset,
                };
                // Keep a previously detailed chunk for one extra ring while it
                // moves away from the focus. Newly approaching chunks still
                // become detailed at the configured radius, avoiding rapid
                // detail changes as the player crosses chunk boundaries.
                let was_detailed = self.previous.iter().any(|previous| {
                    previous.coord == coord && previous.lod == WorldChunkLod::Detailed
                });
                let detail_exit_radius =
                    (self.config.detailed_radius + 1).min(self.config.active_radius);
                let detailed = distance <= self.config.detailed_radius
                    || (was_detailed && distance <= detail_exit_radius);
                let lod = if detailed {
                    WorldChunkLod::Detailed
                } else {
                    WorldChunkLod::Distant
                };
                self.active.push(WorldChunkRequest {
                    coord,
                    lod,
                    distance_squared: u32::try_from(distance * distance).unwrap_or_default(),
                });
            }
        }
        self.active.sort_unstable_by_key(|request| {
            (request.distance_squared, request.coord.z, request.coord.x)
        });
        for request in &self.active {
            if let Some(previous) = self.previous.iter().find(|old| old.coord == request.coord) {
                if previous.lod != request.lod {
                    self.changes.updated.push(*request);
                }
            } else {
                self.changes.added.push(*request);
            }
        }
        for previous in &self.previous {
            if !self
                .active
                .iter()
                .any(|request| request.coord == previous.coord)
            {
                self.changes.removed.push(previous.coord);
            }
        }
        true
    }
}

/// One CPU-generated terrain vertex in absolute world coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldTerrainVertex {
    /// Absolute world-space position (X, elevation, Z).
    pub position: [f32; 3],
    /// Smooth, world-space terrain normal shared by neighbouring chunks.
    pub normal: [f32; 3],
    /// Surface material sampled at this vertex.
    pub surface: crate::VehicleSurface,
}

/// CPU mesh payload for one streamed terrain chunk.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldTerrainMeshData {
    /// Number of vertices along each grid edge.
    pub resolution: usize,
    /// Row-major terrain vertices.
    pub vertices: Vec<WorldTerrainVertex>,
    /// Triangle-list indices with consistent upward winding.
    pub indices: Vec<u32>,
}

/// Builds a deterministic terrain grid for a chunk request.
///
/// Vertex positions are sampled in absolute coordinates. Normals use a fixed
/// world-space finite-difference interval, so shared chunk-edge vertices produce
/// identical results regardless of which chunk generated them.
#[must_use]
pub fn build_world_chunk_mesh(request: WorldChunkRequest) -> Option<WorldTerrainMeshData> {
    const NORMAL_STEP: f32 = 1.0;
    let resolution = match request.lod {
        WorldChunkLod::Detailed => 9,
        WorldChunkLod::Distant => 5,
    };
    let chunk_x = f32::from(i16::try_from(request.coord.x).ok()?);
    let chunk_z = f32::from(i16::try_from(request.coord.z).ok()?);
    let intervals = f32::from(u8::try_from(resolution - 1).ok()?);
    let spacing = WORLD_CHUNK_SIZE / intervals;
    let origin_x = chunk_x * WORLD_CHUNK_SIZE;
    let origin_z = chunk_z * WORLD_CHUNK_SIZE;
    let mut vertices = Vec::with_capacity(resolution * resolution);

    for row in 0..resolution {
        for column in 0..resolution {
            let x = origin_x + f32::from(u8::try_from(column).ok()?) * spacing;
            let z = origin_z + f32::from(u8::try_from(row).ok()?) * spacing;
            let center = crate::world_terrain_sample(x, z);
            let left = crate::world_terrain_sample(x - NORMAL_STEP, z).height;
            let right = crate::world_terrain_sample(x + NORMAL_STEP, z).height;
            let north = crate::world_terrain_sample(x, z - NORMAL_STEP).height;
            let south = crate::world_terrain_sample(x, z + NORMAL_STEP).height;
            let dx = (right - left) / (2.0 * NORMAL_STEP);
            let dz = (south - north) / (2.0 * NORMAL_STEP);
            let length = (dx * dx + 1.0 + dz * dz).sqrt();
            if !center.height.is_finite() || !length.is_finite() || length <= f32::EPSILON {
                return None;
            }
            vertices.push(WorldTerrainVertex {
                position: [x, center.height, z],
                normal: [-dx / length, 1.0 / length, -dz / length],
                surface: center.surface,
            });
        }
    }

    let mut indices = Vec::with_capacity((resolution - 1) * (resolution - 1) * 6);
    for row in 0..(resolution - 1) {
        for column in 0..(resolution - 1) {
            let top_left = u32::try_from(row * resolution + column).ok()?;
            let top_right = top_left + 1;
            let bottom_left = top_left + u32::try_from(resolution).ok()?;
            let bottom_right = bottom_left + 1;
            indices.extend_from_slice(&[
                top_left,
                bottom_left,
                top_right,
                top_right,
                bottom_left,
                bottom_right,
            ]);
        }
    }

    Some(WorldTerrainMeshData {
        resolution,
        vertices,
        indices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generated_chunk_mesh_has_expected_bounded_grid_and_indices() {
        let request = WorldChunkRequest {
            coord: WorldChunkCoord { x: 0, z: 0 },
            lod: WorldChunkLod::Detailed,
            distance_squared: 0,
        };
        let mesh = build_world_chunk_mesh(request).expect("valid terrain mesh");
        assert_eq!(mesh.resolution, 9);
        assert_eq!(mesh.vertices.len(), 81);
        assert_eq!(mesh.indices.len(), 8 * 8 * 6);
        assert!(mesh.vertices.iter().all(|vertex| {
            vertex.position.iter().all(|value| value.is_finite())
                && vertex.normal.iter().all(|value| value.is_finite())
                && (vertex.normal[0].powi(2) + vertex.normal[1].powi(2) + vertex.normal[2].powi(2)
                    - 1.0)
                    .abs()
                    < 1.0e-4
                && vertex.normal[1] > 0.0
        }));
    }

    #[test]
    fn adjacent_chunk_edges_match_positions_heights_and_normals() {
        let left = build_world_chunk_mesh(WorldChunkRequest {
            coord: WorldChunkCoord { x: 2, z: -3 },
            lod: WorldChunkLod::Detailed,
            distance_squared: 0,
        })
        .expect("left chunk");
        let right = build_world_chunk_mesh(WorldChunkRequest {
            coord: WorldChunkCoord { x: 3, z: -3 },
            lod: WorldChunkLod::Detailed,
            distance_squared: 1,
        })
        .expect("right chunk");
        for row in 0..left.resolution {
            let a = left.vertices[row * left.resolution + left.resolution - 1];
            let b = right.vertices[row * right.resolution];
            assert_eq!(a.position, b.position);
            assert_eq!(a.normal, b.normal);
        }
    }

    #[test]
    fn distant_and_detailed_chunks_share_aligned_boundary_samples() {
        let detailed = build_world_chunk_mesh(WorldChunkRequest {
            coord: WorldChunkCoord { x: 0, z: 0 },
            lod: WorldChunkLod::Detailed,
            distance_squared: 0,
        })
        .expect("detailed chunk");
        let distant = build_world_chunk_mesh(WorldChunkRequest {
            coord: WorldChunkCoord { x: 1, z: 0 },
            lod: WorldChunkLod::Distant,
            distance_squared: 1,
        })
        .expect("distant chunk");
        for row in 0..distant.resolution {
            let a = detailed.vertices[(row * 2) * detailed.resolution + 8];
            let b = distant.vertices[row * distant.resolution];
            assert_eq!(a.position, b.position);
            assert_eq!(a.normal, b.normal);
        }
    }

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
    fn negative_world_positions_use_floor_based_chunk_coordinates() {
        let mut streamer = WorldChunkStreamer::standard();

        assert!(streamer.update_focus(-0.01, -64.0));
        assert_eq!(
            streamer.focus_chunk(),
            Some(WorldChunkCoord { x: -1, z: -1 })
        );

        assert!(streamer.update_focus(-64.01, -64.01));
        assert_eq!(
            streamer.focus_chunk(),
            Some(WorldChunkCoord { x: -2, z: -2 })
        );
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
        let safe_chunk = f32::from(i16::try_from(MAX_SAMPLED_CHUNK - 2).unwrap_or_default());
        let max_safe = safe_chunk * WORLD_CHUNK_SIZE;
        let min_safe = -safe_chunk * WORLD_CHUNK_SIZE;
        assert!(streamer.update_focus(max_safe, min_safe));
        let chunks = streamer.active_chunks();
        assert!(chunks.iter().all(|chunk| {
            (i32::from(i16::MIN)..=i32::from(i16::MAX)).contains(&chunk.coord.x)
                && (i32::from(i16::MIN)..=i32::from(i16::MAX)).contains(&chunk.coord.z)
        }));
        let before = chunks.to_vec();
        assert!(!streamer.update_focus(
            f32::from(i16::try_from(MAX_SAMPLED_CHUNK).unwrap_or_default()) * WORLD_CHUNK_SIZE,
            0.0
        ));
        assert_eq!(streamer.active_chunks(), before);
    }

    #[test]
    fn focus_updates_report_incremental_chunk_changes() {
        let mut streamer = WorldChunkStreamer::standard();
        assert!(streamer.update_focus(0.0, 0.0));
        assert_eq!(streamer.changes().added().len(), 25);
        assert_eq!(streamer.changes().removed(), []);

        assert!(streamer.update_focus(WORLD_CHUNK_SIZE, 0.0));
        let changes = streamer.changes().clone();
        assert_eq!(changes.added().len(), 5);
        assert_eq!(changes.removed().len(), 5);
        assert_ne!(changes.updated(), []);
        assert_eq!(streamer.active_chunks().len(), 25);

        assert!(!streamer.update_focus(WORLD_CHUNK_SIZE + 1.0, 0.0));
        assert_eq!(streamer.changes(), &changes);
    }

    #[test]
    fn slot_lookup_returns_stable_slot_and_none_for_evicted_coordinates() {
        let mut streamer = WorldChunkStreamer::standard();
        let mut slots = WorldChunkSlotMap::for_config(WorldChunkConfig::default());
        assert!(streamer.update_focus(0.0, 0.0));
        assert!(slots.apply_changes(streamer.changes()));
        let origin = WorldChunkCoord { x: 0, z: 0 };
        let original = slots.slot_for(origin).expect("origin chunk is assigned");

        assert!(streamer.update_focus(WORLD_CHUNK_SIZE, 0.0));
        assert!(slots.apply_changes(streamer.changes()));
        assert_eq!(
            slots
                .slot_for(origin)
                .expect("origin remains active")
                .slot_index,
            original.slot_index
        );
        assert!(slots.slot_for(WorldChunkCoord { x: -2, z: 0 }).is_none());
    }

    #[test]
    fn slot_map_reuses_evicted_slots_and_preserves_unchanged_chunk_slots() {
        let mut streamer = WorldChunkStreamer::standard();
        let mut slots = WorldChunkSlotMap::for_config(WorldChunkConfig::default());
        assert!(streamer.update_focus(0.0, 0.0));
        assert!(slots.apply_changes(streamer.changes()));
        assert_eq!(slots.occupied(), 25);
        let retained = slots
            .slots()
            .iter()
            .enumerate()
            .find_map(|(index, request)| {
                request
                    .filter(|request| request.coord == WorldChunkCoord { x: 0, z: 0 })
                    .map(|_| index)
            })
            .expect("origin chunk has a slot");

        assert!(streamer.update_focus(WORLD_CHUNK_SIZE, 0.0));
        assert!(slots.apply_changes(streamer.changes()));
        assert_eq!(slots.occupied(), 25);
        assert_eq!(
            slots.slots()[retained]
                .expect("origin slot remains occupied")
                .coord,
            WorldChunkCoord { x: 0, z: 0 }
        );
        assert!(
            slots
                .slots()
                .iter()
                .flatten()
                .any(|request| request.coord == WorldChunkCoord { x: 3, z: 0 })
        );
    }

    #[test]
    fn detailed_lod_has_one_ring_of_exit_hysteresis() {
        let mut streamer = WorldChunkStreamer::new(WorldChunkConfig::new(0, 2).unwrap());
        assert!(streamer.update_focus(0.0, 0.0));
        let origin = WorldChunkCoord { x: 0, z: 0 };
        assert_eq!(
            streamer
                .active_chunks()
                .iter()
                .find(|request| request.coord == origin)
                .unwrap()
                .lod,
            WorldChunkLod::Detailed
        );

        // Moving one chunk away leaves the old focus chunk detailed for one
        // transition ring, while the newly focused chunk is detailed too.
        assert!(streamer.update_focus(WORLD_CHUNK_SIZE, 0.0));
        assert_eq!(
            streamer
                .active_chunks()
                .iter()
                .find(|request| request.coord == origin)
                .unwrap()
                .lod,
            WorldChunkLod::Detailed
        );

        // Once it moves beyond that ring, it returns to distant LOD.
        assert!(streamer.update_focus(WORLD_CHUNK_SIZE * 2.0, 0.0));
        assert_eq!(
            streamer
                .active_chunks()
                .iter()
                .find(|request| request.coord == origin)
                .unwrap()
                .lod,
            WorldChunkLod::Distant
        );
    }

    #[test]
    fn slot_map_keeps_renderer_slot_when_chunk_changes_lod() {
        // A zero-radius detailed core makes the one-ring exit transition
        // observable before the chunk leaves this bounded active window.
        let config = WorldChunkConfig::new(0, 2).unwrap();
        let mut streamer = WorldChunkStreamer::new(config);
        let mut slots = WorldChunkSlotMap::for_config(config);
        let origin = WorldChunkCoord { x: 0, z: 0 };

        assert!(streamer.update_focus(0.0, 0.0));
        assert!(slots.apply_changes(streamer.changes()));
        let original = slots.slot_for(origin).expect("origin chunk is assigned");
        assert_eq!(original.request.lod, WorldChunkLod::Detailed);

        assert!(streamer.update_focus(WORLD_CHUNK_SIZE * 2.0, 0.0));
        assert!(
            streamer.changes().updated().iter().any(|request| {
                request.coord == origin && request.lod == WorldChunkLod::Distant
            })
        );
        assert!(slots.apply_changes(streamer.changes()));

        let updated = slots
            .slot_for(origin)
            .expect("origin remains in active window");
        assert_eq!(updated.slot_index, original.slot_index);
        assert_eq!(updated.request.lod, WorldChunkLod::Distant);
        assert_eq!(slots.occupied(), 25);
    }

    #[test]
    fn slot_map_rejects_inconsistent_changes_without_partial_mutation() {
        let mut slots = WorldChunkSlotMap::new(1);
        let request = WorldChunkRequest {
            coord: WorldChunkCoord { x: 0, z: 0 },
            lod: WorldChunkLod::Detailed,
            distance_squared: 0,
        };
        let malformed = WorldChunkChanges {
            added: vec![request, request],
            removed: Vec::new(),
            updated: Vec::new(),
        };
        assert!(!slots.apply_changes(&malformed));
        assert_eq!(slots.occupied(), 0);
    }

    #[test]
    fn slot_map_rejects_over_capacity_delta_without_releasing_existing_slots() {
        let mut slots = WorldChunkSlotMap::new(1);
        let original = WorldChunkRequest {
            coord: WorldChunkCoord { x: 0, z: 0 },
            lod: WorldChunkLod::Detailed,
            distance_squared: 0,
        };
        let replacement = WorldChunkRequest {
            coord: WorldChunkCoord { x: 1, z: 0 },
            lod: WorldChunkLod::Detailed,
            distance_squared: 1,
        };
        let initial = WorldChunkChanges {
            added: vec![original],
            removed: Vec::new(),
            updated: Vec::new(),
        };
        assert!(slots.apply_changes(&initial));

        let oversized = WorldChunkChanges {
            added: vec![replacement],
            removed: Vec::new(),
            updated: Vec::new(),
        };
        assert!(!slots.apply_changes(&oversized));
        assert_eq!(slots.occupied(), 1);
        assert_eq!(
            slots
                .slot_for(original.coord)
                .expect("original slot is retained")
                .request,
            original
        );
        assert!(slots.slot_for(replacement.coord).is_none());
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
