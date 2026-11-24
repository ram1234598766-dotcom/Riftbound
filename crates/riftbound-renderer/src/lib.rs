//! GPU surface, depth target, camera, and mesh presentation for the Riftbound engine.

use std::sync::Arc;

use riftbound_core::{MaterialId, MeshId, Scene, Transform3, Vec3};
use wgpu::util::DeviceExt;

mod mesh;

use mesh::{Mesh, MeshVertex};
use winit::{event_loop::OwnedDisplayHandle, window::Window};

const CUBE_SHADER: &str = include_str!("../shaders/cube.wgsl");

/// Owns the GPU device, presentation surface, depth target, and scene pipeline.
pub struct Renderer {
    _instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    cube_pipeline: wgpu::RenderPipeline,
    meshes: Vec<Mesh>,
    mesh_batches: Vec<MeshBatch>,
    scene: Scene,
    camera_offset: [f32; 3],
}

struct MeshBatch {
    mesh_index: usize,
    instance_buffer: wgpu::Buffer,
    instance_count: u32,
}

impl Renderer {
    /// Creates a renderer for an existing native window.
    ///
    /// # Errors
    ///
    /// Returns an error if no compatible GPU adapter or surface configuration is available,
    /// or if the graphics device cannot be created.
    pub async fn new(
        display: OwnedDisplayHandle,
        window: Arc<Window>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(display),
        ));
        let surface = instance.create_surface(window.clone())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await?;

        let size = window.inner_size();
        let config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or("No compatible surface configuration was reported by the GPU adapter")?;
        surface.configure(&device, &config);

        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("riftbound-camera-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("riftbound-camera-uniform"),
            contents: &camera_bytes(config.width, config.height, [0.0; 3]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("riftbound-camera-bind-group"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let cube_pipeline = create_cube_pipeline(&device, &camera_layout, config.format);
        let meshes = vec![
            Mesh::cube(&device).ok_or("Failed to create the built-in cube mesh")?,
            Mesh::pyramid(&device).ok_or("Failed to create the built-in pyramid mesh")?,
        ];
        let scene = starter_scene();
        let mesh_batches = create_mesh_batches(&device, &scene, meshes.len())?;

        let (depth_texture, depth_view) = create_depth_target(&device, config.width, config.height);

        Ok(Self {
            _instance: instance,
            surface,
            device,
            queue,
            config,
            depth_texture,
            depth_view,
            camera_buffer,
            camera_bind_group,
            cube_pipeline,
            meshes,
            mesh_batches,
            scene,
            camera_offset: [0.0; 3],
        })
    }

    /// Updates presentation dimensions, depth storage, and camera projection after resize.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        let (depth_texture, depth_view) = create_depth_target(&self.device, width, height);
        self.depth_texture = depth_texture;
        self.depth_view = depth_view;
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            &camera_bytes(width, height, self.camera_offset),
        );
    }

    /// Moves the camera target and eye together across the horizontal world plane.
    pub fn pan_camera(&mut self, delta_x: f32, delta_z: f32) {
        self.camera_offset[0] += delta_x;
        self.camera_offset[2] += delta_z;
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            &camera_bytes(self.config.width, self.config.height, self.camera_offset),
        );
    }

    /// Returns the current CPU-side scene.
    #[must_use]
    pub const fn scene(&self) -> &Scene {
        &self.scene
    }

    /// Replaces the scene and rebuilds its per-mesh GPU instance batches.
    ///
    /// # Errors
    ///
    /// Returns an error if the scene contains more instances than a GPU draw call can address.
    pub fn set_scene(&mut self, scene: Scene) -> Result<(), std::num::TryFromIntError> {
        let mesh_batches = create_mesh_batches(&self.device, &scene, self.meshes.len())?;
        self.mesh_batches = mesh_batches;
        self.scene = scene;
        Ok(())
    }

    /// Draws each indexed mesh batch with depth testing.
    pub fn render(&mut self) {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                drop(frame);
                self.reconfigure();
                return;
            }
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.reconfigure();
                return;
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return,
            wgpu::CurrentSurfaceTexture::Validation => {
                log::error!("GPU surface validation failed while acquiring a frame");
                return;
            }
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("riftbound-frame-encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("riftbound-world-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.012,
                            g: 0.018,
                            b: 0.042,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.cube_pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            for batch in &self.mesh_batches {
                let Some(mesh) = self.meshes.get(batch.mesh_index) else {
                    continue;
                };
                pass.set_vertex_buffer(0, mesh.vertex_buffer().slice(..));
                pass.set_vertex_buffer(1, batch.instance_buffer.slice(..));
                pass.set_index_buffer(mesh.index_buffer().slice(..), wgpu::IndexFormat::Uint16);
                pass.draw_indexed(0..mesh.index_count(), 0, 0..batch.instance_count);
            }
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
    }

    fn reconfigure(&mut self) {
        self.surface.configure(&self.device, &self.config);
        let (depth_texture, depth_view) =
            create_depth_target(&self.device, self.config.width, self.config.height);
        self.depth_texture = depth_texture;
        self.depth_view = depth_view;
    }
}

