//! Rust-owned surface selection, sampling, camera and native renderer.
use super::{
    UiError, error,
    layout::Panel,
    surface_gpu::{Gpu, Vertex},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use symbio_core::{
    Assumption, Domain,
    graph::{Graph, Id, Node},
};
use tauri::{Emitter, Manager};
const STEPS: usize = 64;
#[derive(Clone, Serialize)]
pub struct ViewState {
    pub cell: Option<u64>,
    pub revision: u64,
    pub branch: String,
    pub x_range: [f64; 2],
    pub y_range: [f64; 2],
    pub orbit: [f32; 3],
    pub wire: bool,
    pub samples: usize,
    pub omitted: usize,
    pub heights: Option<[f64; 2]>,
    pub status: String,
}
impl Default for ViewState {
    fn default() -> Self {
        Self {
            cell: None,
            revision: 0,
            branch: String::new(),
            x_range: [-5., 5.],
            y_range: [-5., 5.],
            orbit: [0.7, 0.6, 1.],
            wire: true,
            samples: 0,
            omitted: 0,
            heights: None,
            status: "surface_empty".into(),
        }
    }
}
#[derive(Default)]
pub struct Surface3d {
    state: Mutex<ViewState>,
    gpu: Mutex<Option<Gpu>>,
    ticket: AtomicU64,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Select { id: u64 },
    Range { x: [f64; 2], y: [f64; 2] },
    Orbit { yaw: f32, pitch: f32, zoom: f32 },
    Wire { enabled: bool },
    Reset,
}
pub fn initialize(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    match tauri::async_runtime::block_on(Gpu::prepare(app)) {
        Ok(gpu) => *app.state::<Surface3d>().gpu.lock().unwrap() = Some(gpu),
        Err(e) => {
            eprintln!("Symbio 3D unavailable: {e}");
            app.state::<Surface3d>().state.lock().unwrap().status = "gpu_unavailable".into();
        }
    }
    app.state::<Surface3d>().state.lock().unwrap().cell = app
        .state::<super::Shared>()
        .0
        .lock()
        .unwrap()
        .core
        .project()
        .current()
        .selected;
    refresh(app);
    Ok(())
}
fn publish(app: &tauri::AppHandle) {
    let state = app.state::<Surface3d>();
    let view = state.state.lock().unwrap().clone();
    let _ = app.emit("symbio://surface", view);
    let target = app.clone();
    let _ = app.run_on_main_thread(move || draw(&target));
}
#[tauri::command]
pub fn surface_state(webview: tauri::Webview) -> Result<ViewState, UiError> {
    super::authorize(&webview)?;
    Ok(webview
        .state::<Surface3d>()
        .state
        .lock()
        .map_err(|_| error("internal_error"))?
        .clone())
}
#[tauri::command]
pub async fn surface_command(
    webview: tauri::Webview,
    command: Command,
) -> Result<ViewState, UiError> {
    super::authorize(&webview)?;
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || apply_command(&app, command))
        .await
        .map_err(|_| error("internal_error"))?
}
fn apply_command(app: &tauri::AppHandle, command: Command) -> Result<ViewState, UiError> {
    let state = app.state::<Surface3d>();
    let resample = matches!(command, Command::Select { .. } | Command::Range { .. });
    let selected = if let Command::Select { id } = &command {
        let shared = app.state::<super::Shared>();
        let s = shared.0.lock().map_err(|_| error("internal_error"))?;
        if !s.core.project().current().cells.iter().any(|c| c.id == *id) {
            return Err(error("invalid_project"));
        }
        Some(*id)
    } else {
        None
    };
    if selected.is_some() {
        let layout = app.state::<super::workspace::Workspace>().snapshot();
        super::workspace::execute(
            app,
            super::layout::Command::Select {
                panel: Panel::Surface,
            },
            layout.revision,
        )?;
    }
    {
        let mut s = state.state.lock().map_err(|_| error("internal_error"))?;
        match command {
            Command::Select { .. } => s.cell = selected,
            Command::Range { x, y } => set_ranges(&mut s, x, y)?,
            Command::Orbit { yaw, pitch, zoom } => {
                if ![yaw, pitch, zoom].iter().all(|v| v.is_finite())
                    || !(-1.45..=1.45).contains(&pitch)
                    || !(0.35..=3.).contains(&zoom)
                {
                    return Err(error("domain_error"));
                }
                s.orbit = [yaw.rem_euclid(std::f32::consts::TAU), pitch, zoom];
            }
            Command::Wire { enabled } => s.wire = enabled,
            Command::Reset => s.orbit = ViewState::default().orbit,
        }
        if resample {
            state.ticket.fetch_add(1, Ordering::AcqRel);
        }
    }
    if resample {
        refresh(app)
    } else {
        publish(app)
    }
    Ok(state.state.lock().unwrap().clone())
}
fn set_ranges(state: &mut ViewState, x: [f64; 2], y: [f64; 2]) -> Result<(), UiError> {
    if [x, y]
        .iter()
        .any(|r| !r.iter().all(|v| v.is_finite() && v.abs() <= 1e6) || r[1] - r[0] < 1e-6)
    {
        return Err(error("surface_range"));
    }
    state.x_range = x;
    state.y_range = y;
    Ok(())
}
fn accepts(a: Option<&Assumption>, v: f64) -> bool {
    a.is_none_or(|a| {
        !(a.nonzero && v == 0.
            || a.domain == Domain::Positive && v <= 0.
            || a.domain == Domain::Nonnegative && v < 0.)
    })
}
struct Mesh {
    triangles: Vec<Vertex>,
    lines: Vec<Vertex>,
    samples: usize,
    omitted: usize,
    heights: [f64; 2],
}
fn sample(
    graph: &Graph,
    mut root: Id,
    assumptions: &BTreeMap<String, Assumption>,
    x_range: [f64; 2],
    y_range: [f64; 2],
) -> Result<Mesh, &'static str> {
    if let Node::Equation(a, b) = graph.nodes[root] {
        if matches!(&graph.nodes[a],Node::Symbol(s) if s=="z") {
            root = b
        } else {
            return Err("surface_equation");
        }
    }
    if graph.symbols(root).iter().any(|s| s != "x" && s != "y") {
        return Err("surface_variable");
    }
    let n = STEPS + 1;
    let mut values = vec![None; n * n];
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    let mut count = 0;
    for i in 0..n {
        for j in 0..n {
            let x = x_range[0] + (x_range[1] - x_range[0]) * j as f64 / STEPS as f64;
            let y = y_range[0] + (y_range[1] - y_range[0]) * i as f64 / STEPS as f64;
            if !accepts(assumptions.get("x"), x) || !accepts(assumptions.get("y"), y) {
                continue;
            }
            if let Ok(z) = graph.numeric(root, &BTreeMap::from([("x".into(), x), ("y".into(), y)]))
                && z.is_finite()
                && z.abs() <= 1e12
            {
                values[i * n + j] = Some(z);
                min = min.min(z);
                max = max.max(z);
                count += 1;
            }
        }
    }
    if count == 0 {
        return Err("domain_error");
    }
    let span = (max - min).max(1e-10);
    let flat = max - min < 1e-10;
    let vertex = |i: usize, j: usize| {
        values[i * n + j].map(|z| {
            let t = if flat { 0.5 } else { ((z - min) / span) as f32 };
            Vertex {
                position: [
                    -2. + 4. * j as f32 / STEPS as f32,
                    if flat { 0. } else { (t - 0.5) * 2.5 },
                    -2. + 4. * i as f32 / STEPS as f32,
                ],
                color: [
                    0.10 + 0.55 * t,
                    0.38 + 0.34 * (1. - (t * 2. - 1.).abs()),
                    0.57 + 0.21 * (1. - t),
                ],
            }
        })
    };
    let mut triangles = vec![];
    let mut lines = vec![];
    for i in 0..STEPS {
        for j in 0..STEPS {
            let points = [
                vertex(i, j),
                vertex(i, j + 1),
                vertex(i + 1, j),
                vertex(i + 1, j + 1),
            ];
            for indices in [[0, 2, 1], [1, 2, 3]] {
                if let [Some(a), Some(b), Some(c)] = indices.map(|k| points[k])
                    && (a.position[1] - b.position[1]).abs() < 1.5
                    && (a.position[1] - c.position[1]).abs() < 1.5
                    && (b.position[1] - c.position[1]).abs() < 1.5
                {
                    triangles.extend([a, b, c]);
                }
            }
            if i % 4 == 0
                && let (Some(a), Some(b)) = (points[0], points[1])
                && (a.position[1] - b.position[1]).abs() < 1.5
            {
                lines.extend([a, b].map(|mut v| {
                    v.position[1] += 0.004;
                    v.color = [0.14, 0.28, 0.32];
                    v
                }));
            }
            if j % 4 == 0
                && let (Some(a), Some(b)) = (points[0], points[2])
                && (a.position[1] - b.position[1]).abs() < 1.5
            {
                lines.extend([a, b].map(|mut v| {
                    v.position[1] += 0.004;
                    v.color = [0.14, 0.28, 0.32];
                    v
                }));
            }
        }
    }
    if triangles.is_empty() {
        return Err("domain_error");
    }
    // Floor grid plus distinct x / y / z axes; coordinates stay normalized for stable cameras.
    for k in 0..=8 {
        let p = -2. + k as f32 * 0.5;
        let color = [0.64, 0.72, 0.72];
        for (a, b) in [
            ([p, -1.35, -2.], [p, -1.35, 2.]),
            ([-2., -1.35, p], [2., -1.35, p]),
        ] {
            lines.extend([Vertex { position: a, color }, Vertex { position: b, color }]);
        }
    }
    Ok(Mesh {
        triangles,
        lines,
        samples: count,
        omitted: n * n - count,
        heights: [min, max],
    })
}
pub fn document_changed(app: &tauri::AppHandle) {
    let cell = app
        .state::<super::Shared>()
        .0
        .lock()
        .unwrap()
        .core
        .project()
        .current()
        .selected;
    app.state::<Surface3d>().state.lock().unwrap().cell = cell;
    refresh(app);
}
pub fn refresh(app: &tauri::AppHandle) {
    let state = app.state::<Surface3d>();
    let (ticket, view) = {
        let mut s = state.state.lock().unwrap();
        let ticket = state.ticket.fetch_add(1, Ordering::AcqRel) + 1;
        s.status = "working".into();
        (ticket, s.clone())
    };
    publish(app);
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let input = {
            let shared = app.state::<super::Shared>();
            let session = shared.0.lock().unwrap();
            let p = session.core.project();
            p.current()
                .cells
                .iter()
                .find(|c| Some(c.id) == view.cell)
                .map(|c| {
                    (
                        p.graph.clone(),
                        c.resolved_root.unwrap_or(c.root),
                        p.current().assumptions.clone(),
                        p.revision,
                        p.active.clone(),
                    )
                })
        };
        let (result, revision, branch) = match input {
            Some((g, r, a, v, b)) => (sample(&g, r, &a, view.x_range, view.y_range), v, b),
            None => (Err("missing_reference"), 0, String::new()),
        };
        let state = app.state::<Surface3d>();
        // Guard the generation check and publication with the same state lock as commands.
        let mut s = state.state.lock().unwrap();
        if state.ticket.load(Ordering::Acquire) != ticket {
            return;
        }
        s.revision = revision;
        s.branch = branch;
        let mut gpu = state.gpu.lock().unwrap();
        match result {
            Ok(mesh) => {
                s.samples = mesh.samples;
                s.omitted = mesh.omitted;
                s.heights = Some(mesh.heights);
                s.status = if gpu.is_some() {
                    "ready"
                } else {
                    "gpu_unavailable"
                }
                .into();
                if let Some(gpu) = gpu.as_mut() {
                    gpu.upload(&mesh.triangles, &mesh.lines);
                }
            }
            Err(code) => {
                s.samples = 0;
                s.omitted = 0;
                s.heights = None;
                s.status = code.into();
                if let Some(gpu) = gpu.as_mut() {
                    gpu.upload(&[], &[]);
                }
            }
        }
        drop(gpu);
        drop(s);
        publish(&app);
    });
}
#[derive(Clone, Serialize)]
struct AxisFrame {
    labels: Vec<super::axes3d::ScreenLabel>,
    cell: Option<u64>,
    revision: u64,
    x_range: [f64; 2],
    y_range: [f64; 2],
    orbit: [f32; 3],
    heights: Option<[f64; 2]>,
}
impl AxisFrame {
    fn new(view: &ViewState, labels: Vec<super::axes3d::ScreenLabel>) -> Self {
        Self {
            labels,
            cell: view.cell,
            revision: view.revision,
            x_range: view.x_range,
            y_range: view.y_range,
            orbit: view.orbit,
            heights: view.heights,
        }
    }
}
pub fn draw(app: &tauri::AppHandle) {
    let workspace = app.state::<super::workspace::Workspace>();
    let layout = workspace.effective();
    let host = layout.host(Panel::Surface);
    let state = app.state::<Surface3d>();
    let s = state.state.lock().unwrap().clone();
    let mut gpu = state.gpu.lock().unwrap();
    let Some(gpu) = gpu.as_mut() else { return };
    for label in std::iter::once("main").chain(Panel::ALL.map(|p| p.window())) {
        let Some(window) = app.get_window(label) else {
            continue;
        };
        if !window.is_visible().unwrap_or(false) {
            continue;
        }
        let Ok(size) = workspace.size(&window) else {
            continue;
        };
        let scale = window.scale_factor().unwrap_or(1.);
        let rect = if label == host {
            if let Some(g) = layout.floating_group(Panel::Surface) {
                (g.active == Panel::Surface).then_some([
                    0,
                    workspace.inset(label),
                    size.width,
                    size.height.saturating_sub(workspace.inset(label)),
                ])
            } else {
                layout
                    .geometry([size.width, size.height], scale, workspace.inset(label))
                    .groups
                    .iter()
                    .find(|g| g.active == Panel::Surface)
                    .map(|g| g.rect)
            }
        } else {
            None
        };
        let region = rect.and_then(|[x, y, w, h]| {
            let top = (210. * scale).round() as u32;
            let bottom = (24. * scale).round() as u32;
            (h > top + bottom && w > 0).then(|| [x, y + top, w, h - top - bottom])
        });
        let aspect = region.map_or(1., |r| r[2] as f32 / r[3].max(1) as f32);
        let matrix = camera(s.orbit, aspect);
        let axes = s
            .heights
            .filter(|_| s.status == "ready" && s.wire && region.is_some())
            .map(|z| super::axes3d::Axes::build(s.x_range, s.y_range, z, s.orbit[0]));
        gpu.upload_axes(axes.as_ref().map_or(&[], |a| a.lines.as_slice()));
        if label == host {
            let labels = axes.as_ref().map_or_else(Vec::new, |a| a.project(matrix));
            let _ = app.emit("symbio://surface-axes", AxisFrame::new(&s, labels));
        }
        if let Err(e) = gpu.draw(label, [size.width, size.height], region, &matrix, s.wire) {
            eprintln!("Symbio 3D: {e}");
        }
    }
}
fn camera([yaw, pitch, zoom]: [f32; 3], aspect: f32) -> [[f32; 4]; 4] {
    // Orbit basis targets the origin independently of Euler rotation order.
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    let view = [
        [cy, -sp * sy, cp * sy, 0.],
        [0., cp, sp, 0.],
        [-sy, -sp * cy, cp * cy, 0.],
        [0., 0., -8., 1.],
    ];
    let projection = threex_core::Perspective {
        fov: 45.,
        aspect,
        near: 0.1,
        far: 100.,
        zoom: zoom * aspect.min(1.) * 0.88,
    }
    .matrix()
    .unwrap_or_else(|_| threex_core::identity());
    threex_core::multiply(projection, view)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn surface_sampling_masks_domains_and_retains_exact_extents() {
        let mut g = Graph::default();
        let root = symbio_core::parser::parse(&mut g, "x^2-y^2").unwrap();
        let m = sample(&g, root, &BTreeMap::new(), [-2., 2.], [-2., 2.]).unwrap();
        assert_eq!(m.samples, 4225);
        assert_eq!(m.heights, [-4., 4.]);
        assert!(!m.triangles.is_empty());
        assert!(
            m.triangles
                .iter()
                .flat_map(|v| v.position)
                .all(f32::is_finite)
        );
    }
    #[test]
    fn holes_are_not_bridged_and_unsupported_symbols_reject() {
        let mut g = Graph::default();
        let root = symbio_core::parser::parse(&mut g, "1/x").unwrap();
        let m = sample(&g, root, &BTreeMap::new(), [-2., 2.], [-2., 2.]).unwrap();
        assert_eq!(m.omitted, 65);
        let root = symbio_core::parser::parse(&mut g, "t+x").unwrap();
        assert_eq!(
            sample(&g, root, &BTreeMap::new(), [-2., 2.], [-2., 2.]).err(),
            Some("surface_variable")
        );
    }
    #[test]
    fn cameras_are_finite_for_extreme_allowed_orbits() {
        for pitch in [-1.45, 0., 1.45] {
            for aspect in [0.1, 1., 10.] {
                assert!(
                    camera([1., pitch, 3.], aspect)
                        .iter()
                        .flatten()
                        .all(|n| n.is_finite())
                );
            }
        }
    }
}

