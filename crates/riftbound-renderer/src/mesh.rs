//! CPU mesh descriptions and their GPU vertex/index-buffer representation.

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