#[derive(Clone, Copy)]
struct InstanceRaw {
    model: [f32; 16],
    tint: [f32; 4],
}

impl InstanceRaw {
    const ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
        2 => Float32x4,
        3 => Float32x4,
        4 => Float32x4,
        5 => Float32x4,
        6 => Float32x4
    ];

    const fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: 80,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBUTES,
        }
    }

    fn from_matrix(model: [f32; 16], tint: [f32; 4]) -> Self {
        Self { model, tint }
    }
}

fn starter_scene() -> Scene {
    let mut scene = Scene::new();
    let objects = [
        Transform3::new(
            Vec3::new(-1.65, 0.0, 0.0),
            Vec3::new(0.0, -0.25, 0.0),
            Vec3::new(0.58, 0.58, 0.58),
        ),
        Transform3::new(
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.15, 0.0),
            Vec3::new(0.82, 0.82, 0.82),
        ),
        Transform3::new(
            Vec3::new(1.65, 0.0, 0.0),
            Vec3::new(0.0, 0.45, 0.0),
            Vec3::new(0.58, 0.58, 0.58),
        ),
    ];
    for (index, transform) in objects.into_iter().enumerate() {
        let mesh = if index == 1 {
            MeshId::PYRAMID
        } else {
            MeshId::CUBE
        };
        let id = scene
            .spawn_with_mesh(transform, mesh)
            .expect("starter scene entity ID space available");
        if index == 1 {
            assert!(scene.set_material(id, MaterialId::RIFT));
        } else if index == 2 {
            assert!(scene.set_material(id, MaterialId::AMBER));
        }
    }
    scene
}

fn scene_instances_by_mesh(scene: &Scene, mesh_count: usize) -> Vec<Vec<InstanceRaw>> {
    let mut groups = (0..mesh_count).map(|_| Vec::new()).collect::<Vec<_>>();
    for object in scene.iter() {
        let Ok(mesh_index) = usize::try_from(object.mesh.index()) else {
            continue;
        };
        let Some(group) = groups.get_mut(mesh_index) else {
            continue;
        };
        let world = scene
            .world_matrix(object.id)
            .expect("iterated scene objects must have valid hierarchy");
        group.push(InstanceRaw::from_matrix(
            world,
            material_tint(object.material),
        ));
    }
    groups
}

fn create_mesh_batches(
    device: &wgpu::Device,
    scene: &Scene,
    mesh_count: usize,
) -> Result<Vec<MeshBatch>, std::num::TryFromIntError> {
    let groups = scene_instances_by_mesh(scene, mesh_count);
    let mut batches = Vec::new();
    for (mesh_index, instances) in groups.iter().enumerate() {
        if instances.is_empty() {
            continue;
        }
        let instance_count = u32::try_from(instances.len())?;
        let bytes = instance_bytes(instances);
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("riftbound-mesh-instance-batch"),
            contents: &bytes,
            usage: wgpu::BufferUsages::VERTEX,
        });
        batches.push(MeshBatch {
            mesh_index,
            instance_buffer,
            instance_count,
        });
    }
    Ok(batches)
}

fn material_tint(material: MaterialId) -> [f32; 4] {
    match material {
        MaterialId::RIFT => [0.45, 0.9, 1.0, 1.0],
        MaterialId::AMBER => [1.0, 0.62, 0.28, 1.0],
        _ => [1.0, 1.0, 1.0, 1.0],
    }
}

