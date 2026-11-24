//! CPU mesh descriptions and their GPU vertex-buffer representation.

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

/// GPU-resident, non-indexed triangle mesh.
pub struct Mesh {
    vertex_buffer: wgpu::Buffer,
    vertex_count: u32,
}

impl Mesh {
    /// Uploads a non-empty vertex slice to a GPU buffer.
    ///
    /// Returns None for an empty mesh or a vertex count that cannot fit in a draw call.
    #[must_use]
    pub fn from_vertices(
        device: &wgpu::Device,
        vertices: &[MeshVertex],
        label: &str,
    ) -> Option<Self> {
        if vertices.is_empty() {
            return None;
        }
        let vertex_count = u32::try_from(vertices.len()).ok()?;
        let mut bytes = Vec::with_capacity(vertices.len() * 24);
        for vertex in vertices {
            for component in vertex.position.iter().chain(vertex.color.iter()) {
                bytes.extend_from_slice(&component.to_ne_bytes());
            }
        }
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: &bytes,
            usage: wgpu::BufferUsages::VERTEX,
        });
        Some(Self {
            vertex_buffer,
            vertex_count,
        })
    }

    /// Creates the renderer's starter cube from ordinary CPU-side mesh vertices.
    #[must_use]
    pub fn cube(device: &wgpu::Device) -> Option<Self> {
        Self::from_vertices(device, &cube_vertices(), "riftbound-cube-mesh")
    }

    /// Returns the GPU vertex buffer.
    #[must_use]
    pub const fn vertex_buffer(&self) -> &wgpu::Buffer {
        &self.vertex_buffer
    }

    /// Returns the number of vertices in this mesh.
    #[must_use]
    pub const fn vertex_count(&self) -> u32 {
        self.vertex_count
    }
}

fn cube_vertices() -> Vec<MeshVertex> {
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

    let mut vertices = Vec::with_capacity(36);
    for (corners, color) in faces {
        for index in [0, 1, 2, 0, 2, 3] {
            vertices.push(MeshVertex {
                position: corners[index],
                color,
            });
        }
    }
    vertices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_mesh_has_six_triangles_per_face() {
        let vertices = cube_vertices();
        assert_eq!(vertices.len(), 36);
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
        let vertices = cube_vertices();
        for face in vertices.chunks_exact(6) {
            assert!(face.iter().all(|vertex| vertex.color == face[0].color));
        }
        let distinct_colors: std::collections::HashSet<[u32; 3]> = vertices
            .chunks_exact(6)
            .map(|face| face[0].color.map(f32::to_bits))
            .collect();
        assert_eq!(distinct_colors.len(), 6);
    }
}
