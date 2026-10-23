//! Native region renderer adapted from ThreeX's retained surface / scissor architecture.
//! Geometry and frame buffers never cross the WebView bridge.
use std::{collections::BTreeMap, sync::Arc};
use tauri::Manager;
use wgpu::util::DeviceExt;
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub color: [f32; 3],
}
struct Target {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    depth: Option<wgpu::TextureView>,
}
pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    targets: BTreeMap<String, Target>,
    triangles: wgpu::RenderPipeline,
    lines: wgpu::RenderPipeline,
    camera: wgpu::Buffer,
    bind: wgpu::BindGroup,
    mesh: Option<wgpu::Buffer>,
    grid: Option<wgpu::Buffer>,
    counts: [u32; 2],
}
impl Gpu {
    /// Surface/layer creation must happen on AppKit's event-loop thread, before WKWebViews.
    pub async fn prepare(app: &tauri::AppHandle) -> Result<Self, String> {
        let instance = wgpu::Instance::default();
        let mut surfaces = BTreeMap::new();
        for label in std::iter::once("main").chain(super::layout::Panel::ALL.map(|p| p.window())) {
            let window = app.get_window(label).ok_or("missing host")?;
            surfaces.insert(
                label.to_owned(),
                instance
                    .create_surface(Arc::new(window))
                    .map_err(|e| e.to_string())?,
            );
        }
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: surfaces.get("main"),
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|e| e.to_string())?;
        let format = surfaces["main"]
            .get_capabilities(&adapter)
            .formats
            .into_iter()
            .find(|f| f.is_srgb())
            .ok_or("surface format")?;
        let mut targets = BTreeMap::new();
        for (label, surface) in surfaces {
            let caps = surface.get_capabilities(&adapter);
            if !caps.formats.contains(&format) {
                return Err("surface format mismatch".into());
            }
            targets.insert(
                label,
                Target {
                    surface,
                    config: wgpu::SurfaceConfiguration {
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                        format,
                        width: 0,
                        height: 0,
                        present_mode: wgpu::PresentMode::Fifo,
                        desired_maximum_frame_latency: 2,
                        alpha_mode: caps.alpha_modes[0],
                        view_formats: vec![],
                    },
                    depth: None,
                },
            );
        }
        let camera = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("3D camera"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
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
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Mathematical surface"),
            source: wgpu::ShaderSource::Wgsl(include_str!("surface.wgsl").into()),
        });
        let pipeline = |topology, entry| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: 24,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3],
                    }],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: topology == wgpu::PrimitiveTopology::TriangleList,
                    depth_compare: wgpu::CompareFunction::LessEqual,
                    stencil: Default::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: Default::default(),
                multiview: None,
                cache: None,
            })
        };
        let triangles = pipeline(wgpu::PrimitiveTopology::TriangleList, "fs");
        let lines = pipeline(wgpu::PrimitiveTopology::LineList, "line");
        Ok(Self {
            device,
            queue,
            targets,
            triangles,
            lines,
            camera,
            bind,
            mesh: None,
            grid: None,
            counts: [0; 2],
        })
    }
    pub fn upload(&mut self, mesh: &[Vertex], grid: &[Vertex]) {
        self.counts = [mesh.len() as u32, grid.len() as u32];
        let buffer = |vertices: &[Vertex]| {
            (!vertices.is_empty()).then(|| {
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Rust surface vertices"),
                        contents: bytemuck::cast_slice(vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    })
            })
        };
        self.mesh = buffer(mesh);
        self.grid = buffer(grid);
    }
    pub fn draw(
        &mut self,
        label: &str,
        size: [u32; 2],
        region: Option<[u32; 4]>,
        matrix: &[[f32; 4]; 4],
        wire: bool,
    ) -> Result<(), String> {
        if size.contains(&0)
            || size
                .iter()
                .any(|n| *n > self.device.limits().max_texture_dimension_2d)
        {
            return Ok(());
        }
        let Some(t) = self.targets.get_mut(label) else {
            return Ok(());
        };
        if t.config.width != size[0] || t.config.height != size[1] {
            t.config.width = size[0];
            t.config.height = size[1];
            t.surface.configure(&self.device, &t.config);
            t.depth = Some(
                self.device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: None,
                        size: wgpu::Extent3d {
                            width: size[0],
                            height: size[1],
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Depth32Float,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                        view_formats: &[],
                    })
                    .create_view(&Default::default()),
            );
        }
        let frame = match t.surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                t.surface.configure(&self.device, &t.config);
                t.surface.get_current_texture().map_err(|e| e.to_string())?
            }
            Err(e) => return Err(e.to_string()),
        };
        let view = frame.texture.create_view(&Default::default());
        self.queue
            .write_buffer(&self.camera, 0, bytemuck::cast_slice(matrix));
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("3D panel"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.93,
                            g: 0.96,
                            b: 0.96,
                            a: 1.,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: t.depth.as_ref().unwrap(),
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if let Some([x, y, w, h]) = region
                .filter(|[x, y, w, h]| *w > 0 && *h > 0 && x + w <= size[0] && y + h <= size[1])
            {
                pass.set_viewport(x as f32, y as f32, w as f32, h as f32, 0., 1.);
                pass.set_scissor_rect(x, y, w, h);
                pass.set_bind_group(0, &self.bind, &[]);
                if let Some(mesh) = &self.mesh {
                    pass.set_pipeline(&self.triangles);
                    pass.set_vertex_buffer(0, mesh.slice(..));
                    pass.draw(0..self.counts[0], 0..1);
                }
                if wire && let Some(grid) = &self.grid {
                    pass.set_pipeline(&self.lines);
                    pass.set_vertex_buffer(0, grid.slice(..));
                    pass.draw(0..self.counts[1], 0..1);
                }
            }
        }
        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }
}