#[cfg(test)]
mod semantic_tests {
    use super::*;
    #[test]
    fn explicit_equations_and_assumption_domains_use_the_same_sampling_path() {
        let mut g = Graph::default();
        let root = symbio_core::latex::parse(&mut g, "z=x^2-y^2").unwrap();
        let a = BTreeMap::from([(
            "x".into(),
            Assumption {
                domain: Domain::Nonnegative,
                ..Default::default()
            },
        )]);
        let m = sample(&g, root, &a, [-2., 2.], [-2., 2.]).unwrap();
        assert_eq!(m.samples, 33 * 65);
        assert_eq!(m.omitted, 32 * 65);
        let bad = symbio_core::latex::parse(&mut g, "x^2+y^2=1").unwrap();
        assert_eq!(
            sample(&g, bad, &a, [-2., 2.], [-2., 2.]).err(),
            Some("surface_equation")
        );
    }
    #[test]
    fn orbit_keeps_the_origin_centered() {
        for yaw in [0., 0.7, 3.] {
            for pitch in [-1., 0.6, 1.] {
                let m = camera([yaw, pitch, 1.], 0.7);
                assert_eq!(m[3][0], 0.);
                assert_eq!(m[3][1], 0.);
                assert!(m[3][3] > 0.);
            }
        }
    }
}

