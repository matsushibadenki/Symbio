pub mod batching;
mod gather;
pub mod geometry;
pub mod guides;
pub mod mesh_resources;
use mesh_resources::{MeshBudget, MeshBudgetStatus, MeshCache};
#[cfg(not(target_arch = "wasm32"))]
pub mod native;
use threex_core::{GeometryKind, Instance, Scene, SceneStamp};
struct MeshBuffers {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
    format: wgpu::IndexFormat,
}
impl MeshBuffers {
    fn new(device: &wgpu::Device, data: geometry::MeshData) -> Self {
        let small = data.vertices.len() <= 65536;
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("shared geometry"),
            contents: bytemuck::cast_slice(&data.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices = if small {
            let values: Vec<u16> = data.indices.iter().map(|&i| i as u16).collect();
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("shared indices"),
                contents: bytemuck::cast_slice(&values),
                usage: wgpu::BufferUsages::INDEX,
            })
        } else {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("shared indices"),
                contents: bytemuck::cast_slice(&data.indices),
                usage: wgpu::BufferUsages::INDEX,
            })
        };
        Self {
            vertices,
            indices,
            count: data.indices.len() as u32,
            format: if small {
                wgpu::IndexFormat::Uint16
            } else {
                wgpu::IndexFormat::Uint32
            },
        }
    }
}
use wgpu::util::DeviceExt;
pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    guides: guides::Guides,
    guide_view: Option<guides::GuideView>,
    geometries: [MeshBuffers; 3],
    custom: MeshCache<MeshBuffers>,
    mesh_budget: MeshBudget,
    mesh_budget_dirty: bool,
    batches: batching::GeometryBatches,
    banks: [wgpu::Buffer; 3],
    bank_capacities: [usize; 3],
    banked: bool,
    gather: Option<gather::Gather>,
    gather_plans: gather::PlanCache,
    instances: wgpu::Buffer,
    camera: wgpu::Buffer,
    bind: wgpu::BindGroup,
    capacity: usize,
    count: u32,
    stamp: Option<SceneStamp>,
    depth: Option<wgpu::Texture>,
    size: (u32, u32),
}
impl Renderer {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let buffer = |label, bytes: &[u8], usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: bytes,
                usage,
            })
        };
        let geometries = [GeometryKind::Box, GeometryKind::Plane, GeometryKind::Sphere]
            .map(|kind| MeshBuffers::new(&device, geometry::unit_geometry(kind)));
        let camera = buffer(
            "camera",
            bytemuck::cast_slice(&threex_core::identity()),
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        );
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
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
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cube"),
            source: wgpu::ShaderSource::Wgsl(include_str!("cube.wgsl").into()),
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline=device.create_render_pipeline(&wgpu::RenderPipelineDescriptor{label:None,layout:Some(&pl),vertex:wgpu::VertexState{module:&shader,entry_point:Some("vs"),compilation_options:Default::default(),buffers:&[wgpu::VertexBufferLayout{array_stride:12,step_mode:wgpu::VertexStepMode::Vertex,attributes:&wgpu::vertex_attr_array![0=>Float32x3]},wgpu::VertexBufferLayout{array_stride:80,step_mode:wgpu::VertexStepMode::Instance,attributes:&wgpu::vertex_attr_array![1=>Float32x4,2=>Float32x4,3=>Float32x4,4=>Float32x4,5=>Float32x4]}]},fragment:Some(wgpu::FragmentState{module:&shader,entry_point:Some("fs"),compilation_options:Default::default(),targets:&[Some(wgpu::ColorTargetState{format,blend:None,write_mask:wgpu::ColorWrites::ALL})]}),primitive:wgpu::PrimitiveState::default(),depth_stencil:Some(wgpu::DepthStencilState{format:wgpu::TextureFormat::Depth32Float,depth_write_enabled:true,depth_compare:wgpu::CompareFunction::Less,stencil:Default::default(),bias:Default::default()}),multisample:Default::default(),multiview:None,cache:None});
        let gather = gather::Gather::new(&device);
        let storage = if gather.is_some() {
            wgpu::BufferUsages::STORAGE
        } else {
            wgpu::BufferUsages::empty()
        };
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instances"),
            size: 80,
            usage: wgpu::BufferUsages::VERTEX
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST
                | storage,
            mapped_at_creation: false,
        });
        let banks = std::array::from_fn(|_| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("geometry instances"),
                size: 80,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST | storage,
                mapped_at_creation: false,
            })
        });
        let guides = guides::Guides::new(&device, format, &shader, &pl);
        Self {
            guides,
            guide_view: None,
            device,
            queue,
            pipeline,
            geometries,
            custom: Default::default(),
            mesh_budget: MeshBudget::default(),
            mesh_budget_dirty: true,
            batches: Default::default(),
            banks,
            bank_capacities: [1; 3],
            banked: false,
            gather,
            gather_plans: Default::default(),
            instances,
            camera,
            bind,
            capacity: 1,
            count: 0,
            stamp: None,
            depth: None,
            size: (0, 0),
        }
    }
    pub fn set_guides(&mut self, view: Option<guides::GuideView>) {
        self.guide_view = view;
    }
    /// Applied on the next nonzero render, independently of the Scene stamp.
    pub fn set_mesh_budget(&mut self, budget: MeshBudget) {
        if self.mesh_budget != budget {
            self.mesh_budget = budget;
            self.mesh_budget_dirty = true;
        }
    }
    pub fn mesh_budget_status(&self) -> &MeshBudgetStatus {
        self.custom.status()
    }
    /// Clear presentation only. Geometry, instances and Scene revision stay resident.
    pub fn clear_target(&self, target: &wgpu::TextureView) {
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("empty host"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.012,
                            g: 0.016,
                            b: 0.024,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }
        self.queue.submit(Some(encoder.finish()));
    }
    pub fn render(
        &mut self,
        scene: &Scene,
        target: &wgpu::TextureView,
        width: u32,
        height: u32,
        vp: &[f32; 16],
    ) {
        self.render_region(scene, target, [width, height], [0, 0, width, height], vp);
    }
    /// ATOM extension: physical top-left viewport and scissor; clears the whole target.
    /// Invalid/empty regions are rejected before GPU work. No frame readback.
    pub fn render_region(
        &mut self,
        scene: &Scene,
        target: &wgpu::TextureView,
        size: [u32; 2],
        region: [u32; 4],
        vp: &[f32; 16],
    ) -> bool {
        let [width, height] = size;
        let [x, y, w, h] = region;
        if w == 0
            || h == 0
            || x.checked_add(w).is_none_or(|v| v > width)
            || y.checked_add(h).is_none_or(|v| v > height)
        {
            return false;
        }
        let storage = if self.gather.is_some() {
            wgpu::BufferUsages::STORAGE
        } else {
            wgpu::BufferUsages::empty()
        };
        let mut encoder = self.device.create_command_encoder(&Default::default());
        if !scene.is_current(self.stamp.as_ref()) {
            let data = scene.instances();
            let reallocated = data.len() > self.capacity;
            if reallocated {
                self.capacity = data.len().next_power_of_two();
                self.instances = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("instances"),
                    size: (self.capacity * std::mem::size_of::<Instance>()) as u64,
                    usage: wgpu::BufferUsages::VERTEX
                        | wgpu::BufferUsages::COPY_SRC
                        | wgpu::BufferUsages::COPY_DST
                        | storage,
                    mapped_at_creation: false,
                });
            }
            let updates = scene.instance_updates_since(self.stamp.as_ref(), reallocated);
            self.batches.sync(data, updates.ranges.clone());
            for range in updates.ranges {
                self.queue.write_buffer(
                    &self.instances,
                    (range.start * std::mem::size_of::<Instance>()) as u64,
                    bytemuck::cast_slice(&data[range]),
                );
            }
            let banked = self.batches.requires_banks();
            if banked {
                for kind in 0..3 {
                    let len = self.batches.indices(kind).len();
                    let reallocated = len > self.bank_capacities[kind];
                    let full = reallocated || !self.banked;
                    if reallocated {
                        self.bank_capacities[kind] = len.next_power_of_two();
                        self.banks[kind] = self.device.create_buffer(&wgpu::BufferDescriptor {
                            label: Some("geometry instances"),
                            size: (self.bank_capacities[kind] * 80) as u64,
                            usage: wgpu::BufferUsages::VERTEX
                                | wgpu::BufferUsages::COPY_DST
                                | storage,
                            mapped_at_creation: false,
                        });
                    }
                    if let Some(gather) = &mut self.gather {
                        match self.gather_plans.prepare(
                            &self.device.limits(),
                            [self.instances.size(), self.banks[kind].size()],
                            &self.batches,
                            kind,
                            full,
                        ) {
                            gather::Prepared::Gather { plan, upload } => gather.encode(
                                &self.device,
                                &self.queue,
                                &mut encoder,
                                kind,
                                [&self.instances, &self.banks[kind]],
                                (plan, upload),
                            ),
                            gather::Prepared::Copies(copies) => {
                                for copy in copies {
                                    encoder.copy_buffer_to_buffer(
                                        &self.instances,
                                        (copy.source * 80) as u64,
                                        &self.banks[kind],
                                        (copy.destination * 80) as u64,
                                        (copy.count * 80) as u64,
                                    );
                                }
                            }
                        }
                    } else {
                        for copy in self.batches.copies(kind, full) {
                            encoder.copy_buffer_to_buffer(
                                &self.instances,
                                (copy.source * 80) as u64,
                                &self.banks[kind],
                                (copy.destination * 80) as u64,
                                (copy.count * 80) as u64,
                            );
                        }
                    }
                }
            }
            self.banked = banked;
            self.mesh_budget_dirty = true;
            self.count = data.len() as u32;
            self.stamp = Some(updates.stamp);
        }
        if self.mesh_budget_dirty {
            self.custom.sync(
                self.mesh_budget,
                self.device.limits().max_buffer_size,
                self.batches.geometry_draws().map(|(spec, _)| spec),
                |spec| MeshBuffers::new(&self.device, geometry::geometry(spec)),
            );
            self.mesh_budget_dirty = false;
        }
        self.queue
            .write_buffer(&self.camera, 0, bytemuck::cast_slice(vp));
        if self.size != (width, height) {
            self.depth = Some(self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("depth"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth32Float,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            }));
            self.size = (width, height);
        }
        let depth = self
            .depth
            .as_ref()
            .unwrap()
            .create_view(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("opaque"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.045,
                            g: 0.043,
                            b: 0.039,
                            a: 1.,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_viewport(x as f32, y as f32, w as f32, h as f32, 0., 1.);
            pass.set_scissor_rect(x, y, w, h);
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind, &[]);
            for (spec, range) in self.batches.geometry_draws() {
                let kind = spec.kind;
                pass.set_vertex_buffer(
                    1,
                    if self.banked {
                        self.banks[kind as usize].slice(..)
                    } else {
                        self.instances.slice(..)
                    },
                );
                let geometry = if spec.flags() & !15 == 0 {
                    &self.geometries[kind as usize]
                } else {
                    self.custom
                        .get(&spec)
                        .unwrap_or(&self.geometries[kind as usize])
                };
                pass.set_vertex_buffer(0, geometry.vertices.slice(..));
                pass.set_index_buffer(geometry.indices.slice(..), geometry.format);
                pass.draw_indexed(0..geometry.count, 0, range);
            }
        }
        if let Some(view) = &self.guide_view {
            self.guides.encode(
                &self.device,
                &self.queue,
                &mut encoder,
                target,
                &depth,
                size,
                region,
                vp,
                view,
                &self.geometries[0],
                &self.bind,
                &self.instances,
            );
        }
        self.queue.submit([encoder.finish()]);
        true
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn shader_is_valid() {
        let module = naga::front::wgsl::parse_str(include_str!("cube.wgsl")).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
    }
    #[test]
    #[ignore = "Requires an accessible Metal/WebGPU adapter"]
    fn native_gpu_pipeline_and_draw() {
        pollster::block_on(async {
            let instance = wgpu::Instance::default();
            let adapter = instance
                .request_adapter(&Default::default())
                .await
                .expect("GPU adapter");
            let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
            device.push_error_scope(wgpu::ErrorFilter::Validation);
            let mut renderer = super::Renderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
            let target = renderer.device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: 32,
                    height: 32,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let mut scene = threex_core::Scene::default();
            let mut commands = [0u8; 128];
            commands[0..4].copy_from_slice(&1u32.to_le_bytes());
            commands[4..8].copy_from_slice(&1u32.to_le_bytes());
            commands[64..68].copy_from_slice(&2u32.to_le_bytes());
            commands[68..72].copy_from_slice(&1u32.to_le_bytes());
            for offset in [96, 100, 104, 108, 112, 116] {
                commands[offset..offset + 4].copy_from_slice(&1f32.to_le_bytes());
            }
            commands[120..124].copy_from_slice(&3u32.to_le_bytes());
            scene.apply(&commands).unwrap();
            let vp: [f32; 16] = bytemuck::cast(threex_core::identity());
            renderer.render(
                &scene,
                &target.create_view(&Default::default()),
                32,
                32,
                &vp,
            );
            assert_eq!(renderer.count, 1);
            for (id, flags) in [
                (2u32, 7u32),
                (3, 11),
                (4, 7 | (7 << 4) | (9 << 13)),
                (5, 11 | (256 << 4) | (256 << 13)),
            ] {
                let mut extra = commands;
                extra[4..8].copy_from_slice(&id.to_le_bytes());
                extra[68..72].copy_from_slice(&id.to_le_bytes());
                extra[120..124].copy_from_slice(&flags.to_le_bytes());
                scene.apply(&extra).unwrap();
            }
            renderer.render(
                &scene,
                &target.create_view(&Default::default()),
                32,
                32,
                &vp,
            );
            assert_eq!(renderer.count, 5);
            assert_eq!(renderer.mesh_budget_status().resident_meshes, 2);
            let largest =
                threex_core::GeometrySpec::new(threex_core::GeometryKind::Sphere, [256, 256])
                    .unwrap();
            assert_eq!(
                renderer.custom.get(&largest).unwrap().format,
                wgpu::IndexFormat::Uint32
            );
            let revisions: Vec<_> = (0..3).map(|k| renderer.batches.revision(k)).collect();
            renderer.set_mesh_budget(super::MeshBudget {
                max_bytes: 0,
                max_meshes: 0,
            });
            renderer.render(
                &scene,
                &target.create_view(&Default::default()),
                32,
                32,
                &vp,
            );
            assert_eq!(renderer.mesh_budget_status().resident_meshes, 0);
            assert_eq!(renderer.mesh_budget_status().fallback.len(), 2);
            assert_eq!(
                revisions,
                (0..3)
                    .map(|k| renderer.batches.revision(k))
                    .collect::<Vec<_>>()
            );
            renderer.set_mesh_budget(super::MeshBudget::default());
            renderer.render(
                &scene,
                &target.create_view(&Default::default()),
                32,
                32,
                &vp,
            );
            assert_eq!(renderer.mesh_budget_status().resident_meshes, 2);
            assert_eq!(renderer.batches.draws().count(), 3);
            assert!(renderer.device.pop_error_scope().await.is_none());
        });
    }
}

#[cfg(test)]
mod region_tests;
