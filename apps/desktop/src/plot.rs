//! Rust-owned sample data, retained ThreeX Scene and wgpu surface; no frame IPC.
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::Manager;
use threex_renderer::native::{AcquireFailure, SurfaceBackend, WgpuSurface};
#[derive(Default)]
pub struct Plot {
    target: Mutex<Option<Target>>,
    busy: AtomicBool,
}
struct Target {
    scene: threex_core::Scene,
    backend: WgpuSurface,
    y_extent: f32,
}
struct Guard<'a>(&'a AtomicBool);
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
fn point(bytes: &mut Vec<u8>, id: u32, position: [f32; 3], scale: [f32; 3], color: [f32; 3]) {
    let mut record = [0u8; 64];
    record[..4].copy_from_slice(&1u32.to_le_bytes());
    record[4..8].copy_from_slice(&id.to_le_bytes());
    let values = [
        position[0],
        position[1],
        position[2],
        0.,
        0.,
        0.,
        scale[0],
        scale[1],
        scale[2],
        color[0],
        color[1],
        color[2],
    ];
    for (i, value) in values.into_iter().enumerate() {
        record[8 + i * 4..12 + i * 4].copy_from_slice(&value.to_le_bytes());
    }
    record[56..60].copy_from_slice(&3u32.to_le_bytes());
    bytes.extend(record);
    record[..4].copy_from_slice(&2u32.to_le_bytes());
    bytes.extend(record);
}
#[tauri::command]
pub async fn show_plot(
    webview: tauri::Webview,
    id: u64,
    generation: u64,
) -> Result<(), super::UiError> {
    super::authorize(&webview)?;
    let app = webview.app_handle().clone();
    let file_operation = app.state::<super::files::FileOperation>();
    let _file_guard = file_operation.acquire()?;
    let plot = app.state::<Plot>();
    if plot.busy.swap(true, Ordering::AcqRel) {
        return Err(super::error("panel_busy"));
    }
    let _guard = Guard(&plot.busy);
    let (graph, root, name, assumptions) = {
        let state = app.state::<super::Shared>();
        let session = state.0.lock().map_err(|_| super::error("internal_error"))?;
        session.check_generation(generation)?;
        let p = session.core.project();
        let c = p
            .current()
            .cells
            .iter()
            .find(|c| c.id == id)
            .ok_or_else(|| super::error("invalid_project"))?;
        (
            p.graph.clone(),
            c.resolved_root.unwrap_or(c.root),
            c.name.clone(),
            p.current().assumptions.clone(),
        )
    };
    let symbols = graph.symbols(root);
    if symbols.iter().any(|s| s != "x") {
        return Err(super::error("plot_variable"));
    }
    let samples = tauri::async_runtime::spawn_blocking(move || {
        let mut samples = vec![];
        for i in 0..1024 {
            let x = -5. + 10. * i as f64 / 1023.;
            if assumptions.get("x").is_some_and(|a| {
                (a.nonzero && x == 0.)
                    || (a.domain == symbio_core::Domain::Positive && x <= 0.)
                    || (a.domain == symbio_core::Domain::Nonnegative && x < 0.)
            }) {
                continue;
            }
            if let Ok(y) = graph.numeric(root, &BTreeMap::from([("x".into(), x)]))
                && y.abs() < 1e12
            {
                samples.push([x as f32, y as f32]);
            }
        }
        samples
    })
    .await
    .map_err(|_| super::error("internal_error"))?;
    if samples.is_empty() {
        return Err(super::error("domain_error"));
    }
    let extent = samples.iter().map(|p| p[1].abs()).fold(1f32, f32::max) * 1.1;
    let mut bytes = vec![];
    point(
        &mut bytes,
        1,
        [0., 0., 0.],
        [10.5, extent * 0.008, 0.02],
        [0.55; 3],
    );
    point(
        &mut bytes,
        2,
        [0., 0., 0.],
        [0.035, extent * 2., 0.02],
        [0.55; 3],
    );
    for (i, p) in samples.iter().enumerate() {
        point(
            &mut bytes,
            i as u32 + 3,
            [p[0], p[1], 0.02],
            [0.024, extent * 0.006, 0.015],
            [0.25, 0.78, 0.68],
        );
    }
    let mut scene = threex_core::Scene::default();
    scene
        .apply(&bytes)
        .map_err(|_| super::error("internal_error"))?;
    let title = format!(
        "Symbio · {name}(x) · x ∈ [−5, 5] · y ∈ [−{extent:.3}, {extent:.3}] · {} samples / float64",
        samples.len()
    );
    let needs_init = plot
        .target
        .lock()
        .map_err(|_| super::error("internal_error"))?
        .is_none();
    let handle = app.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let result: Result<Option<(wgpu::Instance, wgpu::Surface<'static>)>, super::UiError> =
            (|| {
                let window = if let Some(w) = handle.get_window("plot") {
                    w
                } else {
                    tauri::WindowBuilder::new(&handle, "plot")
                        .title(&title)
                        .inner_size(900., 600.)
                        .min_inner_size(400., 300.)
                        .build()
                        .map_err(|_| super::error("panel_error"))?
                };
                window
                    .set_title(&title)
                    .map_err(|_| super::error("panel_error"))?;
                window.show().map_err(|_| super::error("panel_error"))?;
                // Metal's CAMetalLayer must be created on AppKit's main thread.
                if needs_init {
                    let instance = wgpu::Instance::default();
                    let surface = instance
                        .create_surface(Arc::new(window))
                        .map_err(|_| super::error("gpu_unavailable"))?;
                    Ok(Some((instance, surface)))
                } else {
                    Ok(None)
                }
            })();
        let _ = tx.send(result);
    })
    .map_err(|_| super::error("panel_error"))?;
    let prepared = tauri::async_runtime::spawn_blocking(move || rx.recv())
        .await
        .map_err(|_| super::error("internal_error"))?
        .map_err(|_| super::error("internal_error"))??;
    if let Some((instance, surface)) = prepared {
        let backend = WgpuSurface::prepare(&instance, surface)
            .await
            .map_err(|_| super::error("gpu_unavailable"))?;
        *plot
            .target
            .lock()
            .map_err(|_| super::error("internal_error"))? = Some(Target {
            scene,
            backend,
            y_extent: extent,
        });
    } else {
        let mut target = plot
            .target
            .lock()
            .map_err(|_| super::error("internal_error"))?;
        if let Some(t) = target.as_mut() {
            t.scene = scene;
            t.y_extent = extent;
        }
    }
    let redraw = app.clone();
    app.run_on_main_thread(move || draw(&redraw))
        .map_err(|_| super::error("panel_error"))?;
    Ok(())
}
pub fn draw(app: &tauri::AppHandle) {
    let Some(window) = app.get_window("plot") else {
        return;
    };
    if !window.is_visible().unwrap_or(false) {
        return;
    }
    let Ok(size) = window.inner_size() else {
        return;
    };
    if size.width == 0 || size.height == 0 {
        return;
    }
    let state = app.state::<Plot>();
    let Ok(mut lock) = state.target.lock() else {
        return;
    };
    let Some(target) = lock.as_mut() else { return };
    if target.backend.configure(size.width, size.height).is_err() {
        return;
    }
    let y = target.y_extent;
    let matrix = [
        1. / 5.5,
        0.,
        0.,
        0.,
        0.,
        1. / y,
        0.,
        0.,
        0.,
        0.,
        -0.1,
        0.,
        0.,
        0.,
        0.5,
        1.,
    ];
    for attempt in 0..2 {
        match target.backend.draw(&target.scene, &matrix) {
            Ok(()) => break,
            Err(AcquireFailure::Lost | AcquireFailure::Outdated) if attempt == 0 => {
                let _ = target.backend.configure(size.width, size.height);
            }
            Err(e) => {
                eprintln!("Symbio plot: {e:?}");
                break;
            }
        }
    }
}
pub fn event(window: &tauri::Window, event: &tauri::WindowEvent) {
    match event {
        tauri::WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            let _ = window.hide();
        }
        tauri::WindowEvent::Resized(_)
        | tauri::WindowEvent::Focused(true)
        | tauri::WindowEvent::ScaleFactorChanged { .. } => draw(window.app_handle()),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn samples_create_visible_instances() {
        let mut bytes = vec![];
        super::point(&mut bytes, 1, [1., 2., 0.], [0.02; 3], [0.2, 0.8, 0.7]);
        let mut scene = threex_core::Scene::default();
        scene.apply(&bytes).unwrap();
        assert_eq!(scene.instances().len(), 1);
        assert_eq!(scene.instances()[0].model[3], [1., 2., 0., 1.]);
    }
}
