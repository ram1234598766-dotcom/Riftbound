//! CPU mesh descriptions and their GPU vertex/index-buffer representation.

use riftbound_core::{
    VehicleSurface, terrain_height, terrain_surface, world_road_sample, world_terrain_sample,
};
use wgpu::util::DeviceExt;

const WORLD_TERRAIN_SKIRT_DEPTH: f32 = 12.0;

/// One position-and-color vertex in the renderer's standard mesh format.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshVertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
}

impl MeshVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];

    /// Describes the vertex layout consumed by the standard mesh pipeline.
    #[must_use]
    pub const fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: 24,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// GPU-resident indexed triangle mesh.
pub struct Mesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
}

impl Mesh {
    /// Uploads vertex and 16-bit index data to GPU buffers.
    ///
    /// Returns None for empty data, out-of-range indices, or counts that cannot fit a draw call.
    #[must_use]
    pub fn from_data(
        device: &wgpu::Device,
        vertices: &[MeshVertex],
        indices: &[u16],
        label: &str,
    ) -> Option<Self> {
        if vertices.is_empty() || !valid_index_data(vertices.len(), indices) {
            return None;
        }
        let index_count = u32::try_from(indices.len()).ok()?;
        let mut vertex_bytes = Vec::with_capacity(vertices.len() * 24);
        for vertex in vertices {
            for component in vertex.position.iter().chain(vertex.color.iter()) {
                vertex_bytes.extend_from_slice(&component.to_ne_bytes());
            }
        }
        let index_bytes: Vec<u8> = indices
            .iter()
            .flat_map(|index| index.to_ne_bytes())
            .collect();
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: &vertex_bytes,
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("riftbound-mesh-indices"),
            contents: &index_bytes,
            usage: wgpu::BufferUsages::INDEX,
        });
        Some(Self {
            vertex_buffer,
            index_buffer,
            index_count,
        })
    }

    /// Creates the renderer's starter cube with shared face-corner vertices.
    #[must_use]
    pub fn cube(device: &wgpu::Device) -> Option<Self> {
        let (vertices, indices) = cube_data();
        Self::from_data(device, &vertices, &indices, "riftbound-cube-mesh")
    }

    /// Creates a compact cube LOD with shared corner vertices.
    #[must_use]
    pub fn cube_lod(device: &wgpu::Device) -> Option<Self> {
        let (vertices, indices) = cube_lod_data();
        Self::from_data(device, &vertices, &indices, "riftbound-cube-lod-mesh")
    }

    /// Creates a low-poly pyramid mesh with a square base and four triangular sides.
    #[must_use]
    pub fn pyramid(device: &wgpu::Device) -> Option<Self> {
        let (vertices, indices) = pyramid_data();
        Self::from_data(device, &vertices, &indices, "riftbound-pyramid-mesh")
    }

    /// Creates a faceted octahedral crystal mesh with alternating cyan facets.
    #[must_use]
    pub fn crystal(device: &wgpu::Device) -> Option<Self> {
        let (vertices, indices) = crystal_data();
        Self::from_data(device, &vertices, &indices, "riftbound-crystal-mesh")
    }

    /// Creates one of four detailed terrain quadrants sampled in shared world space.
    #[must_use]
    pub fn terrain_chunk(device: &wgpu::Device, chunk_x: u8, chunk_z: u8) -> Option<Self> {
        let (vertices, indices) = terrain_chunk_data(chunk_x, chunk_z, 9)?;
        Self::from_data(device, &vertices, &indices, "riftbound-terrain-chunk-mesh")
    }

    /// Creates a simplified terrain quadrant with matching outer-edge samples.
    #[must_use]
    pub fn terrain_chunk_lod(device: &wgpu::Device, chunk_x: u8, chunk_z: u8) -> Option<Self> {
        let (vertices, indices) = terrain_chunk_data(chunk_x, chunk_z, 5)?;
        Self::from_data(
            device,
            &vertices,
            &indices,
            "riftbound-terrain-chunk-lod-mesh",
        )
    }

    /// Creates a detailed world-scale chunk from deterministic absolute coordinates.
    ///
    /// Chunks are 64 world units wide. Adjacent chunks sample identical border coordinates.
    #[must_use]
    pub fn world_terrain_chunk(device: &wgpu::Device, chunk_x: i32, chunk_z: i32) -> Option<Self> {
        let (vertices, indices) = world_terrain_chunk_data(chunk_x, chunk_z, 9)?;
        Self::from_data(
            device,
            &vertices,
            &indices,
            "riftbound-world-terrain-chunk-mesh",
        )
    }

    /// Creates a compact 5x5 world chunk for distant terrain.
    #[must_use]
    pub fn world_terrain_chunk_lod(
        device: &wgpu::Device,
        chunk_x: i32,
        chunk_z: i32,
    ) -> Option<Self> {
        let (vertices, indices) = world_terrain_chunk_data(chunk_x, chunk_z, 5)?;
        Self::from_data(
            device,
            &vertices,
            &indices,
            "riftbound-world-terrain-chunk-lod-mesh",
        )
    }

    /// Returns the GPU vertex buffer.
    #[must_use]
    pub const fn vertex_buffer(&self) -> &wgpu::Buffer {
        &self.vertex_buffer
    }

    /// Returns the GPU index buffer.
    #[must_use]
    pub const fn index_buffer(&self) -> &wgpu::Buffer {
        &self.index_buffer
    }

    /// Returns the number of indices in this mesh.
    #[must_use]
    pub const fn index_count(&self) -> u32 {
        self.index_count
    }
}

