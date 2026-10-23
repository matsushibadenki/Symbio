//! ATOM editor guides: GPU-only grid, axes and visible-silhouette selection mask.
use wgpu::util::DeviceExt;
#[derive(Clone, Debug)]
pub struct Gizmo {
    pub origin: [f32; 3],
    pub axes: [[f32; 3]; 3],
    pub size: f32,
    pub selected_axis: Option<u32>,
    pub mode: u32,
}
#[derive(Clone, Debug)]
pub struct GuideView {
    pub selected: Option<[f32; 16]>,
    pub selected_index: u32,
    pub eye: [f32; 3],
    pub distance: f32,
    pub scale: f32,
    pub grid: bool,
    pub axes: bool,
    pub gizmo: Option<Gizmo>,
}
pub(crate) struct Guides {
    uniform: wgpu::Buffer,
    bind: wgpu::BindGroup,
    mask_layout: wgpu::BindGroupLayout,
    grid: wgpu::RenderPipeline,
    axes: wgpu::RenderPipeline,
    gizmo: wgpu::RenderPipeline,
    selected: wgpu::RenderPipeline,
    outline: wgpu::RenderPipeline,
    mask: Option<(wgpu::TextureView, wgpu::BindGroup, [u32; 2])>,
}
impl Guides {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        object_shader: &wgpu::ShaderModule,
        object_layout: &wgpu::PipelineLayout,
    ) -> Self {
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("editor guide view"),
            contents: &[0; 304],
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
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
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });
        let mask_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("editor guides"),
            source: wgpu::ShaderSource::Wgsl(include_str!("guides.wgsl").into()),
        });
        let standard = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let composite = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout, &mask_layout],
            push_constant_ranges: &[],
        });
        let make = |vertex, fragment, color, depth, mesh, layout: &wgpu::PipelineLayout| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(fragment),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vertex),
                    compilation_options: Default::default(),
                    buffers: if mesh {
                        &[wgpu::VertexBufferLayout {
                            array_stride: 12,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &wgpu::vertex_attr_array![0=>Float32x3],
                        }]
                    } else {
                        &[]
                    },
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: color,
                        blend: if mesh {
                            None
                        } else {
                            Some(wgpu::BlendState::ALPHA_BLENDING)
                        },
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: if depth {
                    Some(wgpu::DepthStencilState {
                        format: wgpu::TextureFormat::Depth32Float,
                        depth_write_enabled: false,
                        depth_compare: wgpu::CompareFunction::LessEqual,
                        stencil: Default::default(),
                        bias: Default::default(),
                    })
                } else {
                    None
                },
                multisample: Default::default(),
                multiview: None,
                cache: None,
            })
        };
        let grid = make("fullscreen", "grid", format, true, false, &standard);
        let axes = make("axes", "axis_color", format, true, false, &standard);
        let gizmo = make("axes", "axis_color", format, false, false, &standard);
        // Reuse the exact opaque vertex entry point, binding and instance buffer.
        // A separately compiled model/VP expression can disagree at near/far extremes.
        let selected = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shared object selection mask"), layout: Some(object_layout),
            vertex: wgpu::VertexState { module:object_shader, entry_point:Some("vs"), compilation_options:Default::default(), buffers:&[
                wgpu::VertexBufferLayout { array_stride:12,step_mode:wgpu::VertexStepMode::Vertex,attributes:&wgpu::vertex_attr_array![0=>Float32x3] },
                wgpu::VertexBufferLayout { array_stride:80,step_mode:wgpu::VertexStepMode::Instance,attributes:&wgpu::vertex_attr_array![1=>Float32x4,2=>Float32x4,3=>Float32x4,4=>Float32x4,5=>Float32x4] },
            ] },
            fragment:Some(wgpu::FragmentState { module:object_shader, entry_point:Some("selection_mask"),compilation_options:Default::default(),targets:&[Some(wgpu::ColorTargetState{format:wgpu::TextureFormat::R8Unorm,blend:None,write_mask:wgpu::ColorWrites::ALL})] }),
            primitive:Default::default(),depth_stencil:Some(wgpu::DepthStencilState{format:wgpu::TextureFormat::Depth32Float,depth_write_enabled:false,depth_compare:wgpu::CompareFunction::LessEqual,stencil:Default::default(),bias:Default::default()}),multisample:Default::default(),multiview:None,cache:None,
        });
        let outline = make("fullscreen", "outline", format, false, false, &composite);
        Self {
            uniform,
            bind,
            mask_layout,
            grid,
            axes,
            gizmo,
            selected,
            outline,
            mask: None,
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn encode(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        size: [u32; 2],
        region: [u32; 4],
        vp: &[f32; 16],
        view: &GuideView,
        geometry: &super::MeshBuffers,
        object_bind: &wgpu::BindGroup,
        instances: &wgpu::Buffer,
    ) {
        let Some(inverse) = inverse(vp) else {
            return;
        };
        let mut data = [0f32; 76];
        data[..16].copy_from_slice(vp);
        data[16..32].copy_from_slice(&inverse);
        if let Some(model) = view.selected {
            data[32..48].copy_from_slice(&model);
        }
        data[48..51].copy_from_slice(&view.eye);
        data[51] = view.distance.max(0.05);
        data[52..56].copy_from_slice(&region.map(|v| v as f32));
        data[56..60].copy_from_slice(&[
            u8::from(view.grid) as f32,
            u8::from(view.axes) as f32,
            view.scale.clamp(0.5, 4.),
            grid_step(view.distance),
        ]);
        if let Some(gizmo) = &view.gizmo {
            data[60..63].copy_from_slice(&gizmo.origin);
            data[63] = 1.;
            data[64..67].copy_from_slice(&gizmo.axes[0]);
            data[67] = gizmo.size;
            data[68..71].copy_from_slice(&gizmo.axes[1]);
            data[71] = gizmo.selected_axis.map_or(-1., |i| i as f32);
            data[72..75].copy_from_slice(&gizmo.axes[2]);
            data[75] = gizmo.mode as f32;
        }
        queue.write_buffer(&self.uniform, 0, bytemuck::cast_slice(&data));
        let attachment = |texture, load| {
            Some(wgpu::RenderPassColorAttachment {
                view: texture,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load,
                    store: wgpu::StoreOp::Store,
                },
            })
        };
        let depth_attachment = || {
            Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            })
        };
        let viewport = |pass: &mut wgpu::RenderPass<'_>| {
            pass.set_viewport(
                region[0] as f32,
                region[1] as f32,
                region[2] as f32,
                region[3] as f32,
                0.,
                1.,
            );
            pass.set_scissor_rect(region[0], region[1], region[2], region[3]);
        };
        if view.grid || view.axes {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("grid and axes"),
                color_attachments: &[attachment(target, wgpu::LoadOp::Load)],
                depth_stencil_attachment: depth_attachment(),
                ..Default::default()
            });
            viewport(&mut pass);
            pass.set_bind_group(0, &self.bind, &[]);
            pass.set_pipeline(&self.grid);
            pass.draw(0..3, 0..1);
            if view.axes {
                pass.set_pipeline(&self.axes);
                pass.draw(0..6, 0..3);
            }
        }
        let Some(model) = view.selected else {
            return;
        };
        let Some(scissor) = selection_scissor(vp, &model, region, view.scale) else {
            return;
        };
        if self.mask.as_ref().is_none_or(|m| m.2 != size) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("visible selection mask"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let mask = texture.create_view(&Default::default());
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &self.mask_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&mask),
                }],
            });
            self.mask = Some((mask, bind, size));
        }
        let (mask, bind, _) = self.mask.as_ref().unwrap();
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("visible selection"),
                color_attachments: &[attachment(
                    mask,
                    wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                )],
                depth_stencil_attachment: depth_attachment(),
                ..Default::default()
            });
            viewport(&mut pass);
            pass.set_pipeline(&self.selected);
            pass.set_bind_group(0, object_bind, &[]);
            pass.set_vertex_buffer(1, instances.slice(..));
            pass.set_vertex_buffer(0, geometry.vertices.slice(..));
            pass.set_index_buffer(geometry.indices.slice(..), geometry.format);
            pass.draw_indexed(
                0..geometry.count,
                0,
                view.selected_index..view.selected_index + 1,
            );
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("selection outline"),
                color_attachments: &[attachment(target, wgpu::LoadOp::Load)],
                ..Default::default()
            });
            viewport(&mut pass);
            pass.set_scissor_rect(scissor[0], scissor[1], scissor[2], scissor[3]);
            pass.set_pipeline(&self.outline);
            pass.set_bind_group(0, &self.bind, &[]);
            pass.set_bind_group(1, bind, &[]);
            pass.draw(0..3, 0..1);
        }
        if view.gizmo.is_some() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("transform handles"),
                color_attachments: &[attachment(target, wgpu::LoadOp::Load)],
                ..Default::default()
            });
            viewport(&mut pass);
            pass.set_pipeline(&self.gizmo);
            pass.set_bind_group(0, &self.bind, &[]);
            pass.draw(0..9, 3..6);
        }
    }
}
fn grid_step(distance: f32) -> f32 {
    let desired = distance.max(0.05) * 0.15;
    let decade = 10f32.powf(desired.log10().floor());
    let normalized = desired / decade;
    decade
        * if normalized >= 5. {
            5.
        } else if normalized >= 2. {
            2.
        } else {
            1.
        }
}

