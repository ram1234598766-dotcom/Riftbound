//! GPU surface, depth target, camera, and mesh presentation for the Riftbound engine.

use std::sync::Arc;

use riftbound_core::{MaterialId, MeshId, Scene, Transform3, Vec3};
use wgpu::util::DeviceExt;

mod hud;
mod mesh;

pub use hud::HudData;
use hud::HudVertex;
use mesh::{Mesh, MeshVertex};

const HUD_SHADER: &str = r"
struct VertexInput { @location(0) position: vec2<f32>, @location(1) color: vec4<f32> };
struct VertexOutput { @builtin(position) position: vec4<f32>, @location(0) color: vec4<f32> };
@vertex fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(input.position, 0.0, 1.0);
    out.color = input.color;
    return out;
}
@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> { return input.color; }
";
use winit::{event_loop::OwnedDisplayHandle, window::Window};

const CUBE_SHADER: &str = include_str!("../shaders/cube.wgsl");

const HUD_BUFFER_CAPACITY: u64 = 512 * 1024;

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
    hud_pipeline: wgpu::RenderPipeline,
    hud_buffer: wgpu::Buffer,
    hud_buffer_capacity: u64,
    hud_vertices: Vec<HudVertex>,
    hud_bytes: Vec<u8>,
    hud_data: HudData,
    meshes: Vec<Mesh>,
    mesh_batches: Vec<MeshBatch>,
    changed_ids: Vec<riftbound_core::EntityId>,
    affected_meshes: Vec<bool>,
    visibility_dirty: bool,
    scene: Scene,
    camera_offset: [f32; 3],
    camera_yaw: f32,
    camera_pitch: f32,
    camera_distance: f32,
}