fn valid_index_data(vertex_count: usize, indices: &[u16]) -> bool {
    !indices.is_empty()
        && indices.len().is_multiple_of(3)
        && indices
            .iter()
            .all(|index| usize::from(*index) < vertex_count)
}

fn cube_lod_data() -> (Vec<MeshVertex>, Vec<u16>) {
    let color = [0.12, 0.48, 0.82];
    let vertices = vec![
        MeshVertex {
            position: [-1.0, -1.0, -1.0],
            color,
        },
        MeshVertex {
            position: [1.0, -1.0, -1.0],
            color,
        },
        MeshVertex {
            position: [1.0, 1.0, -1.0],
            color,
        },
        MeshVertex {
            position: [-1.0, 1.0, -1.0],
            color,
        },
        MeshVertex {
            position: [-1.0, -1.0, 1.0],
            color,
        },
        MeshVertex {
            position: [1.0, -1.0, 1.0],
            color,
        },
        MeshVertex {
            position: [1.0, 1.0, 1.0],
            color,
        },
        MeshVertex {
            position: [-1.0, 1.0, 1.0],
            color,
        },
    ];
    let indices = vec![
        4, 5, 6, 4, 6, 7, 1, 0, 3, 1, 3, 2, 5, 1, 2, 5, 2, 6, 0, 4, 7, 0, 7, 3, 7, 6, 2, 7, 2, 3,
        0, 1, 5, 0, 5, 4,
    ];
    (vertices, indices)
}

fn pyramid_data() -> (Vec<MeshVertex>, Vec<u16>) {
    let vertices = vec![
        MeshVertex {
            position: [-1.0, -1.0, -1.0],
            color: [0.08, 0.30, 0.82],
        },
        MeshVertex {
            position: [1.0, -1.0, -1.0],
            color: [0.08, 0.30, 0.82],
        },
        MeshVertex {
            position: [1.0, -1.0, 1.0],
            color: [0.08, 0.30, 0.82],
        },
        MeshVertex {
            position: [-1.0, -1.0, 1.0],
            color: [0.08, 0.30, 0.82],
        },
        MeshVertex {
            position: [0.0, 1.0, 0.0],
            color: [0.62, 0.22, 1.0],
        },
    ];
    let indices = vec![0, 2, 1, 0, 3, 2, 0, 1, 4, 1, 2, 4, 2, 3, 4, 3, 0, 4];
    (vertices, indices)
}

fn crystal_data() -> (Vec<MeshVertex>, Vec<u16>) {
    let vertices = vec![
        MeshVertex {
            position: [0.0, 1.0, 0.0],
            color: [0.18, 0.92, 1.0],
        },
        MeshVertex {
            position: [0.0, -1.0, 0.0],
            color: [0.08, 0.35, 0.88],
        },
        MeshVertex {
            position: [1.0, 0.0, 0.0],
            color: [0.22, 0.72, 1.0],
        },
        MeshVertex {
            position: [0.0, 0.0, 1.0],
            color: [0.12, 0.58, 0.96],
        },
        MeshVertex {
            position: [-1.0, 0.0, 0.0],
            color: [0.32, 0.86, 1.0],
        },
        MeshVertex {
            position: [0.0, 0.0, -1.0],
            color: [0.10, 0.48, 0.90],
        },
    ];
    let indices = vec![
        0, 2, 3, 0, 3, 4, 0, 4, 5, 0, 5, 2, 1, 3, 2, 1, 4, 3, 1, 5, 4, 1, 2, 5,
    ];
    (vertices, indices)
}

fn terrain_surface_color(surface: VehicleSurface) -> [f32; 3] {
    match surface {
        VehicleSurface::Asphalt => [0.20, 0.23, 0.28],
        VehicleSurface::Gravel => [0.42, 0.35, 0.25],
        VehicleSurface::Mud => [0.24, 0.15, 0.10],
        VehicleSurface::Sand => [0.62, 0.48, 0.28],
        VehicleSurface::Snow => [0.66, 0.76, 0.86],
        VehicleSurface::Rock => [0.34, 0.38, 0.43],
    }
}