// Only shade a conservative projected box plus the DPI-scaled outline radius.
fn selection_scissor(
    vp: &[f32; 16],
    model: &[f32; 16],
    region: [u32; 4],
    scale: f32,
) -> Option<[u32; 4]> {
    let to_matrix = |m: &[f32; 16]| std::array::from_fn(|c| std::array::from_fn(|r| m[c * 4 + r]));
    let matrix = threex_core::multiply(to_matrix(vp), to_matrix(model));
    let mut lower = [f32::INFINITY; 2];
    let mut upper = [f32::NEG_INFINITY; 2];
    for x in [-0.5, 0.5] {
        for y in [-0.5, 0.5] {
            for z in [-0.5, 0.5] {
                let p = [x, y, z, 1.];
                let clip: [f32; 4] =
                    std::array::from_fn(|r| (0..4).map(|c| matrix[c][r] * p[c]).sum());
                if clip[3] <= 0. || clip[2] < 0. {
                    return Some(region);
                }
                let screen = [
                    region[0] as f32 + (clip[0] / clip[3] + 1.) * 0.5 * region[2] as f32,
                    region[1] as f32 + (1. - clip[1] / clip[3]) * 0.5 * region[3] as f32,
                ];
                for i in 0..2 {
                    lower[i] = lower[i].min(screen[i]);
                    upper[i] = upper[i].max(screen[i]);
                }
            }
        }
    }
    let pad = (scale.clamp(0.5, 4.) * 2.).round().clamp(1., 6.) + 1.;
    let lo: [u32; 2] = std::array::from_fn(|i| {
        (lower[i] - pad)
            .floor()
            .clamp(region[i] as f32, (region[i] + region[i + 2]) as f32) as u32
    });
    let hi: [u32; 2] = std::array::from_fn(|i| {
        (upper[i] + pad)
            .ceil()
            .clamp(region[i] as f32, (region[i] + region[i + 2]) as f32) as u32
    });
    (hi[0] > lo[0] && hi[1] > lo[1]).then_some([lo[0], lo[1], hi[0] - lo[0], hi[1] - lo[1]])
}
fn inverse(matrix: &[f32; 16]) -> Option<[f32; 16]> {
    let mut rows = [[0f64; 8]; 4];
    for r in 0..4 {
        for c in 0..4 {
            rows[r][c] = matrix[c * 4 + r] as f64;
        }
        rows[r][r + 4] = 1.;
    }
    for c in 0..4 {
        let pivot = (c..4).max_by(|a, b| rows[*a][c].abs().total_cmp(&rows[*b][c].abs()))?;
        if rows[pivot][c].abs() < 1e-20 {
            return None;
        }
        rows.swap(c, pivot);
        let d = rows[c][c];
        for n in &mut rows[c] {
            *n /= d;
        }
        for r in 0..4 {
            if r != c {
                let f = rows[r][c];
                let pivot_row = rows[c];
                for (value, pivot) in rows[r].iter_mut().zip(pivot_row) {
                    *value -= f * pivot;
                }
            }
        }
    }
    let result = std::array::from_fn(|i| rows[i % 4][i / 4 + 4] as f32);
    result.iter().all(|v| v.is_finite()).then_some(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_bounds_clip_to_viewport_and_skip_offscreen() {
        let identity: [f32; 16] = std::array::from_fn(|i| if i / 4 == i % 4 { 1. } else { 0. });
        let mut model = identity;
        model[14] = 0.6;
        model[10] = 0.1;
        let rect = selection_scissor(&identity, &model, [10, 20, 100, 100], 1.).unwrap();
        assert!(rect[0] > 10 && rect[1] > 20 && rect[2] < 100 && rect[3] < 100);
        model[12] = 100.;
        assert!(selection_scissor(&identity, &model, [10, 20, 100, 100], 1.).is_none());
        model[14] = -0.1;
        assert_eq!(
            selection_scissor(&identity, &model, [10, 20, 100, 100], 1.),
            Some([10, 20, 100, 100])
        );
    }
    #[test]
    fn editor_shader_validates() {
        let module = naga::front::wgsl::parse_str(include_str!("guides.wgsl")).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
    }
    #[test]
    fn projection_inverse_roundtrips_and_rejects_singular() {
        let m = threex_core::Perspective {
            fov: 50.,
            aspect: 1.5,
            near: 0.001,
            far: 40000.,
            zoom: 1.,
        }
        .matrix()
        .unwrap();
        let flat = std::array::from_fn(|i| m[i / 4][i % 4]);
        let inv = inverse(&flat).unwrap();
        let inv = std::array::from_fn(|c| std::array::from_fn(|r| inv[c * 4 + r]));
        let product = threex_core::multiply(m, inv);
        for (c, col) in product.iter().enumerate() {
            for (r, v) in col.iter().enumerate() {
                assert!((v - if c == r { 1. } else { 0. }).abs() < 1e-4);
            }
        }
        assert!(inverse(&[0.; 16]).is_none());
    }
}
