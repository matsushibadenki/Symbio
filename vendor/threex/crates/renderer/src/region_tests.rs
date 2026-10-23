//! ATOM host extension: compare region pixels with a full-size reference.
use super::*;
fn read(renderer: &Renderer, texture: &wgpu::Texture, size: u32) -> Vec<u8> {
    let buffer = renderer.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256 * size as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = renderer.device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
    );
    renderer.queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    renderer
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    rx.recv().unwrap().unwrap();
    buffer.slice(..).get_mapped_range().to_vec()
}
#[test]
#[ignore = "Requires accessible native GPU; reads back only in this test"]
fn native_gpu_region_is_unscaled_and_clipped() {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let adapter = instance.request_adapter(&Default::default()).await.unwrap();
        let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut renderer = Renderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
        let texture = |n| {
            renderer.device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: n,
                    height: n,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        };
        let full = texture(32);
        let region = texture(64);
        let mut scene = Scene::default();
        let mut commands = [0u8; 128];
        for (offset, value) in [(0, 1u32), (4, 1), (64, 2), (68, 1), (120, 3)] {
            commands[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        for offset in [96, 100, 104, 108, 112, 116] {
            commands[offset..offset + 4].copy_from_slice(&1f32.to_le_bytes());
        }
        scene.apply(&commands).unwrap();
        let vp: [f32; 16] = bytemuck::cast(threex_core::identity());
        renderer.render(&scene, &full.create_view(&Default::default()), 32, 32, &vp);
        let view = region.create_view(&Default::default());
        assert!(renderer.render_region(&scene, &view, [64, 64], [16, 16, 32, 32], &vp));
        let expected = read(&renderer, &full, 32);
        let actual = read(&renderer, &region, 64);
        let background = &actual[..4];
        let mut colored = 0;
        for y in 0..64usize {
            for x in 0..64usize {
                let pixel = &actual[y * 256 + x * 4..y * 256 + x * 4 + 4];
                if (16..48).contains(&x) && (16..48).contains(&y) {
                    let reference =
                        &expected[(y - 16) * 256 + (x - 16) * 4..(y - 16) * 256 + (x - 16) * 4 + 4];
                    assert_eq!(pixel, reference);
                    if pixel != background {
                        colored += 1;
                    }
                } else {
                    assert_eq!(pixel, background);
                }
            }
        }
        assert!(colored > 0);
        for r in [[0, 0, 0, 32], [63, 63, 2, 2], [u32::MAX, 0, 2, 1]] {
            assert!(!renderer.render_region(&scene, &view, [64, 64], r, &vp));
        }
        assert_eq!(read(&renderer, &region, 64), actual);
        let stamp = renderer.stamp.clone();
        let count = renderer.count;
        let instances = renderer.instances.clone();
        renderer.clear_target(&view);
        let blank = read(&renderer, &region, 64);
        assert!(
            blank
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| pixel == &blank[..4])
        );
        assert_eq!(renderer.stamp, stamp);
        assert_eq!(renderer.count, count);
        assert_eq!(renderer.instances, instances);
        assert!(renderer.render_region(&scene, &view, [64, 64], [16, 16, 32, 32], &vp));
        assert_eq!(read(&renderer, &region, 64), actual);
        renderer
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        assert!(renderer.device.pop_error_scope().await.is_none());
    });
}