fn instance_bytes(instances: &[InstanceRaw]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(instances.len() * 80);
    for instance in instances {
        for component in instance.model.iter().chain(instance.tint.iter()) {
            bytes.extend_from_slice(&component.to_ne_bytes());
        }
    }
    bytes
}

fn create_cube_pipeline(
    device: &wgpu::Device,
    camera_layout: &wgpu::BindGroupLayout,
    surface_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("riftbound-cube-shader"),
        source: wgpu::ShaderSource::Wgsl(CUBE_SHADER.into()),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("riftbound-cube-pipeline-layout"),
        bind_group_layouts: &[Some(camera_layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("riftbound-cube-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(MeshVertex::layout()), Some(InstanceRaw::layout())],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
fn create_depth_target(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("riftbound-depth-buffer"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn camera_bytes(width: u32, height: u32, offset: [f32; 3]) -> [u8; 64] {
    #[allow(clippy::cast_precision_loss)]
    let aspect = width.max(1) as f32 / height.max(1) as f32;
    let eye = [3.2_f32 + offset[0], 2.4 + offset[1], 4.8 + offset[2]];
    let target = offset;
    let forward = normalize([target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]]);
    let side = normalize(cross(forward, [0.0, 1.0, 0.0]));
    let up = cross(side, forward);

    let view = [
        side[0],
        up[0],
        -forward[0],
        0.0,
        side[1],
        up[1],
        -forward[1],
        0.0,
        side[2],
        up[2],
        -forward[2],
        0.0,
        -dot(side, eye),
        -dot(up, eye),
        dot(forward, eye),
        1.0,
    ];
    let near = 0.1_f32;
    let far = 100.0_f32;
    let focal = 1.0 / (45.0_f32.to_radians() * 0.5).tan();
    let projection = [
        focal / aspect,
        0.0,
        0.0,
        0.0,
        0.0,
        -focal,
        0.0,
        0.0,
        0.0,
        0.0,
        far / (near - far),
        -1.0,
        0.0,
        0.0,
        (far * near) / (near - far),
        0.0,
    ];
    let matrix = multiply(projection, view);
    let mut bytes = [0_u8; 64];
    for (index, value) in matrix.iter().enumerate() {
        bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_ne_bytes());
    }
    bytes
}

fn multiply(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    let mut result = [0.0; 16];
    for (column, b_column) in b.chunks_exact(4).enumerate() {
        for (inner, a_column) in a.chunks_exact(4).enumerate() {
            let factor = b_column[inner];
            for (row, value) in a_column.iter().enumerate() {
                result[column * 4 + row] += value * factor;
            }
        }
    }
    result
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(value: [f32; 3]) -> [f32; 3] {
    let length = dot(value, value).sqrt().max(f32::EPSILON);
    [value[0] / length, value[1] / length, value[2] / length]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_uniform_is_finite_for_landscape_and_portrait_targets() {
        for bytes in [
            camera_bytes(1280, 720, [0.0; 3]),
            camera_bytes(720, 1280, [1.0, 0.0, -1.0]),
        ] {
            for chunk in bytes.chunks_exact(4) {
                let value = f32::from_ne_bytes(chunk.try_into().expect("four-byte float"));
                assert!(value.is_finite());
            }
        }
    }

    #[test]
    fn scene_instances_have_distinct_finite_model_transforms() {
        let scene = starter_scene();
        let groups = scene_instances_by_mesh(&scene, 2);
        assert_eq!(groups[0].len(), 2);
        assert_eq!(groups[1].len(), 1);
        let instances = groups.iter().flatten().collect::<Vec<_>>();
        assert_eq!(instances.len(), 3);
        assert_ne!(instances[0].model, instances[1].model);
        assert!(
            instances
                .iter()
                .all(|instance| { instance.model.iter().all(|component| component.is_finite()) })
        );
        assert_eq!(instance_bytes(&groups[0]).len(), groups[0].len() * 80);
        assert_eq!(groups[1][0].tint, material_tint(MaterialId::RIFT));
        assert_eq!(material_tint(MaterialId::AMBER), [1.0, 0.62, 0.28, 1.0]);
    }

    #[test]
    fn matrix_multiplication_preserves_identity() {
        let identity = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        assert_eq!(multiply(identity, identity), identity);
    }
}