struct MeshBatch {
    mesh_index: usize,
    instance_buffer: wgpu::Buffer,
    instance_capacity: u64,
    instance_staging: Vec<u8>,
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
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
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
            contents: &camera_bytes(config.width, config.height, [0.0; 3], 0.588, 0.40, 6.2),
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
        let hud_pipeline = create_hud_pipeline(&device, config.format);
        // Reuse a bounded dynamic buffer instead of allocating a GPU buffer every frame.
        let hud_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("riftbound-hud-vertices"),
            size: HUD_BUFFER_CAPACITY,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let meshes = vec![
            Mesh::cube(&device).ok_or("Failed to create the built-in cube mesh")?,
            Mesh::pyramid(&device).ok_or("Failed to create the built-in pyramid mesh")?,
            Mesh::crystal(&device).ok_or("Failed to create the built-in crystal mesh")?,
            Mesh::terrain(&device).ok_or("Failed to create the built-in terrain mesh")?,
        ];
        let scene = starter_scene();
        let mesh_batches = create_mesh_batches(&device, &scene, meshes.len())?;
        let affected_meshes = vec![false; meshes.len()];

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
            hud_pipeline,
            hud_buffer,
            hud_buffer_capacity: HUD_BUFFER_CAPACITY,
            hud_vertices: Vec::with_capacity(4_096),
            hud_bytes: Vec::with_capacity(4_096 * 24),
            hud_data: HudData {
                collected: 0,
                total: 5,
                status: String::from("COLLECT RIFT SHARDS"),
                prompt: None,
            },
            meshes,
            mesh_batches,
            changed_ids: Vec::with_capacity(16),
            affected_meshes,
            visibility_dirty: true,
            scene,
            camera_offset: [0.0; 3],
            camera_yaw: 0.588,
            camera_pitch: 0.40,
            camera_distance: 6.2,
        })
    }

    /// Updates presentation dimensions, depth storage, and camera projection after resize.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.visibility_dirty = true;
        self.surface.configure(&self.device, &self.config);
        let (depth_texture, depth_view) = create_depth_target(&self.device, width, height);
        self.depth_texture = depth_texture;
        self.depth_view = depth_view;
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            &camera_bytes(
                width,
                height,
                self.camera_offset,
                self.camera_yaw,
                self.camera_pitch,
                self.camera_distance,
            ),
        );
    }

    /// Moves the camera target and eye to follow a world-space position.
    pub fn follow_camera(&mut self, position: Vec3) {
        self.camera_offset = [position.x, position.y, position.z];
        self.visibility_dirty = true;
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            &camera_bytes(
                self.config.width,
                self.config.height,
                self.camera_offset,
                self.camera_yaw,
                self.camera_pitch,
                self.camera_distance,
            ),
        );
    }

    /// Updates one live object's transform and rebuilds its instance batches.
    ///
    /// # Errors
    ///
    /// Returns an error if the scene contains more instances than a GPU draw call can address.
    pub fn set_object_transform(
        &mut self,
        id: riftbound_core::EntityId,
        transform: Transform3,
    ) -> Result<bool, std::num::TryFromIntError> {
        self.set_object_transforms(&[(id, transform)])
    }

    /// Updates several live object transforms with one GPU instance-buffer rebuild.
    ///
    /// Returns `false` without changing the scene if any supplied entity ID is stale.
    ///
    /// # Errors
    ///
    /// Returns an error if the scene contains more instances than a GPU draw call can address.
    pub fn set_object_transforms(
        &mut self,
        transforms: &[(riftbound_core::EntityId, Transform3)],
    ) -> Result<bool, std::num::TryFromIntError> {
        if transforms
            .iter()
            .any(|(id, _)| self.scene.get(*id).is_none())
        {
            return Ok(false);
        }
        self.changed_ids.clear();
        self.changed_ids
            .extend(transforms.iter().map(|(id, _)| *id));
        for (id, transform) in transforms {
            if let Some(current) = self.scene.get_mut(*id) {
                *current = *transform;
            }
        }

        // Only upload mesh batches whose objects (or parent hierarchies) changed.
        self.affected_meshes.fill(false);
        for object in self.scene.iter() {
            let mut ancestor = Some(object.id);
            let mut affected = false;
            while let Some(id) = ancestor {
                if self.changed_ids.contains(&id) {
                    affected = true;
                    break;
                }
                ancestor = self
                    .scene
                    .iter()
                    .find(|candidate| candidate.id == id)
                    .and_then(|candidate| candidate.parent);
            }
            if affected {
                if let Ok(mesh_index) = usize::try_from(object.mesh.index()) {
                    if let Some(flag) = self.affected_meshes.get_mut(mesh_index) {
                        *flag = true;
                    }
                }
            }
        }
        self.update_mesh_batch_buffers()?;
        self.visibility_dirty = true;
        Ok(true)
    }

    fn update_mesh_batch_buffers(&mut self) -> Result<(), std::num::TryFromIntError> {
        for batch in &mut self.mesh_batches {
            if !self
                .affected_meshes
                .get(batch.mesh_index)
                .copied()
                .unwrap_or(false)
            {
                continue;
            }
            batch.instance_staging.clear();
            let mut instance_count = 0_usize;
            for object in self.scene.iter() {
                let Ok(mesh_index) = usize::try_from(object.mesh.index()) else {
                    continue;
                };
                if mesh_index != batch.mesh_index {
                    continue;
                }
                let world = self
                    .scene
                    .world_matrix(object.id)
                    .expect("iterated scene objects must have valid hierarchy");
                let instance = InstanceRaw::from_matrix(world, material_tint(object.material));
                append_instance_bytes(&mut batch.instance_staging, &instance);
                instance_count += 1;
            }
            let draw_count = u32::try_from(instance_count)?;
            if instance_count == 0 {
                batch.instance_count = 0;
                continue;
            }
            let required_capacity = u64::try_from(batch.instance_staging.len())?;
            if required_capacity > batch.instance_capacity {
                batch.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("riftbound-mesh-instance-batch"),
                    size: required_capacity,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                batch.instance_capacity = required_capacity;
            }
            self.queue
                .write_buffer(&batch.instance_buffer, 0, &batch.instance_staging);
            batch.instance_count = draw_count;
        }
        Ok(())
    }

    /// Moves the camera target and eye together across the horizontal world plane.
    pub fn pan_camera(&mut self, delta_x: f32, delta_z: f32) {
        self.camera_offset[0] += delta_x;
        self.camera_offset[2] += delta_z;
        self.visibility_dirty = true;
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            &camera_bytes(
                self.config.width,
                self.config.height,
                self.camera_offset,
                self.camera_yaw,
                self.camera_pitch,
                self.camera_distance,
            ),
        );
    }

    /// Rotates the orbit camera around its followed target.
    pub fn orbit_camera(&mut self, yaw_delta: f32, pitch_delta: f32) {
        if !yaw_delta.is_finite() || !pitch_delta.is_finite() {
            return;
        }
        self.camera_yaw = (self.camera_yaw + yaw_delta).rem_euclid(std::f32::consts::TAU);
        self.camera_pitch = (self.camera_pitch + pitch_delta).clamp(0.12, 1.15);
        self.visibility_dirty = true;
        self.write_camera();
    }

    /// Returns the current horizontal orbit-camera yaw in radians.
    #[must_use]
    pub const fn camera_yaw(&self) -> f32 {
        self.camera_yaw
    }

    /// Adjusts orbit camera distance, clamped to a useful third-person range.
    pub fn zoom_camera(&mut self, distance_delta: f32) {
        if !distance_delta.is_finite() {
            return;
        }
        self.camera_distance = (self.camera_distance + distance_delta).clamp(3.0, 12.0);
        self.visibility_dirty = true;
        self.write_camera();
    }

    fn write_camera(&self) {
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            &camera_bytes(
                self.config.width,
                self.config.height,
                self.camera_offset,
                self.camera_yaw,
                self.camera_pitch,
                self.camera_distance,
            ),
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

    /// Updates the native mission HUD content.
    pub fn set_hud(&mut self, data: HudData) {
        self.hud_data = data;
    }

    /// Draws the world followed by a depth-independent 2D HUD overlay.
    #[allow(clippy::too_many_lines)]
    pub fn render(&mut self) {
        if self.visibility_dirty {
            self.update_visible_instance_buffers();
            self.visibility_dirty = false;
        }
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
        hud::build_vertices_into(
            &self.hud_data,
            self.config.width,
            self.config.height,
            &mut self.hud_vertices,
        );
        if !self.hud_vertices.is_empty() {
            self.hud_bytes.clear();
            for vertex in &self.hud_vertices {
                for value in vertex.position.iter().chain(vertex.color.iter()) {
                    self.hud_bytes.extend_from_slice(&value.to_ne_bytes());
                }
            }
            if self.hud_bytes.len() as u64 > self.hud_buffer_capacity {
                log::warn!(
                    "HUD vertex data exceeds the reusable buffer capacity; skipping HUD draw"
                );
            } else {
                self.queue
                    .write_buffer(&self.hud_buffer, 0, &self.hud_bytes);
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("riftbound-hud-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_pipeline(&self.hud_pipeline);
                pass.set_vertex_buffer(0, self.hud_buffer.slice(..));
                pass.draw(0..u32::try_from(self.hud_vertices.len()).unwrap_or(0), 0..1);
            }
        }
        self.queue.submit([encoder.finish()]);
        self.queue.present(frame);
    }

    /// Rebuilds visible instance lists only when the camera or scene changes.
    /// CPU staging vectors and GPU buffers are retained between updates.
    fn update_visible_instance_buffers(&mut self) {
        let camera = camera_bytes(
            self.config.width,
            self.config.height,
            self.camera_offset,
            self.camera_yaw,
            self.camera_pitch,
            self.camera_distance,
        );
        let mut view_projection = [0.0_f32; 16];
        for (index, value) in view_projection.iter_mut().enumerate() {
            let start = index * 4;
            *value = f32::from_ne_bytes(camera[start..start + 4].try_into().expect("matrix float"));
        }
        for batch in &mut self.mesh_batches {
            batch.instance_staging.clear();
            let mut count = 0_u32;
            for object in self.scene.iter() {
                if usize::try_from(object.mesh.index()).ok() != Some(batch.mesh_index) {
                    continue;
                }
                let world = self.scene.world_matrix(object.id)
                    .expect("iterated scene objects must have valid hierarchy");
                if !sphere_in_frustum(&view_projection, world) {
                    continue;
                }
                append_instance_bytes(
                    &mut batch.instance_staging,
                    &InstanceRaw::from_matrix(world, material_tint(object.material)),
                );
                count = count.saturating_add(1);
            }
            batch.instance_count = count;
            if batch.instance_staging.is_empty() {
                continue;
            }
            let required = u64::try_from(batch.instance_staging.len()).unwrap_or(u64::MAX);
            if required > batch.instance_capacity {
                batch.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("riftbound-visible-instance-batch"),
                    size: required,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                batch.instance_capacity = required;
            }
            self.queue.write_buffer(&batch.instance_buffer, 0, &batch.instance_staging);
        }
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
        let instance_capacity = u64::try_from(bytes.len())?;
        let instance_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("riftbound-mesh-instance-batch"),
            contents: &bytes,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        batches.push(MeshBatch {
            mesh_index,
            instance_buffer,
            instance_capacity,
            instance_staging: bytes,
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
        append_instance_bytes(&mut bytes, instance);
    }
    bytes
}

fn append_instance_bytes(bytes: &mut Vec<u8>, instance: &InstanceRaw) {
    for component in instance.model.iter().chain(instance.tint.iter()) {
        bytes.extend_from_slice(&component.to_ne_bytes());
    }
}

fn create_hud_pipeline(
    device: &wgpu::Device,
    surface_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("riftbound-hud-shader"),
        source: wgpu::ShaderSource::Wgsl(HUD_SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("riftbound-hud-layout"),
        bind_group_layouts: &[],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("riftbound-hud-pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(HudVertex::layout())],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
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

fn camera_bytes(
    width: u32,
    height: u32,
    offset: [f32; 3],
    yaw: f32,
    pitch: f32,
    distance: f32,
) -> [u8; 80] {
    #[allow(clippy::cast_precision_loss)]
    let aspect = width.max(1) as f32 / height.max(1) as f32;
    let horizontal = distance * pitch.cos();
    let eye = [
        offset[0] + yaw.sin() * horizontal,
        offset[1] + distance * pitch.sin(),
        offset[2] + yaw.cos() * horizontal,
    ];
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
    let mut bytes = [0_u8; 80];
    for (index, value) in matrix.iter().enumerate() {
        bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_ne_bytes());
    }
    for (index, value) in eye.into_iter().chain([1.0]).enumerate() {
        let start = 64 + index * 4;
        bytes[start..start + 4].copy_from_slice(&value.to_ne_bytes());
    }
    bytes
}

/// Conservative world-space bounding sphere test against the WebGPU clip volume.
/// The built-in meshes fit within a unit cube, so sqrt(3) covers their corners.
fn sphere_in_frustum(matrix: &[f32; 16], model: [f32; 16]) -> bool {
    let center = [model[12], model[13], model[14]];
    let scale = [
        (model[0] * model[0] + model[1] * model[1] + model[2] * model[2]).sqrt(),
        (model[4] * model[4] + model[5] * model[5] + model[6] * model[6]).sqrt(),
        (model[8] * model[8] + model[9] * model[9] + model[10] * model[10]).sqrt(),
    ];
    let radius = scale.into_iter().fold(0.0_f32, f32::max) * 1.733;
    let row = |index: usize| [matrix[index], matrix[4 + index], matrix[8 + index], matrix[12 + index]];
    let r0 = row(0);
    let r1 = row(1);
    let r2 = row(2);
    let r3 = row(3);
    let planes = [
        [r3[0] + r0[0], r3[1] + r0[1], r3[2] + r0[2], r3[3] + r0[3]],
        [r3[0] - r0[0], r3[1] - r0[1], r3[2] - r0[2], r3[3] - r0[3]],
        [r3[0] + r1[0], r3[1] + r1[1], r3[2] + r1[2], r3[3] + r1[3]],
        [r3[0] - r1[0], r3[1] - r1[1], r3[2] - r1[2], r3[3] - r1[3]],
        r2,
        [r3[0] - r2[0], r3[1] - r2[1], r3[2] - r2[2], r3[3] - r2[3]],
    ];
    planes.iter().all(|plane| {
        let distance = plane[0] * center[0] + plane[1] * center[1]
            + plane[2] * center[2] + plane[3];
        let normal_length = (plane[0] * plane[0] + plane[1] * plane[1]
            + plane[2] * plane[2]).sqrt();
        distance >= -radius * normal_length
    })
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
            camera_bytes(1280, 720, [0.0; 3], 0.588, 0.40, 6.2),
            camera_bytes(720, 1280, [1.0, 0.0, -1.0], 1.2, 0.8, 8.0),
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
    fn frustum_keeps_visible_objects_and_rejects_distant_objects() {
        let bytes = camera_bytes(1280, 720, [0.0; 3], 0.588, 0.40, 6.2);
        let mut matrix = [0.0_f32; 16];
        for (index, value) in matrix.iter_mut().enumerate() {
            *value = f32::from_ne_bytes(bytes[index * 4..index * 4 + 4].try_into().unwrap());
        }
        let visible = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0];
        let mut distant = visible;
        distant[12] = 10_000.0;
        assert!(sphere_in_frustum(&matrix, visible));
        assert!(!sphere_in_frustum(&matrix, distant));
    }

    #[test]
    fn matrix_multiplication_preserves_identity() {
        let identity = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        assert_eq!(multiply(identity, identity), identity);
    }
}