#[test]
#[ignore = "Requires accessible native GPU; reads back only in this test"]
fn native_gpu_lighting_distinguishes_three_faces_after_nonuniform_scale() {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let adapter = instance.request_adapter(&Default::default()).await.unwrap();
        let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut renderer = Renderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
        let texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("lighting regression"),
            size: wgpu::Extent3d {
                width: 64,
                height: 64,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let mut scene = Scene::default();
        let mut commands = [0u8; 128];
        for (offset, value) in [(0, 1u32), (4, 1), (64, 2), (68, 1), (120, 3)] {
            commands[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        for (offset, value) in [
            (96, 1.3f32),
            (100, 0.8),
            (104, 1.),
            (108, 1.),
            (112, 1.),
            (116, 1.),
        ] {
            commands[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        scene.apply(&commands).unwrap();
        let camera = threex_core::Perspective {
            fov: 50.,
            aspect: 1.,
            near: 0.1,
            far: 100.,
            zoom: 1.,
        }
        .view_projection(&[2., 1.8, 4., -0.38, 0.46, 0., 1., 1., 1.])
        .unwrap();
        renderer.render(
            &scene,
            &texture.create_view(&Default::default()),
            64,
            64,
            &bytemuck::cast(camera),
        );
        let pixels = read(&renderer, &texture, 64);
        let mut levels = std::collections::BTreeMap::<u8, usize>::new();
        for pixel in pixels.as_chunks::<4>().0 {
            assert_eq!(pixel[3], 255);
            if pixel[0] > 32 {
                *levels.entry(pixel[0] / 16).or_default() += 1;
            }
        }
        let visible_faces: Vec<_> = levels
            .into_iter()
            .filter(|(_, count)| *count >= 10)
            .collect();
        assert!(
            visible_faces.len() >= 3,
            "three lit faces expected: {visible_faces:?}"
        );
        assert!(
            visible_faces.last().unwrap().0 - visible_faces[0].0 >= 5,
            "face contrast too low"
        );
        assert!(renderer.device.pop_error_scope().await.is_none());
    });
}

#[test]
#[ignore = "Requires accessible native GPU; reads back only in this test"]
fn native_gpu_selection_occlusion_grid_axes_and_mask_resize() {
    pollster::block_on(async {
        let instance = wgpu::Instance::default();
        let adapter = instance.request_adapter(&Default::default()).await.unwrap();
        let (device, queue) = adapter.request_device(&Default::default()).await.unwrap();
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut renderer = Renderer::new(device, queue, wgpu::TextureFormat::Rgba8Unorm);
        let texture = |n| {
            renderer.device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: n,
                    height: n,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        };
        let full = texture(64);
        let small = texture(32);
        let mut scene = Scene::default();
        let wire = |id: u32, position: [f32; 3], scale: f32| {
            let mut b = [0u8; 128];
            for (offset, value) in [(0, 1u32), (4, id), (64, 2), (68, id), (120, 3)] {
                b[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            }
            for (index, value) in position
                .into_iter()
                .chain([0.; 3])
                .chain([scale; 3])
                .chain([0.62, 0.58, 0.51])
                .enumerate()
            {
                b[72 + index * 4..76 + index * 4].copy_from_slice(&value.to_le_bytes());
            }
            b
        };
        let mut rotated = wire(1, [0., 0., 0.], 1.);
        for (offset, value) in [(84, 0.35f32), (88, 0.5), (100, 0.7), (104, 1.3)] {
            rotated[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        scene.apply(&rotated).unwrap();
        let vp: [f32; 16] = bytemuck::cast(
            threex_core::Perspective {
                fov: 50.,
                aspect: 1.,
                near: 0.001,
                far: 40000.,
                zoom: 1.,
            }
            .view_projection(&[0., 1.6, 5., -0.3, 0., 0., 1., 1., 1.])
            .unwrap(),
        );
        let view = full.create_view(&Default::default());
        let region = [8, 8, 48, 48];
        let mut guide = guides::GuideView {
            selected: None,
            selected_index: 0,
            eye: [0., 1.6, 5.],
            distance: 5.,
            scale: 1.,
            grid: false,
            axes: false,
            gizmo: None,
        };
        renderer.render_region(&scene, &view, [64, 64], region, &vp);
        let reference = read(&renderer, &full, 64);
        let stamp = renderer.stamp.clone();
        let instances = renderer.instances.clone();
        guide.selected = Some(bytemuck::cast(scene.instances()[0].model));
        renderer.set_guides(Some(guide.clone()));
        renderer.render_region(&scene, &view, [64, 64], region, &vp);
        let outlined = read(&renderer, &full, 64);
        let amber = |pixels: &[u8]| {
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[0] > 220 && p[1] > 80 && p[1] < 150 && p[2] < 50)
                .count()
        };
        for (i, (actual, original)) in outlined
            .as_chunks::<4>()
            .0
            .iter()
            .zip(reference.as_chunks::<4>().0)
            .enumerate()
        {
            let x = i % 64;
            let y = i / 64;
            if (8..56).contains(&x) && (8..56).contains(&y) && original != &reference[..4] {
                assert_eq!(
                    actual.as_slice(),
                    original.as_slice(),
                    "outline leaked into object at {x},{y}"
                );
            }
        }
        let thin = amber(&outlined);
        assert!(thin > 20, "missing silhouette: {thin}");
        // The solid surface remains unchanged; outline is only outside the mask.
        assert_eq!(
            &outlined[32 * 256 + 32 * 4..32 * 256 + 32 * 4 + 4],
            &reference[32 * 256 + 32 * 4..32 * 256 + 32 * 4 + 4]
        );
        guide.scale = 2.;
        renderer.set_guides(Some(guide.clone()));
        renderer.render_region(&scene, &view, [64, 64], region, &vp);
        assert!(amber(&read(&renderer, &full, 64)) > thin);
        guide.selected = None;
        guide.grid = true;
        renderer.set_guides(Some(guide.clone()));
        renderer.render_region(&scene, &view, [64, 64], region, &vp);
        let grid = read(&renderer, &full, 64);
        assert_ne!(grid, reference);
        for y in 0..64usize {
            for x in 0..64usize {
                if !(8..56).contains(&x) || !(8..56).contains(&y) {
                    assert_eq!(
                        &grid[y * 256 + x * 4..y * 256 + x * 4 + 4],
                        &reference[y * 256 + x * 4..y * 256 + x * 4 + 4]
                    );
                }
            }
        }
        guide.grid = false;
        guide.axes = true;
        renderer.set_guides(Some(guide.clone()));
        renderer.render_region(&scene, &view, [64, 64], region, &vp);
        let axes = read(&renderer, &full, 64);
        assert_ne!(axes, grid);
        assert_ne!(axes, reference);
        guide.axes = false;
        guide.selected = Some(bytemuck::cast(scene.instances()[0].model));
        renderer.set_guides(Some(guide.clone()));
        renderer.render_region(&scene, &view, [64, 64], region, &vp);
        let without_handles = read(&renderer, &full, 64);
        for mode in 1..=3 {
            guide.gizmo = Some(guides::Gizmo {
                origin: [0., 0., 0.],
                axes: [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]],
                size: 1.5,
                selected_axis: Some(0),
                mode,
            });
            renderer.set_guides(Some(guide.clone()));
            renderer.render_region(&scene, &view, [64, 64], region, &vp);
            let handles = read(&renderer, &full, 64);
            assert!(handles != without_handles, "missing gizmo mode {mode}");
            for y in 0..64usize {
                for x in 0..64usize {
                    if !(8..56).contains(&x) || !(8..56).contains(&y) {
                        let offset = y * 256 + x * 4;
                        assert_eq!(&handles[offset..offset + 4], &reference[offset..offset + 4]);
                    }
                }
            }
        }
        guide.gizmo = None;
        guide.selected = Some(bytemuck::cast(scene.instances()[0].model));
        renderer.set_guides(Some(guide.clone()));
        renderer.render(&scene, &small.create_view(&Default::default()), 32, 32, &vp);
        assert!(amber(&read(&renderer, &small, 32)) > 0);
        assert_eq!(renderer.stamp, stamp);
        assert_eq!(renderer.instances, instances);
        guide.selected = None;
        renderer.set_guides(Some(guide.clone()));
        renderer.render_region(&scene, &view, [64, 64], region, &vp);
        assert_eq!(read(&renderer, &full, 64), reference);
        // A larger foreground box completely hides the selected box: no x-ray outline.
        scene.apply(&wire(2, [0., 0., 2.], 3.)).unwrap();
        renderer.set_guides(None);
        renderer.render_region(&scene, &view, [64, 64], region, &vp);
        let occluder = read(&renderer, &full, 64);
        guide.selected = Some(bytemuck::cast(scene.instances()[0].model));
        renderer.set_guides(Some(guide));
        renderer.render_region(&scene, &view, [64, 64], region, &vp);
        assert_eq!(read(&renderer, &full, 64), occluder);
        assert!(renderer.device.pop_error_scope().await.is_none());
    });
}