fn terrain_chunk_data(
    chunk_x: u8,
    chunk_z: u8,
    side: usize,
) -> Option<(Vec<MeshVertex>, Vec<u16>)> {
    if !(2..=9).contains(&side) || (8 % (side - 1)) != 0 {
        return None;
    }
    let cells_per_chunk = side - 1;
    let chunk_x = usize::from(chunk_x);
    let chunk_z = usize::from(chunk_z);
    if chunk_x >= 2 || chunk_z >= 2 {
        return None;
    }
    let mut vertices = Vec::with_capacity(side * side);
    let mut indices = Vec::with_capacity(cells_per_chunk * cells_per_chunk * 6);
    for z in 0..side {
        for x in 0..side {
            let global_x = chunk_x * 8 + x * (8 / cells_per_chunk);
            let global_z = chunk_z * 8 + z * (8 / cells_per_chunk);
            let world_x = -10.0
                + f32::from(u8::try_from(global_x).expect("terrain x coordinate fits u8")) * 1.25;
            let world_z = -10.0
                + f32::from(u8::try_from(global_z).expect("terrain z coordinate fits u8")) * 1.25;
            let local_x = f32::from(u8::try_from(x).expect("terrain x fits u8")) / 4.0 - 1.0;
            let local_z = f32::from(u8::try_from(z).expect("terrain z fits u8")) / 4.0 - 1.0;
            let variation = f32::from(
                u8::try_from((global_x * 13 + global_z * 7) % 5)
                    .expect("terrain variation fits u8"),
            ) * 0.012;
            let height = terrain_height(world_x, world_z);
            let elevation_tint = height.max(0.0) * 0.22;
            let base_color = terrain_surface_color(terrain_surface(world_x, world_z));
            vertices.push(MeshVertex {
                position: [local_x, height, local_z],
                color: [
                    (base_color[0] + variation + elevation_tint).min(1.0),
                    (base_color[1] + variation + elevation_tint).min(1.0),
                    (base_color[2] + variation * 1.5 + elevation_tint).min(1.0),
                ],
            });
        }
    }
    for z in 0..cells_per_chunk {
        for x in 0..cells_per_chunk {
            let top_left = u16::try_from(z * side + x).expect("terrain vertex index fits u16");
            let top_right = top_left + 1;
            let bottom_left = top_left + u16::try_from(side).expect("terrain side fits u16");
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
    Some((vertices, indices))
}

/// Generates a world chunk in local mesh space from stable absolute terrain samples.
fn world_terrain_chunk_data(
    chunk_x: i32,
    chunk_z: i32,
    side: usize,
) -> Option<(Vec<MeshVertex>, Vec<u16>)> {
    if ![5, 9].contains(&side) {
        return None;
    }
    // Keep world coordinates inside the sampler's documented precision range.
    let chunk_x = i16::try_from(chunk_x).ok()?;
    let chunk_z = i16::try_from(chunk_z).ok()?;
    let origin_x = f32::from(chunk_x) * 64.0;
    let origin_z = f32::from(chunk_z) * 64.0;
    let cells = side - 1;
    let step = 64.0 / f32::from(u8::try_from(cells).ok()?);
    let mut vertices = Vec::with_capacity(side * side);
    let mut indices = Vec::with_capacity(cells * cells * 6);

    for z in 0..side {
        for x in 0..side {
            let local_x = -32.0 + f32::from(u8::try_from(x).ok()?) * step;
            let local_z = -32.0 + f32::from(u8::try_from(z).ok()?) * step;
            let sample = world_terrain_sample(origin_x + local_x, origin_z + local_z);
            let base = terrain_surface_color(sample.surface);
            let elevation_tint = sample.height.max(0.0) * 0.012;
            vertices.push(MeshVertex {
                position: [local_x, sample.height, local_z],
                color: [
                    (base[0] + elevation_tint).min(1.0),
                    (base[1] + elevation_tint).min(1.0),
                    (base[2] + elevation_tint).min(1.0),
                ],
            });
        }
    }

    for z in 0..cells {
        for x in 0..cells {
            let top_left = u16::try_from(z * side + x).ok()?;
            let top_right = top_left + 1;
            let bottom_left = top_left + u16::try_from(side).ok()?;
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

    // Add the road ribbon only to chunks that fully contain its corridor.
    // This avoids drawing duplicate, translated copies in every streamed X column.
    let road_is_inside_chunk = (0..side).all(|z| {
        let local_z =
            -32.0 + f32::from(u8::try_from(z).expect("road row fits u8")) * step;
        let road = world_road_sample(origin_x, origin_z + local_z);
        let left = road.center_x - road.half_width - road.shoulder_width;
        let right = road.center_x + road.half_width + road.shoulder_width;
        left >= origin_x - 32.0 && right <= origin_x + 32.0
    });
    if road_is_inside_chunk {
        let road_start = u16::try_from(vertices.len()).ok()?;
        for z in 0..side {
            let local_z = -32.0 + f32::from(u8::try_from(z).ok()?) * step;
            let world_z = origin_z + local_z;
            let road = world_road_sample(origin_x, world_z);
            let edge_offsets = [
                -road.half_width - road.shoulder_width,
                -road.half_width,
                road.half_width,
                road.half_width + road.shoulder_width,
            ];
            for (edge_index, offset) in edge_offsets.into_iter().enumerate() {
                let world_x = road.center_x + offset;
                let local_x = world_x - origin_x;
                let sample = world_terrain_sample(world_x, world_z);
                let color = if edge_index == 0 || edge_index == 3 {
                    terrain_surface_color(VehicleSurface::Gravel)
                } else {
                    [0.12, 0.14, 0.17]
                };
                vertices.push(MeshVertex {
                    position: [local_x, sample.height + 0.2, local_z],
                    color,
                });
            }
        }
        for z in 0..cells {
            for band in 0..3 {
                let top_left = road_start
                    + u16::try_from(z * 4 + band).expect("road vertex index fits u16");
                let top_right = top_left + 1;
                let bottom_left = top_left + 4;
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

        // Dash spacing is fixed in world units, not tied to terrain LOD, so
        // markings line up when adjacent chunks use different mesh densities.
        const DASH_COUNT: usize = 8;
        const DASH_LENGTH: f32 = 4.0;
        const DASH_STEP: f32 = 8.0;
        const LINE_HALF_WIDTH: f32 = 0.12;
        for dash in 0..DASH_COUNT {
            let start_z = -32.0 + f32::from(u8::try_from(dash).ok()?) * DASH_STEP;
            let end_z = start_z + DASH_LENGTH;
            let start_world_z = origin_z + start_z;
            let end_world_z = origin_z + end_z;
            let start_road = world_road_sample(origin_x, start_world_z);
            let end_road = world_road_sample(origin_x, end_world_z);
            let start_height =
                world_terrain_sample(start_road.center_x, start_world_z).height + 0.24;
            let end_height =
                world_terrain_sample(end_road.center_x, end_world_z).height + 0.24;
            let line_start = u16::try_from(vertices.len()).ok()?;
            let line_color = [0.95, 0.78, 0.28];
            vertices.extend_from_slice(&[
                MeshVertex {
                    position: [
                        start_road.center_x - LINE_HALF_WIDTH - origin_x,
                        start_height,
                        start_z,
                    ],
                    color: line_color,
                },
                MeshVertex {
                    position: [
                        start_road.center_x + LINE_HALF_WIDTH - origin_x,
                        start_height,
                        start_z,
                    ],
                    color: line_color,
                },
                MeshVertex {
                    position: [
                        end_road.center_x - LINE_HALF_WIDTH - origin_x,
                        end_height,
                        end_z,
                    ],
                    color: line_color,
                },
                MeshVertex {
                    position: [
                        end_road.center_x + LINE_HALF_WIDTH - origin_x,
                        end_height,
                        end_z,
                    ],
                    color: line_color,
                },
            ]);
            indices.extend_from_slice(&[
                line_start,
                line_start + 2,
                line_start + 1,
                line_start + 1,
                line_start + 2,
                line_start + 3,
            ]);
        }
    }

    // The eastbound spur spans three streamed chunks. Generate only the
    // stations owned by this chunk; shared boundary stations keep edges aligned.
    if chunk_z == 0 {
        let chunk_min_x = origin_x - 32.0;
        let chunk_max_x = origin_x + 32.0;
        let spur_min_x = chunk_min_x.max(0.0);
        let spur_max_x = chunk_max_x.min(160.0);
        if spur_min_x <= spur_max_x {
            let mut stations = Vec::with_capacity(10);
            stations.push(spur_min_x);
            for station in 1..20 {
                let world_x = f32::from(u8::try_from(station).ok()?) * 8.0;
                if world_x > spur_min_x && world_x < spur_max_x {
                    stations.push(world_x);
                }
            }
            if spur_max_x > spur_min_x {
                stations.push(spur_max_x);
            }
            let spur_start = u16::try_from(vertices.len()).ok()?;
            for world_x in &stations {
                for (band, offset) in [-8.0_f32, -5.5, 5.5, 8.0].into_iter().enumerate() {
                    let world_z = offset;
                    let sample = world_terrain_sample(*world_x, world_z);
                    let color = if band == 0 || band == 3 {
                        terrain_surface_color(VehicleSurface::Gravel)
                    } else {
                        [0.12, 0.14, 0.17]
                    };
                    vertices.push(MeshVertex {
                        position: [*world_x - origin_x, sample.height + 0.23, world_z - origin_z],
                        color,
                    });
                }
            }
            for station in 0..stations.len().saturating_sub(1) {
                for band in 0..3 {
                    let top_left = spur_start
                        + u16::try_from(station * 4 + band).expect("spur vertex index fits u16");
                    let top_right = top_left + 1;
                    let bottom_left = top_left + 4;
                    let bottom_right = bottom_left + 1;
                    indices.extend_from_slice(&[
                        top_left,
                        top_right,
                        bottom_left,
                        top_right,
                        bottom_right,
                        bottom_left,
                    ]);
                }
            }
        }
    }

    // A short vertical skirt hides cracks where a 9x9 chunk meets a 5x5 LOD
    // chunk. Each edge is generated from that chunk's own border samples, so
    // the overlap remains valid even when the two edge profiles differ.
    let mut append_skirt = |edge: Vec<usize>| -> Option<()> {
        for pair in edge.windows(2) {
            let top_a = *vertices.get(pair[0])?;
            let top_b = *vertices.get(pair[1])?;
            let base = u16::try_from(vertices.len()).ok()?;
            let mut bottom_a = top_a;
            bottom_a.position[1] -= WORLD_TERRAIN_SKIRT_DEPTH;
            let mut bottom_b = top_b;
            bottom_b.position[1] -= WORLD_TERRAIN_SKIRT_DEPTH;
            vertices.extend_from_slice(&[top_a, bottom_a, top_b, bottom_b]);
            indices.extend_from_slice(&[base, base + 1, base + 2, base + 2, base + 1, base + 3]);
        }
        Some(())
    };
    append_skirt((0..side).collect())?;
    append_skirt((0..side).map(|z| z * side + side - 1).collect())?;
    append_skirt(((side - 1) * side..side * side).collect())?;
    append_skirt((0..side).rev().map(|z| z * side).collect())?;
    Some((vertices, indices))
}

fn cube_data() -> (Vec<MeshVertex>, Vec<u16>) {
    let faces = [
        (
            [
                [-1.0, -1.0, 1.0],
                [1.0, -1.0, 1.0],
                [1.0, 1.0, 1.0],
                [-1.0, 1.0, 1.0],
            ],
            [0.08, 0.78, 1.0],
        ),
        (
            [
                [1.0, -1.0, -1.0],
                [-1.0, -1.0, -1.0],
                [-1.0, 1.0, -1.0],
                [1.0, 1.0, -1.0],
            ],
            [0.12, 0.30, 0.82],
        ),
        (
            [
                [1.0, -1.0, 1.0],
                [1.0, -1.0, -1.0],
                [1.0, 1.0, -1.0],
                [1.0, 1.0, 1.0],
            ],
            [0.55, 0.20, 0.95],
        ),
        (
            [
                [-1.0, -1.0, -1.0],
                [-1.0, -1.0, 1.0],
                [-1.0, 1.0, 1.0],
                [-1.0, 1.0, -1.0],
            ],
            [0.12, 0.88, 0.72],
        ),
        (
            [
                [-1.0, 1.0, 1.0],
                [1.0, 1.0, 1.0],
                [1.0, 1.0, -1.0],
                [-1.0, 1.0, -1.0],
            ],
            [0.16, 0.48, 0.92],
        ),
        (
            [
                [-1.0, -1.0, -1.0],
                [1.0, -1.0, -1.0],
                [1.0, -1.0, 1.0],
                [-1.0, -1.0, 1.0],
            ],
            [0.06, 0.20, 0.52],
        ),
    ];

    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (face_index, (corners, color)) in faces.into_iter().enumerate() {
        for position in corners {
            vertices.push(MeshVertex { position, color });
        }
        let base = u16::try_from(face_index * 4).expect("cube vertex offset fits u16");
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    (vertices, indices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_lod_preserves_cube_topology_with_fewer_vertices() {
        let (vertices, indices) = cube_lod_data();
        assert_eq!(vertices.len(), 8);
        assert_eq!(indices.len(), 36);
        assert!(valid_index_data(vertices.len(), &indices));
        assert!(vertices.iter().all(|vertex| {
            vertex
                .position
                .iter()
                .chain(vertex.color.iter())
                .all(|value| value.is_finite())
        }));
    }

    #[test]
    fn pyramid_has_valid_indexed_triangles() {
        let (vertices, indices) = pyramid_data();
        assert_eq!(vertices.len(), 5);
        assert_eq!(indices.len(), 18);
        assert!(valid_index_data(vertices.len(), &indices));
    }

    #[test]
    fn crystal_has_eight_valid_facets() {
        let (vertices, indices) = crystal_data();
        assert_eq!(vertices.len(), 6);
        assert_eq!(indices.len(), 24);
        assert!(valid_index_data(vertices.len(), &indices));
        assert!(
            vertices
                .iter()
                .all(|vertex| vertex.position.iter().all(|v| v.is_finite()))
        );
    }

    #[test]
    fn world_chunks_share_edges_across_positive_and_negative_coordinates() {
        let northwest = world_terrain_chunk_data(-3, 2, 9).expect("northwest world chunk");
        let northeast = world_terrain_chunk_data(-2, 2, 9).expect("northeast world chunk");
        let southwest = world_terrain_chunk_data(-3, 3, 9).expect("southwest world chunk");
        let southeast = world_terrain_chunk_data(-2, 3, 9).expect("southeast world chunk");

        let world_vertex = |vertex: MeshVertex, center_x: f32, center_z: f32| {
            (
                [
                    vertex.position[0] + center_x,
                    vertex.position[1],
                    vertex.position[2] + center_z,
                ],
                vertex.color,
            )
        };
        let east_edge = |vertices: &[MeshVertex], center_x: f32, center_z: f32| {
            (0..9)
                .map(|row| world_vertex(vertices[row * 9 + 8], center_x, center_z))
                .collect::<Vec<_>>()
        };
        let west_edge = |vertices: &[MeshVertex], center_x: f32, center_z: f32| {
            (0..9)
                .map(|row| world_vertex(vertices[row * 9], center_x, center_z))
                .collect::<Vec<_>>()
        };
        let south_edge = |vertices: &[MeshVertex], center_x: f32, center_z: f32| {
            vertices[72..81]
                .iter()
                .copied()
                .map(|vertex| world_vertex(vertex, center_x, center_z))
                .collect::<Vec<_>>()
        };
        let north_edge = |vertices: &[MeshVertex], center_x: f32, center_z: f32| {
            vertices[0..9]
                .iter()
                .copied()
                .map(|vertex| world_vertex(vertex, center_x, center_z))
                .collect::<Vec<_>>()
        };

        assert_eq!(
            east_edge(&northwest.0, -192.0, 128.0),
            west_edge(&northeast.0, -128.0, 128.0)
        );
        assert_eq!(
            east_edge(&southwest.0, -192.0, 192.0),
            west_edge(&southeast.0, -128.0, 192.0)
        );
        assert_eq!(
            south_edge(&northwest.0, -192.0, 128.0),
            north_edge(&southwest.0, -192.0, 192.0)
        );
        assert_eq!(
            south_edge(&northeast.0, -128.0, 128.0),
            north_edge(&southeast.0, -128.0, 192.0)
        );
    }

    #[test]
    fn world_chunk_lod_uses_expected_grid_and_rejects_invalid_inputs() {
        let detailed = world_terrain_chunk_data(0, 0, 9).expect("detailed chunk");
        let distant = world_terrain_chunk_data(0, 0, 5).expect("distant chunk");
        assert_eq!(detailed.0.len(), 81 + 4 * 8 * 4 + 9 * 4 + 8 * 4 + 5 * 4);
        assert_eq!(distant.0.len(), 25 + 4 * 4 * 4 + 5 * 4 + 8 * 4 + 5 * 4);
        assert_eq!(detailed.1.len(), 384 + 4 * 8 * 6 + 8 * 3 * 6 + 8 * 6 + 4 * 3 * 6);
        assert_eq!(distant.1.len(), 96 + 4 * 4 * 6 + 4 * 3 * 6 + 8 * 6 + 4 * 3 * 6);
        assert!(valid_index_data(detailed.0.len(), &detailed.1));
        assert!(valid_index_data(distant.0.len(), &distant.1));
        for (vertices, _) in [&detailed, &distant] {
            assert!(vertices.iter().all(|vertex| {
                vertex
                    .position
                    .iter()
                    .chain(vertex.color.iter())
                    .all(|value| value.is_finite())
            }));
        }
        let base_vertex_count = 25;
        assert!(distant.0[base_vertex_count..].chunks_exact(4).all(|skirt| {
            (skirt[0].position[1] - skirt[1].position[1] - WORLD_TERRAIN_SKIRT_DEPTH).abs() < 1.0e-4
                && (skirt[2].position[1] - skirt[3].position[1] - WORLD_TERRAIN_SKIRT_DEPTH).abs()
                    < 1.0e-4
        }));
        assert!(world_terrain_chunk_data(i32::MAX, 0, 9).is_none());
        assert!(world_terrain_chunk_data(0, 0, 7).is_none());
    }

    #[test]
    fn world_chunk_road_ribbon_is_raised_continuous_and_inside_chunk_edges() {
        for side in [5, 9] {
            let (vertices, indices) = world_terrain_chunk_data(0, 0, side).expect("world chunk");
            let road_start = if side == 9 { 81 } else { 25 };
            let road_vertices = &vertices[road_start..road_start + side * 4];
            assert_eq!(road_vertices.len(), side * 4);
            assert!(road_vertices.iter().all(|vertex| {
                vertex
                    .position
                    .iter()
                    .chain(vertex.color.iter())
                    .all(|value| value.is_finite())
            }));
            for row in 0..side {
                let world_z = -32.0
                    + f32::from(u8::try_from(row).expect("road row fits u8"))
                        * (64.0 / f32::from(u8::try_from(side - 1).expect("road side fits u8")));
                let center = world_road_sample(0.0, world_z).center_x;
                let left = road_vertices[row * 4 + 1];
                let right = road_vertices[row * 4 + 2];
                assert!((left.position[0] - (center - 5.5)).abs() < 1.0e-4);
                assert!((right.position[0] - (center + 5.5)).abs() < 1.0e-4);
                assert!(left.position[1] > world_terrain_sample(center - 5.5, world_z).height);
                assert!(right.position[1] > world_terrain_sample(center + 5.5, world_z).height);
            }
            assert!(valid_index_data(vertices.len(), &indices));
        }
    }

    #[test]
    fn road_centre_markings_have_lod_independent_world_spacing() {
        for side in [5, 9] {
            let (vertices, indices) = world_terrain_chunk_data(0, 0, side).expect("road chunk");
            let base_vertices = side * side;
            let ribbon_vertices = side * 4;
            let marking_start = base_vertices + ribbon_vertices;
            let markings = &vertices[marking_start..marking_start + 8 * 4];
            assert_eq!(markings.len(), 32);
            for dash in 0..8 {
                let segment = &markings[dash * 4..dash * 4 + 4];
                assert!(
                    (segment[1].position[0] - segment[0].position[0] - 0.24).abs() < 1.0e-4
                );
                assert!(
                    (segment[3].position[0] - segment[2].position[0] - 0.24).abs() < 1.0e-4
                );
                assert!((segment[2].position[2] - segment[0].position[2] - 4.0).abs() < 1.0e-4);
                assert!(
                    segment
                        .iter()
                        .all(|vertex| vertex.color == [0.95, 0.78, 0.28])
                );
            }
            assert!(valid_index_data(vertices.len(), &indices));
        }
    }

    #[test]
    fn eastbound_spur_mesh_continues_across_streamed_chunks() {
        for side in [5, 9] {
            for (chunk_x, first_x, station_count) in [(0, 0.0, 5), (1, 32.0, 9), (2, 96.0, 9)] {
                let (vertices, indices) =
                    world_terrain_chunk_data(chunk_x, 0, side).expect("spur chunk");
                let spur_start = side * side + side * 4 + 8 * 4;
                let spur = &vertices[spur_start..spur_start + station_count * 4];
                assert_eq!(spur.len(), station_count * 4);
                for station in 0..station_count {
                    let row = &spur[station * 4..station * 4 + 4];
                    let world_x = first_x + f32::from(u8::try_from(station).expect("station fits u8")) * 8.0;
                    let origin_x = f32::from(i16::try_from(chunk_x).expect("chunk x fits i16")) * 64.0;
                    assert!(row.iter().all(|vertex| {
                        (vertex.position[0] + origin_x - world_x).abs() < 1.0e-4
                    }));
                    assert!(row.iter().all(|vertex| vertex.position[1] > -30.0));
                    assert!((row[1].position[2] - row[0].position[2] - 2.5).abs() < 1.0e-4);
                }
                assert!(valid_index_data(vertices.len(), &indices));
            }
        }
        let unrelated = world_terrain_chunk_data(0, 1, 9).expect("adjacent chunk");
        assert_eq!(unrelated.0.len(), 81 + 4 * 8 * 4 + 9 * 4 + 8 * 4);
    }

    #[test]
    fn road_ribbon_is_not_duplicated_into_distant_x_chunks() {
        let road_chunk = world_terrain_chunk_data(0, 0, 9).expect("road chunk");
        let distant_chunk = world_terrain_chunk_data(1, 0, 9).expect("distant chunk");
        assert_eq!(road_chunk.0.len(), 81 + 4 * 8 * 4 + 9 * 4 + 8 * 4 + 5 * 4);
        assert_eq!(distant_chunk.0.len(), 81 + 4 * 8 * 4 + 9 * 4);
        assert_eq!(distant_chunk.1.len(), 384 + 4 * 8 * 6 + 8 * 3 * 6);
    }

    #[test]
    fn terrain_surface_palette_distinguishes_all_driving_zones() {
        let colors = [
            terrain_surface_color(VehicleSurface::Asphalt),
            terrain_surface_color(VehicleSurface::Gravel),
            terrain_surface_color(VehicleSurface::Mud),
            terrain_surface_color(VehicleSurface::Sand),
            terrain_surface_color(VehicleSurface::Snow),
            terrain_surface_color(VehicleSurface::Rock),
        ];
        for color in colors {
            assert!(
                color
                    .iter()
                    .all(|component| (0.0..=1.0).contains(component))
            );
        }
        for (index, first) in colors.iter().enumerate() {
            for second in colors.iter().skip(index + 1) {
                assert_ne!(first, second);
            }
        }
    }

    #[test]
    fn terrain_chunks_have_valid_grids_and_matching_shared_edges() {
        let chunks = [
            terrain_chunk_data(0, 0, 9).expect("north-west chunk"),
            terrain_chunk_data(1, 0, 9).expect("north-east chunk"),
            terrain_chunk_data(0, 1, 9).expect("south-west chunk"),
            terrain_chunk_data(1, 1, 9).expect("south-east chunk"),
        ];
        for (vertices, indices) in &chunks {
            assert_eq!(vertices.len(), 9 * 9);
            assert_eq!(indices.len(), 8 * 8 * 6);
            assert!(valid_index_data(vertices.len(), indices));
            assert!(vertices.iter().all(|vertex| {
                vertex
                    .position
                    .iter()
                    .chain(vertex.color.iter())
                    .all(|value| value.is_finite())
            }));
        }
        let edge = |vertices: &[MeshVertex], x: usize| {
            (0..9)
                .map(|z| {
                    let vertex = vertices[z * 9 + x];
                    (vertex.position[1], vertex.color)
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(edge(&chunks[0].0, 8), edge(&chunks[1].0, 0));
        assert_eq!(edge(&chunks[2].0, 8), edge(&chunks[3].0, 0));
        let south_edge = |vertices: &[MeshVertex], z: usize| {
            (0..9)
                .map(|x| {
                    let vertex = vertices[z * 9 + x];
                    (vertex.position[1], vertex.color)
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(south_edge(&chunks[0].0, 8), south_edge(&chunks[2].0, 0));
        assert_eq!(south_edge(&chunks[1].0, 8), south_edge(&chunks[3].0, 0));
        assert!(terrain_chunk_data(2, 0, 9).is_none());
        assert!(terrain_chunk_data(0, 2, 9).is_none());
    }

    #[test]
    fn terrain_lod_uses_quarter_vertex_count_and_keeps_shared_edges() {
        let northwest = terrain_chunk_data(0, 0, 5).expect("north-west LOD chunk");
        let northeast = terrain_chunk_data(1, 0, 5).expect("north-east LOD chunk");
        assert_eq!(northwest.0.len(), 25);
        assert_eq!(northwest.1.len(), 4 * 4 * 6);
        assert!(valid_index_data(northwest.0.len(), &northwest.1));
        let west_edge = (0..5)
            .map(|z| {
                let vertex = northwest.0[z * 5 + 4];
                (vertex.position[1], vertex.color)
            })
            .collect::<Vec<_>>();
        let east_edge = (0..5)
            .map(|z| {
                let vertex = northeast.0[z * 5];
                (vertex.position[1], vertex.color)
            })
            .collect::<Vec<_>>();
        assert_eq!(west_edge, east_edge);
        assert!(terrain_chunk_data(0, 0, 4).is_none());
    }

    #[test]
    fn cube_uses_shared_face_vertices_and_indexed_triangles() {
        let (vertices, indices) = cube_data();
        assert_eq!(vertices.len(), 24);
        assert_eq!(indices.len(), 36);
        assert_eq!(indices.len() % 3, 0);
        assert!(
            indices
                .iter()
                .all(|index| usize::from(*index) < vertices.len())
        );
        assert!(vertices.iter().all(|vertex| {
            vertex
                .position
                .iter()
                .chain(vertex.color.iter())
                .all(|component| component.is_finite())
        }));
    }

    #[test]
    fn cube_faces_have_independent_material_colors() {
        let (vertices, _) = cube_data();
        let distinct_colors: std::collections::HashSet<[u32; 3]> = vertices
            .chunks_exact(4)
            .map(|face| face[0].color.map(f32::to_bits))
            .collect();
        assert_eq!(distinct_colors.len(), 6);
    }

    #[test]
    fn mesh_index_validation_rejects_invalid_triangles_and_references() {
        assert!(valid_index_data(3, &[0, 1, 2]));
        assert!(!valid_index_data(3, &[]));
        assert!(!valid_index_data(3, &[0, 1]));
        assert!(!valid_index_data(3, &[0, 1, 3]));
    }
}
