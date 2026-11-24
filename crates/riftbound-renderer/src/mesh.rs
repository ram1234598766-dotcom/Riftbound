//! CPU mesh descriptions and their GPU vertex/index-buffer representation.

use riftbound_core::terrain_height;
use wgpu::util::DeviceExt;

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

    /// Creates a subdivided, flat terrain surface with deterministic ground colour variation.
    #[must_use]
    pub fn terrain(device: &wgpu::Device) -> Option<Self> {
        let (vertices, indices) = terrain_data();
        Self::from_data(device, &vertices, &indices, "riftbound-terrain-mesh")
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

fn terrain_data() -> (Vec<MeshVertex>, Vec<u16>) {
    const SIDE: usize = 17;
    let mut vertices = Vec::with_capacity(SIDE * SIDE);
    let mut indices = Vec::with_capacity((SIDE - 1) * (SIDE - 1) * 6);
    for z in 0..SIDE {
        for x in 0..SIDE {
            let u = f32::from(u8::try_from(x).expect("terrain x fits u8")) / 16.0;
            let v = f32::from(u8::try_from(z).expect("terrain z fits u8")) / 16.0;
            let variation =
                f32::from(u8::try_from((x * 13 + z * 7) % 5).expect("terrain variation fits u8"))
                    * 0.012;
            // Sample the same world-space terrain used by character grounding.
            let height = terrain_height(u * 20.0 - 10.0, v * 20.0 - 10.0);
            let elevation_tint = height.max(0.0) * 0.22;
            vertices.push(MeshVertex {
                position: [u * 2.0 - 1.0, height, v * 2.0 - 1.0],
                color: [
                    0.055 + variation + elevation_tint,
                    0.13 + variation + elevation_tint,
                    0.22 + variation * 1.5 + elevation_tint,
                ],
            });
        }
    }
    for z in 0..(SIDE - 1) {
        for x in 0..(SIDE - 1) {
            let top_left = u16::try_from(z * SIDE + x).expect("terrain vertex index fits u16");
            let top_right = top_left + 1;
            let bottom_left = top_left + u16::try_from(SIDE).expect("terrain side fits u16");
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
    (vertices, indices)
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
    fn terrain_is_a_subdivided_valid_grid_with_colour_variation() {
        let (vertices, indices) = terrain_data();
        assert_eq!(vertices.len(), 17 * 17);
        assert_eq!(indices.len(), 16 * 16 * 6);
        assert!(valid_index_data(vertices.len(), &indices));
        assert!(vertices.iter().all(|vertex| {
            vertex
                .position
                .iter()
                .chain(vertex.color.iter())
                .all(|value| value.is_finite())
        }));
        assert!(
            vertices
                .iter()
                .any(|vertex| vertex.color.map(f32::to_bits) != vertices[0].color.map(f32::to_bits))
        );
        assert!(vertices.iter().any(|vertex| vertex.position[1] > 0.01));
        assert!(vertices.iter().any(|vertex| vertex.position[1] < -0.01));
        assert!(
            vertices
                .iter()
                .all(|vertex| vertex.position[1].abs() <= 0.075)
        );
        assert!(
            vertices
                .iter()
                .filter(|vertex| (vertex.position[0].abs() - 1.0).abs() < 0.000_001
                    || (vertex.position[2].abs() - 1.0).abs() < 0.000_001)
                .all(|vertex| vertex.position[1].abs() < 0.000_001)
        );
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