#[cfg(test)]
mod range_tests {
    use super::*;
    #[test]
    fn separate_domains_change_numeric_bounds_independently() {
        let mut g = Graph::default();
        let root = symbio_core::parser::parse(&mut g, "x+10*y").unwrap();
        let mesh = sample(&g, root, &BTreeMap::new(), [-2., 2.], [10., 30.]).unwrap();
        assert_eq!(mesh.heights, [98., 302.]);
        assert_eq!(mesh.samples, 4225);
        let changed = sample(&g, root, &BTreeMap::new(), [-1., 1.], [10., 30.]).unwrap();
        assert_eq!(changed.heights, [99., 301.]);
    }
    #[test]
    fn invalid_second_axis_never_changes_the_first_axis() {
        let mut view = ViewState::default();
        let before = (view.x_range, view.y_range);
        for bad in [[2., 2.], [3., -3.], [0., f64::NAN], [-1e7, 1.], [0., 1e-8]] {
            assert!(set_ranges(&mut view, [-2., 2.], bad).is_err());
            assert_eq!((view.x_range, view.y_range), before);
        }
        set_ranges(&mut view, [-2., 2.], [10., 30.]).unwrap();
        assert_eq!(view.x_range, [-2., 2.]);
        assert_eq!(view.y_range, [10., 30.]);
    }
}

#[cfg(test)]
mod transport_tests {
    #[test]
    fn frame_and_view_camera_stamps_have_identical_json_precision() {
        let view = super::ViewState::default();
        let frame = super::AxisFrame::new(&view, vec![]);
        let view: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&view).unwrap()).unwrap();
        let frame: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&frame).unwrap()).unwrap();
        assert_eq!(view["orbit"], frame["orbit"]);
    }
}
