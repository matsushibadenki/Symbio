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
    pub comparisons: Vec<u64>,
    pub layers: Vec<LayerState>,
    pub generation: u64,
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
            comparisons: vec![],
            layers: vec![],
            generation: 0,
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
const COLORS: [[u8; 3]; 4] = [[22, 101, 163], [190, 79, 0], [123, 69, 153], [0, 123, 99]];
#[derive(Clone, Serialize)]
pub struct LayerState {
    pub id: u64,
    pub name: String,
    pub color: [u8; 3],
    pub status: String,
    pub samples: usize,
    pub omitted: usize,
    pub heights: Option<[f64; 2]>,
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
    Compare { ids: Vec<u64> },
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
    generation: u64,
    revision: u64,
) -> Result<ViewState, UiError> {
    super::authorize(&webview)?;
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || apply_command(&app, command, generation, revision))
        .await
        .map_err(|_| error("internal_error"))?
}
fn apply_command(
    app: &tauri::AppHandle,
    command: Command,
    generation: u64,
    revision: u64,
) -> Result<ViewState, UiError> {
    let state = app.state::<Surface3d>();
    let resample = matches!(
        command,
        Command::Select { .. } | Command::Compare { .. } | Command::Range { .. }
    );
    {
        let shared = app.state::<super::Shared>();
        let session = shared.0.lock().map_err(|_| error("internal_error"))?;
        session.check_generation(generation)?;
        if session.core.project().revision != revision {
            return Err(error("revision_conflict"));
        }
    }
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
        let shared = app.state::<super::Shared>();
        let session = shared.0.lock().map_err(|_| error("internal_error"))?;
        session.check_generation(generation)?;
        if session.core.project().revision != revision {
            return Err(error("revision_conflict"));
        }
        let mut s = state.state.lock().map_err(|_| error("internal_error"))?;
        match command {
            Command::Select { .. } => {
                s.cell = selected;
                s.comparisons.retain(|id| Some(*id) != selected);
            }
            Command::Compare { ids } => {
                validate_comparisons(
                    s.cell,
                    &ids,
                    &session
                        .core
                        .project()
                        .current()
                        .cells
                        .iter()
                        .map(|c| c.id)
                        .collect::<Vec<_>>(),
                )?;
                s.comparisons = ids;
            }
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
        s.generation = generation;
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
fn validate_comparisons(
    primary: Option<u64>,
    ids: &[u64],
    available: &[u64],
) -> Result<(), UiError> {
    if ids.len() > 3 {
        return Err(error("surface_compare_limit"));
    }
    let mut seen = std::collections::BTreeSet::new();
    if !ids.is_empty() && !primary.is_some_and(|id| available.contains(&id)) {
        return Err(error("invalid_project"));
    }
    for id in ids {
        if Some(*id) == primary || !available.contains(id) || !seen.insert(*id) {
            return Err(error("invalid_project"));
        }
    }
    Ok(())
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
    surface_lines: usize,
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
    let span = max - min;
    let flat = span == 0.;
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
    let surface_lines = lines.len();
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
        surface_lines,
        triangles,
        lines,
        samples: count,
        omitted: n * n - count,
        heights: [min, max],
    })
}
fn linear(color: [u8; 3]) -> [f32; 3] {
    color.map(|c| {
        let c = c as f32 / 255.;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    })
}
fn sample_group(
    graph: &Graph,
    inputs: &[(u64, String, Option<Id>)],
    assumptions: &BTreeMap<String, Assumption>,
    x: [f64; 2],
    y: [f64; 2],
) -> (Option<Mesh>, Vec<LayerState>) {
    let mut meshes = Vec::new();
    let mut layers = Vec::new();
    for (index, (id, name, root)) in inputs.iter().enumerate() {
        let color = COLORS[index % COLORS.len()];
        let result = root
            .ok_or("missing_reference")
            .and_then(|root| sample(graph, root, assumptions, x, y));
        let mut layer = LayerState {
            id: *id,
            name: name.clone(),
            color,
            status: "ready".into(),
            samples: 0,
            omitted: 0,
            heights: None,
        };
        match result {
            Ok(mesh) => {
                layer.samples = mesh.samples;
                layer.omitted = mesh.omitted;
                layer.heights = Some(mesh.heights);
                meshes.push((index, mesh));
            }
            Err(code) => layer.status = code.into(),
        }
        layers.push(layer);
    }
    if meshes.is_empty() {
        return (None, layers);
    }
    if inputs.len() == 1 {
        return (Some(meshes.remove(0).1), layers);
    }
    let heights = [
        meshes
            .iter()
            .map(|(_, m)| m.heights[0])
            .fold(f64::INFINITY, f64::min),
        meshes
            .iter()
            .map(|(_, m)| m.heights[1])
            .fold(f64::NEG_INFINITY, f64::max),
    ];
    let mut combined = Mesh {
        surface_lines: 0,
        triangles: vec![],
        lines: vec![],
        samples: 0,
        omitted: 0,
        heights,
    };
    let floor = meshes[0].1.lines[meshes[0].1.surface_lines..].to_vec();
    for (index, mesh) in meshes {
        let color = linear(COLORS[index % COLORS.len()]);
        let old = mesh.heights;
        let span = heights[1] - heights[0];
        let remap = |mut v: Vertex, wire: bool| {
            let old_y = v.position[1] - if wire { 0.004 } else { 0. };
            let z = if old[1] == old[0] {
                old[0]
            } else {
                old[0] + (old_y as f64 / 2.5 + 0.5) * (old[1] - old[0])
            };
            v.position[1] = if span == 0. {
                0.
            } else {
                ((z - heights[0]) / span - 0.5) as f32 * 2.5
            };
            if wire {
                v.position[1] += 0.004;
                v.color = color.map(|c| c * 0.45)
            } else {
                v.color = color
            }
            v
        };
        combined
            .triangles
            .extend(mesh.triangles.into_iter().map(|v| remap(v, false)));
        combined.lines.extend(
            mesh.lines
                .into_iter()
                .take(mesh.surface_lines)
                .map(|v| remap(v, true)),
        );
        combined.samples += mesh.samples;
        combined.omitted += mesh.omitted;
    }
    combined.surface_lines = combined.lines.len();
    combined.lines.extend(floor);
    (Some(combined), layers)
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
    {
        let state = app.state::<Surface3d>();
        let mut view = state.state.lock().unwrap();
        view.cell = cell;
        view.comparisons.clear();
        view.layers.clear();
    }
    refresh(app);
}
pub fn refresh(app: &tauri::AppHandle) {
    let state = app.state::<Surface3d>();
    let (ticket, view) = {
        let shared = app.state::<super::Shared>();
        let session = shared.0.lock().unwrap();
        let mut s = state.state.lock().unwrap();
        if s.generation != session.generation() {
            s.generation = session.generation();
            s.cell = session.core.project().current().selected;
            s.comparisons.clear();
        }
        let ticket = state.ticket.fetch_add(1, Ordering::AcqRel) + 1;
        s.status = "working".into();
        s.layers.clear();
        s.samples = 0;
        s.omitted = 0;
        s.heights = None;
        (ticket, s.clone())
    };
    publish(app);
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (g, inputs, a, revision, branch, generation) = {
            let shared = app.state::<super::Shared>();
            let session = shared.0.lock().unwrap();
            let p = session.core.project();
            let ids = view
                .cell
                .into_iter()
                .chain(view.comparisons.iter().copied());
            let inputs = ids
                .map(|id| {
                    let c = p.current().cells.iter().find(|c| c.id == id);
                    (
                        id,
                        c.map(|c| c.name.clone())
                            .unwrap_or_else(|| format!("#{id}")),
                        c.map(|c| c.resolved_root.unwrap_or(c.root)),
                    )
                })
                .collect::<Vec<_>>();
            (
                p.graph.clone(),
                inputs,
                p.current().assumptions.clone(),
                p.revision,
                p.active.clone(),
                session.generation(),
            )
        };
        if generation != view.generation {
            return;
        }
        let (result, layers) = sample_group(&g, &inputs, &a, view.x_range, view.y_range);
        let state = app.state::<Surface3d>();
        // Guard the generation check and publication with the same state lock as commands.
        let mut s = state.state.lock().unwrap();
        if state.ticket.load(Ordering::Acquire) != ticket {
            return;
        }
        s.generation = generation;
        s.layers = layers;
        s.revision = revision;
        s.branch = branch;
        let mut gpu = state.gpu.lock().unwrap();
        match result {
            Some(mesh) => {
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
            None => {
                s.samples = 0;
                s.omitted = 0;
                s.heights = None;
                s.status = s
                    .layers
                    .first()
                    .map(|l| l.status.clone())
                    .unwrap_or_else(|| "surface_empty".into());
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
    generation: u64,
    comparisons: Vec<u64>,
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
            generation: view.generation,
            comparisons: view.comparisons.clone(),
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
        let region = region.filter(|_| s.status == "ready");
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

#[cfg(test)]
mod comparison_tests {
    use super::*;
    fn input(g: &mut Graph, id: u64, source: &str) -> (u64, String, Option<Id>) {
        (
            id,
            format!("s{id}"),
            Some(symbio_core::parser::parse(g, source).unwrap()),
        )
    }
    #[test]
    fn comparison_selection_is_bounded_unique_and_branch_local() {
        assert!(validate_comparisons(Some(1), &[2, 3, 4], &[1, 2, 3, 4]).is_ok());
        assert_eq!(
            validate_comparisons(Some(1), &[2, 3, 4, 5], &[1, 2, 3, 4, 5])
                .unwrap_err()
                .code,
            "surface_compare_limit"
        );
        for ids in [vec![1], vec![2, 2], vec![99]] {
            assert!(validate_comparisons(Some(1), &ids, &[1, 2]).is_err());
        }
        assert!(validate_comparisons(None, &[2], &[1, 2]).is_err());
        assert!(validate_comparisons(None, &[], &[]).is_ok());
        assert!(
            serde_json::from_str::<Command>(r#"{"kind":"compare","ids":[2],"extra":true}"#)
                .is_err()
        );
    }
    #[test]
    fn flat_planes_keep_their_absolute_height_separation() {
        let mut g = Graph::default();
        let inputs = [input(&mut g, 1, "0"), input(&mut g, 2, "10")];
        let (mesh, layers) = sample_group(&g, &inputs, &BTreeMap::new(), [-2., 2.], [-2., 2.]);
        let mesh = mesh.unwrap();
        assert_eq!(mesh.heights, [0., 10.]);
        assert_eq!(mesh.samples, 8450);
        assert_eq!(layers.len(), 2);
        let n = sample(
            &g,
            inputs[0].2.unwrap(),
            &BTreeMap::new(),
            [-2., 2.],
            [-2., 2.],
        )
        .unwrap()
        .triangles
        .len();
        assert!(
            mesh.triangles[..n]
                .iter()
                .all(|v| (v.position[1] + 1.25).abs() < 1e-6)
        );
        assert!(
            mesh.triangles[n..]
                .iter()
                .all(|v| (v.position[1] - 1.25).abs() < 1e-6)
        );
        assert_ne!(mesh.triangles[0].color, mesh.triangles[n].color);
        assert!(
            mesh.lines[mesh.surface_lines..]
                .iter()
                .all(|v| v.position[1] == -1.35)
        );
        assert_eq!(mesh.lines.len() - mesh.surface_lines, 36);
    }
    #[test]
    fn curves_share_one_height_scale_and_small_values_are_not_flattened() {
        let mut g = Graph::default();
        let inputs = [input(&mut g, 1, "x"), input(&mut g, 2, "2*x")];
        let (mesh, _) = sample_group(&g, &inputs, &BTreeMap::new(), [-1., 1.], [2., 3.]);
        let mesh = mesh.unwrap();
        assert_eq!(mesh.heights, [-2., 2.]);
        let count = sample(
            &g,
            inputs[0].2.unwrap(),
            &BTreeMap::new(),
            [-1., 1.],
            [2., 3.],
        )
        .unwrap()
        .triangles
        .len();
        assert!(
            mesh.triangles[..count]
                .iter()
                .all(|v| v.position[1].abs() <= 0.625001)
        );
        let tiny = [input(&mut g, 3, "0"), input(&mut g, 4, "1/1000000000000")];
        let (mesh, _) = sample_group(&g, &tiny, &BTreeMap::new(), [-1., 1.], [-1., 1.]);
        let mesh = mesh.unwrap();
        assert!(mesh.triangles.iter().any(|v| v.position[1] > 1.2));
        assert!(mesh.triangles.iter().any(|v| v.position[1] < -1.2));
        let root = symbio_core::parser::parse(&mut g, "x/1000000000000").unwrap();
        let m = sample(&g, root, &BTreeMap::new(), [-1., 1.], [-1., 1.]).unwrap();
        assert!(m.triangles.iter().any(|v| v.position[1].abs() > 1.2));
    }
    #[test]
    fn failures_are_per_layer_and_no_valid_geometry_is_fabricated() {
        let mut g = Graph::default();
        let inputs = [
            input(&mut g, 1, "t+x"),
            input(&mut g, 2, "x+y"),
            (3, "deleted".into(), None),
        ];
        let (mesh, layers) = sample_group(&g, &inputs, &BTreeMap::new(), [-1., 1.], [-1., 1.]);
        assert!(mesh.is_some());
        assert_eq!(layers[0].status, "surface_variable");
        assert_eq!(layers[1].status, "ready");
        assert_eq!(layers[2].status, "missing_reference");
        assert_eq!(mesh.unwrap().samples, 4225);
        let (mesh, layers) = sample_group(
            &g,
            &[inputs[0].clone(), inputs[2].clone()],
            &BTreeMap::new(),
            [-1., 1.],
            [-1., 1.],
        );
        assert!(mesh.is_none());
        assert!(layers.iter().all(|l| l.heights.is_none()));
        assert!(
            sample_group(&g, &[], &BTreeMap::new(), [-1., 1.], [-1., 1.])
                .0
                .is_none()
        );
    }
    #[test]
    fn assumptions_holes_ranges_and_geometry_recompute_together() {
        let mut g = Graph::default();
        let inputs = [input(&mut g, 1, "1/x"), input(&mut g, 2, "x+y")];
        let assumptions = BTreeMap::from([(
            "x".into(),
            Assumption {
                domain: Domain::Positive,
                ..Default::default()
            },
        )]);
        let (mesh, layers) = sample_group(&g, &inputs, &assumptions, [-1., 1.], [10., 30.]);
        let mesh = mesh.unwrap();
        assert_eq!(mesh.samples, 2 * 32 * 65);
        assert!(layers.iter().all(|l| l.omitted == 33 * 65));
        assert!(
            mesh.triangles
                .iter()
                .flat_map(|v| v.position)
                .all(f32::is_finite)
        );
        let first = mesh.heights;
        let (changed, _) = sample_group(&g, &inputs, &assumptions, [1., 2.], [-1., 1.]);
        assert_ne!(first, changed.unwrap().heights);
    }
    #[test]
    fn single_surface_retains_original_geometry_and_palette() {
        let mut g = Graph::default();
        let inputs = [input(&mut g, 1, "x^2-y^2")];
        let reference = sample(
            &g,
            inputs[0].2.unwrap(),
            &BTreeMap::new(),
            [-1., 1.],
            [-1., 1.],
        )
        .unwrap();
        let (mesh, _) = sample_group(&g, &inputs, &BTreeMap::new(), [-1., 1.], [-1., 1.]);
        let mesh = mesh.unwrap();
        assert_eq!(mesh.heights, reference.heights);
        assert_eq!(mesh.triangles.len(), reference.triangles.len());
        assert_eq!(mesh.lines.len(), reference.lines.len());
        assert_eq!(mesh.triangles[0].color, reference.triangles[0].color);
    }
    #[test]
    fn axis_stamps_include_document_generation_and_comparison_identity() {
        let view = ViewState {
            cell: Some(1),
            comparisons: vec![2, 3],
            generation: 9,
            ..Default::default()
        };
        let v = serde_json::to_value(AxisFrame::new(&view, vec![])).unwrap();
        assert_eq!(v["generation"], 9);
        assert_eq!(v["comparisons"], serde_json::json!([2, 3]));
        let bytes = serde_json::to_vec(&view).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains("triangles"));
        assert!(!text.contains("vertices"));
    }
}
