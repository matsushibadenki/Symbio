//! Persistent native child WebViews follow a Rust-owned dock tree (UNGE composition).
use super::{
    UiError, error,
    layout::{Axis, Command, Destination, LayoutState, Panel},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{Emitter, Manager};

pub struct Workspace {
    state: Mutex<LayoutState>,
    ready: Mutex<BTreeSet<String>>,
    path: Option<PathBuf>,
    moving: AtomicBool,
    resize_running: AtomicBool,
    resize_requested: AtomicBool,
    insets: Mutex<BTreeMap<String, u32>>,
    native_sizes: Mutex<BTreeMap<String, [u32; 2]>>,
    preview: Mutex<Option<SplitPreview>>,
}
#[derive(Clone)]
struct SplitPreview {
    origin: [f64; 2],
    scale: f64,
    owner: String,
    id: u32,
    ratio: f64,
    revision: u64,
}
struct Guard<'a>(&'a AtomicBool);
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
impl Workspace {
    pub fn load(path: PathBuf) -> Self {
        let read = || -> Result<LayoutState, Box<dyn std::error::Error>> {
            let file = match std::fs::File::open(&path) {
                Ok(f) => f,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    return Ok(LayoutState::default());
                }
                Err(e) => return Err(e.into()),
            };
            let mut bytes = Vec::new();
            file.take(16_385).read_to_end(&mut bytes)?;
            if bytes.len() > 16_384 {
                return Err("oversized layout".into());
            }
            Ok(LayoutState::decode(&bytes)?)
        };
        let (state, path) = match read() {
            Ok(s) => (s, Some(path)),
            Err(e) => {
                eprintln!("Symbio layout preserved; using defaults: {e}");
                (LayoutState::default(), None)
            }
        };
        Self {
            state: Mutex::new(state),
            ready: Mutex::new(BTreeSet::new()),
            path,
            moving: AtomicBool::new(false),
            resize_running: AtomicBool::new(false),
            resize_requested: AtomicBool::new(false),
            insets: Mutex::new(BTreeMap::new()),
            native_sizes: Mutex::new(BTreeMap::new()),
            preview: Mutex::new(None),
        }
    }
    pub fn inset(&self, label: &str) -> u32 {
        self.insets
            .lock()
            .expect("insets")
            .get(label)
            .copied()
            .unwrap_or(0)
    }
    /// Tauri's reparent-to-empty-window path may report the child size on macOS.
    /// Cache the actual native content bounds on its event-loop thread instead.
    pub fn size(&self, window: &tauri::Window) -> tauri::Result<tauri::PhysicalSize<u32>> {
        if let Some(size) = self
            .native_sizes
            .lock()
            .expect("native sizes")
            .get(window.label())
        {
            return Ok(tauri::PhysicalSize::new(size[0], size[1]));
        }
        window.inner_size()
    }
    /// Only called on the AppKit event-loop thread; never expose native geometry to JS.
    pub fn update_inset(&self, window: &tauri::Window) {
        #[cfg(target_os = "macos")]
        if let Ok(pointer) = window.ns_window() {
            // SAFETY: Tauri owns this live NSWindow and this caller is on the main thread.
            let native = unsafe { &*pointer.cast::<objc2_app_kit::NSWindow>() };
            if let Some(view) = native.contentView() {
                let frame = view.frame();
                let scale = window.scale_factor().unwrap_or(1.);
                self.native_sizes.lock().expect("native sizes").insert(
                    window.label().into(),
                    [
                        (frame.size.width * scale).round() as u32,
                        (frame.size.height * scale).round() as u32,
                    ],
                );
                let layout = native.contentLayoutRect();
                let top =
                    (frame.origin.y + frame.size.height - layout.origin.y - layout.size.height)
                        .max(view.safeAreaInsets().top)
                        .max(0.);
                self.insets.lock().expect("insets").insert(
                    window.label().into(),
                    (top * window.scale_factor().unwrap_or(1.)).round() as u32,
                );
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = window;
    }
    pub fn snapshot(&self) -> LayoutState {
        self.state.lock().expect("layout state").clone()
    }
    pub fn effective(&self) -> LayoutState {
        let mut state = self.snapshot();
        if let Some(p) = self.preview.lock().expect("split preview").as_ref()
            && p.revision == state.revision
            && let Some(root) = &mut state.root
        {
            root.set_ratio(p.id, p.ratio);
        }
        state
    }
    pub fn busy(&self) -> bool {
        self.moving.load(Ordering::Acquire)
    }
    fn begin(&self) -> Result<Guard<'_>, UiError> {
        self.moving
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| error("panel_busy"))?;
        Ok(Guard(&self.moving))
    }
    fn save(&self) -> Result<(), UiError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let write = || -> Result<(), Box<dyn std::error::Error>> {
            let parent = path.parent().ok_or("no layout directory")?;
            std::fs::create_dir_all(parent)?;
            let mut file = tempfile::NamedTempFile::new_in(parent)?;
            file.write_all(&serde_json::to_vec_pretty(&self.snapshot())?)?;
            file.as_file().sync_all()?;
            file.persist(path)?;
            Ok(())
        };
        write().map_err(|e| {
            eprintln!("Symbio layout save: {e}");
            error("layout_save_failed")
        })
    }
}
fn native(e: impl std::fmt::Display) -> UiError {
    eprintln!("Symbio panels: {e}");
    error("panel_error")
}
fn bounds(view: &tauri::Webview, r: [u32; 4]) -> Result<(), UiError> {
    view.set_bounds(tauri::Rect {
        position: tauri::PhysicalPosition::new(r[0] as i32, r[1] as i32).into(),
        size: tauri::PhysicalSize::new(r[2].max(1), r[3].max(1)).into(),
    })
    .map_err(native)
}
fn apply_bounds(app: &tauri::AppHandle) -> Result<(), UiError> {
    let home = app.get_window("main").ok_or_else(|| error("panel_error"))?;
    let size = app.state::<Workspace>().size(&home).map_err(native)?;
    if size.width == 0 || size.height == 0 {
        return Ok(());
    }
    let workspace = app.state::<Workspace>();
    let state = workspace.effective();
    let scale = home.scale_factor().map_err(native)?;
    let geometry = state.geometry([size.width, size.height], scale, workspace.inset("main"));
    if let Some(toolbar) = app.get_webview("toolbar") {
        bounds(&toolbar, geometry.toolbar)?;
    }
    for host in Panel::ALL {
        if let Some(window) = app.get_window(host.window()) {
            let title = state
                .floating_groups
                .iter()
                .find(|g| g.host == host)
                .map(|g| {
                    g.panels
                        .iter()
                        .map(|p| panel_title(*p, &state.locale).trim_start_matches("Symbio · "))
                        .collect::<Vec<_>>()
                        .join(" · ")
                });
            window
                .set_title(&format!(
                    "Symbio · {}",
                    title.unwrap_or_else(|| panel_title(host, &state.locale)
                        .trim_start_matches("Symbio · ")
                        .into())
                ))
                .map_err(native)?;
        }
    }
    for panel in Panel::ALL {
        let Some(view) = app.get_webview(panel.label()) else {
            continue;
        };
        let (rect, active) = if let Some(group) = state.floating_group(panel) {
            let window = view.window();
            let size = workspace.size(&window).map_err(native)?;
            let top = workspace.inset(window.label()).min(size.height);
            let height = size.height - top;
            (
                [0, top, size.width, height.min(size.height - top)],
                group.active == panel,
            )
        } else if let Some(group) = geometry.groups.iter().find(|g| g.panels.contains(&panel)) {
            (group.rect, group.active == panel)
        } else {
            continue;
        };
        // Inactive WKWebViews must finish initial navigation before being hidden.
        if active
            || !workspace
                .ready
                .lock()
                .expect("ready views")
                .contains(panel.label())
        {
            bounds(&view, rect)?;
            if active {
                view.show().map_err(native)?;
            }
        } else {
            view.hide().map_err(native)?;
        }
    }
    for i in 0..3 {
        if let Some(view) = app.get_webview(&format!("split-{i}")) {
            if let Some(split) = geometry.splits.get(i) {
                bounds(&view, split.rect)?;
                view.show().map_err(native)?;
            } else if workspace
                .ready
                .lock()
                .expect("ready views")
                .contains(view.label())
            {
                view.hide().map_err(native)?;
            } else {
                bounds(&view, [0, 0, 1, 1])?;
            }
        }
    }
    Ok(())
}
fn redraw(app: &tauri::AppHandle) {
    let target = app.clone();
    let _ = app.run_on_main_thread(move || {
        super::plot::draw(&target);
        super::surface3d::draw(&target);
    });
}
fn notify(app: &tauri::AppHandle) {
    let _ = app.emit("symbio://dock-preview", Option::<Destination>::None);
    let _ = app.emit("symbio://layout", app.state::<Workspace>().snapshot());
    redraw(app);
}
pub fn resize(app: &tauri::AppHandle) {
    let workspace = app.state::<Workspace>();
    workspace.resize_requested.store(true, Ordering::Release);
    if workspace.resize_running.swap(true, Ordering::AcqRel) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let workspace = app.state::<Workspace>();
        while workspace.resize_requested.swap(false, Ordering::AcqRel) {
            // A final resize/load/focus event must survive a concurrent reparent.
            let _guard = loop {
                if let Ok(guard) = workspace.begin() {
                    break guard;
                }
                std::thread::sleep(std::time::Duration::from_millis(4));
            };
            if let Err(e) = apply_bounds(&app) {
                eprintln!("Symbio bounds: {}", e.code);
            }
            notify(&app);
        }
        workspace.resize_running.store(false, Ordering::Release);
        if workspace.resize_requested.load(Ordering::Acquire) {
            resize(&app);
        }
    });
}
pub fn initialize(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let home = tauri::WindowBuilder::new(app, "main")
        .title("Symbio")
        .inner_size(1280., 800.)
        .min_inner_size(800., 480.)
        .visible(false)
        .build()?;
    let mut floating = Vec::new();
    for host in Panel::ALL {
        let window = floating_window(app, host).map_err(|e| std::io::Error::other(e.code))?;
        app.state::<Workspace>().update_inset(&window);
        floating.push(window);
    }
    // Prepare every possible host surface before child WebViews; reuse one renderer.
    let _ = floating;
    super::surface3d::initialize(app)?;
    app.state::<Workspace>().update_inset(&home);
    let state = app.state::<Workspace>().snapshot();
    add_view(&home, "toolbar")?;
    for panel in Panel::ALL {
        let parent = app
            .get_window(state.host(panel))
            .ok_or("missing panel host")?;
        app.state::<Workspace>().update_inset(&parent);
        add_view(&parent, panel.label())?;
        if state.floating.contains(&panel) {
            parent.show()?;
        }
    }
    for i in 0..3 {
        add_view(&home, &format!("split-{i}"))?;
    }
    apply_bounds(app).map_err(|e| std::io::Error::other(e.code))?;
    home.show()?;
    app.state::<super::window_placement::Store>()
        .start_recording();
    for host in Panel::ALL {
        if let Some(window) = app.get_window(host.window()) {
            app.state::<super::window_placement::Store>()
                .capture(&window);
        }
    }
    super::window_placement::schedule_save(app);
    Ok(())
}
fn add_view(window: &tauri::Window, label: &str) -> tauri::Result<()> {
    window.add_child(
        tauri::webview::WebviewBuilder::new(
            label,
            tauri::WebviewUrl::App(format!("index.html?view={label}").into()),
        )
        .transparent(label == "surface")
        .on_page_load(|view, payload| {
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                view.state::<Workspace>()
                    .ready
                    .lock()
                    .expect("ready views")
                    .insert(view.label().into());
                resize(view.app_handle());
            }
        }),
        tauri::PhysicalPosition::new(0, 0),
        tauri::PhysicalSize::new(320, 240),
    )?;
    Ok(())
}
fn panel_title(panel: Panel, locale: &str) -> &'static str {
    match (panel, locale) {
        (Panel::Surface, "ja") => "Symbio · 3D",
        (Panel::Surface, "zh-CN") => "Symbio · 3D",
        (Panel::Surface, _) => "Symbio · 3D",
        (Panel::Objects, "ja") => "Symbio · オブジェクト",
        (Panel::Inspector, "ja") => "Symbio · 構造・履歴",
        (Panel::Notebook, "ja") => "Symbio · ノート",
        (Panel::Objects, "zh-CN") => "Symbio · 对象",
        (Panel::Inspector, "zh-CN") => "Symbio · 结构与历史",
        (Panel::Notebook, "zh-CN") => "Symbio · 笔记",
        (Panel::Objects, _) => "Symbio · Objects",
        (Panel::Inspector, _) => "Symbio · Inspector",
        (Panel::Notebook, _) => "Symbio · Notebook",
    }
}
fn floating_window(app: &tauri::AppHandle, panel: Panel) -> Result<tauri::Window, UiError> {
    if let Some(window) = app.get_window(panel.window()) {
        return Ok(window);
    }
    let title = panel_title(panel, &app.state::<Workspace>().snapshot().locale);
    let window = tauri::WindowBuilder::new(app, panel.window())
        .title(title)
        .inner_size(if panel == Panel::Notebook { 800. } else { 380. }, 560.)
        .min_inner_size(240., 240.)
        .visible(false)
        .center()
        .build()
        .map_err(native)?;
    app.state::<super::window_placement::Store>()
        .restore(&window, [240., 240.])
        .map_err(native)?;
    Ok(window)
}
// Worker-only: flush native metrics before reparenting a child into an empty window.
fn refresh_metrics(app: &tauri::AppHandle, window: &tauri::Window) -> Result<(), UiError> {
    let (tx, rx) = std::sync::mpsc::channel();
    let window = window.clone();
    let target = app.clone();
    app.run_on_main_thread(move || {
        target.state::<Workspace>().update_inset(&window);
        let _ = tx.send(());
    })
    .map_err(native)?;
    rx.recv().map_err(native)
}
/// Perform native moves before publication; rollback every moved WebView on failure.
pub fn execute(
    app: &tauri::AppHandle,
    command: Command,
    expected: u64,
) -> Result<LayoutState, UiError> {
    let workspace = app.state::<Workspace>();
    let _guard = workspace.begin()?;
    if workspace.preview.lock().expect("split preview").is_some() {
        return Err(error("panel_busy"));
    }
    let before = workspace.snapshot();
    let focus = match &command {
        Command::Dock { panel, .. }
        | Command::FloatGroup { panel }
        | Command::Return { panel }
        | Command::ReturnGroup { panel } => Some(*panel),
        _ => None,
    };
    let next = before.apply(command, expected).map_err(error)?;
    let mut moved: Vec<(tauri::Webview, tauri::Window)> = vec![];
    let move_result = (|| {
        for panel in Panel::ALL {
            if before.host(panel) == next.host(panel) {
                continue;
            }
            let view = app
                .get_webview(panel.label())
                .ok_or_else(|| error("panel_error"))?;
            let previous = view.window();
            let target = app
                .get_window(next.host(panel))
                .ok_or_else(|| error("panel_error"))?;
            refresh_metrics(app, &target)?;
            moved.push((view.clone(), previous));
            view.reparent(&target).map_err(native)?;
        }
        Ok(())
    })();
    if let Err(e) = move_result {
        for (view, previous) in moved.into_iter().rev() {
            let _ = view.reparent(&previous);
        }
        return Err(e);
    }
    *workspace.state.lock().expect("layout state") = next.clone();
    let result = (|| {
        apply_bounds(app)?;
        for panel in Panel::ALL {
            if let Some(window) = app.get_window(panel.window()) {
                if next.floating_groups.iter().any(|g| g.host == panel) {
                    if !window.is_visible().map_err(native)? {
                        window.show().map_err(native)?;
                    }
                    let shown = window.clone();
                    let handle = app.clone();
                    app.run_on_main_thread(move || {
                        handle.state::<Workspace>().update_inset(&shown);
                        handle
                            .state::<super::window_placement::Store>()
                            .capture(&shown);
                        super::window_placement::schedule_save(&handle);
                    })
                    .map_err(native)?;
                } else {
                    window.hide().map_err(native)?;
                }
            }
        }
        if let Some(panel) = focus.filter(|p| before.host(*p) != next.host(*p))
            && let Some(window) = app.get_window(next.host(panel))
        {
            window.set_focus().map_err(native)?;
        }
        workspace.save()
    })();
    notify(app);
    result?;
    Ok(next)
}
fn layout_authorize(view: &tauri::Webview) -> Result<(), UiError> {
    if matches!(view.label(), "split-0" | "split-1" | "split-2") {
        Ok(())
    } else {
        super::authorize(view)
    }
}
fn panel_authorize(view: &tauri::Webview, panel: Panel) -> Result<(), UiError> {
    let source = Panel::from_label(view.label()).ok_or_else(|| error("forbidden"))?;
    if view
        .state::<Workspace>()
        .snapshot()
        .tabs(source)
        .contains(&panel)
    {
        Ok(())
    } else {
        Err(error("forbidden"))
    }
}
#[tauri::command]
pub fn workspace_state(webview: tauri::Webview) -> Result<LayoutState, UiError> {
    if webview.label() == "comparison" {
        return Ok(webview.state::<Workspace>().snapshot());
    }
    layout_authorize(&webview)?;
    resize(webview.app_handle());
    redraw(webview.app_handle());
    Ok(webview.state::<Workspace>().snapshot())
}
#[tauri::command]
pub async fn workspace_command(
    webview: tauri::Webview,
    command: Command,
    expected_revision: u64,
) -> Result<LayoutState, UiError> {
    match &command {
        Command::Dock { panel, .. }
        | Command::Select { panel }
        | Command::Return { panel }
        | Command::FloatGroup { panel }
        | Command::ReturnGroup { panel } => panel_authorize(&webview, *panel)?,
        Command::Locale { .. } | Command::Reset => {
            if webview.label() != "toolbar" {
                return Err(error("forbidden"));
            }
        }
        Command::Resize { .. } => return Err(error("forbidden")),
    }
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || execute(&app, command, expected_revision))
        .await
        .map_err(|_| error("internal_error"))?
}
/// Captured pointer coordinates are relative to the sender WebView; convert once in Rust.
fn pointer_in_main(view: &tauri::Webview, pointer: Option<[f64; 2]>) -> Result<[f64; 2], UiError> {
    let pointer = pointer
        .filter(|p| p.iter().all(|v| v.is_finite() && v.abs() < 1e6))
        .ok_or_else(|| error("invalid_layout"))?;
    let home = view
        .app_handle()
        .get_window("main")
        .ok_or_else(|| error("panel_error"))?;
    let source = view.window();
    let source_origin = source.inner_position().map_err(native)?;
    let offset = view.position().map_err(native)?;
    let scale = source.scale_factor().map_err(native)?;
    let origin = home.inner_position().map_err(native)?;
    Ok([
        source_origin.x as f64 + offset.x as f64 + pointer[0] * scale - origin.x as f64,
        source_origin.y as f64 + offset.y as f64 + pointer[1] * scale - origin.y as f64,
    ])
}
fn geometry(app: &tauri::AppHandle) -> Result<super::layout::Geometry, UiError> {
    let home = app.get_window("main").ok_or_else(|| error("panel_error"))?;
    let size = app.state::<Workspace>().size(&home).map_err(native)?;
    let workspace = app.state::<Workspace>();
    Ok(workspace.effective().geometry(
        [size.width, size.height],
        home.scale_factor().map_err(native)?,
        workspace.inset("main"),
    ))
}
#[tauri::command]
pub async fn panel_drag(
    webview: tauri::Webview,
    panel: Panel,
    phase: String,
    pointer: Option<[f64; 2]>,
    expected_revision: u64,
) -> Result<Option<Destination>, UiError> {
    panel_authorize(&webview, panel)?;
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if phase == "cancel" {
            let _ = app.emit("symbio://dock-preview", Option::<Destination>::None);
            return Ok(None);
        }
        if !matches!(phase.as_str(), "preview" | "drop") {
            return Err(error("invalid_layout"));
        }
        if app.state::<Workspace>().snapshot().revision != expected_revision {
            return Err(error("layout_conflict"));
        }
        let destination = geometry(&app)?.drop_target(pointer_in_main(&webview, pointer)?, panel);
        if phase == "drop" {
            let _ = app.emit("symbio://dock-preview", Option::<Destination>::None);
            if let Some(d) = destination {
                execute(
                    &app,
                    Command::Dock {
                        panel,
                        target: d.target,
                        zone: d.zone,
                    },
                    expected_revision,
                )?;
            }
        } else {
            let _ = app.emit("symbio://dock-preview", destination);
        }
        Ok(destination)
    })
    .await
    .map_err(|_| error("internal_error"))?
}
#[tauri::command]
pub async fn split_resize(
    webview: tauri::Webview,
    id: u32,
    phase: String,
    pointer: Option<[f64; 2]>,
    ratio: Option<f64>,
    expected_revision: u64,
) -> Result<LayoutState, UiError> {
    let slot = match webview.label() {
        "split-0" => 0,
        "split-1" => 1,
        _ => return Err(error("forbidden")),
    };
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let workspace = app.state::<Workspace>();
        let _guard = workspace.begin()?;
        if phase == "cancel" {
            let mut preview = workspace.preview.lock().expect("split preview");
            if preview.as_ref().is_some_and(|p| p.owner == webview.label()) {
                *preview = None;
            }
            drop(preview);
            apply_bounds(&app)?;
            redraw(&app);
            return Ok(workspace.snapshot());
        }
        if !matches!(phase.as_str(), "preview" | "commit") {
            return Err(error("invalid_layout"));
        }
        let state = workspace.snapshot();
        if state.revision != expected_revision {
            return Err(error("layout_conflict"));
        }
        let geometry = geometry(&app)?;
        let split = geometry
            .splits
            .get(slot)
            .filter(|s| s.id == id)
            .ok_or_else(|| error("layout_conflict"))?;
        let existing = workspace.preview.lock().expect("split preview").clone();
        if existing
            .as_ref()
            .is_some_and(|p| p.owner != webview.label() || p.id != id)
        {
            return Err(error("panel_busy"));
        }
        // The divider itself moves during preview. Freeze its input coordinate space
        // so delayed pointer events cannot add the new origin to an old event twice.
        let origin = if let Some(p) = &existing {
            p.origin
        } else {
            pointer_in_main(&webview, Some([0., 0.]))?
        };
        let scale = if let Some(p) = &existing {
            p.scale
        } else {
            webview.window().scale_factor().map_err(native)?
        };
        let ratio = if let Some(r) = ratio {
            r
        } else {
            let pointer = pointer
                .filter(|p| p.iter().all(|v| v.is_finite() && v.abs() < 1e6))
                .ok_or_else(|| error("invalid_layout"))?;
            let p = [
                origin[0] + pointer[0] * scale,
                origin[1] + pointer[1] * scale,
            ];
            let k = if split.axis == Axis::Horizontal { 0 } else { 1 };
            let gap = split.rect[k + 2] as f64;
            let room = (split.parent[k + 2] as f64 - gap).max(1.);
            ((p[k] - split.parent[k] as f64 - gap / 2.) / room).clamp(0.1, 0.9)
        };
        let next = state
            .apply(Command::Resize { id, ratio }, expected_revision)
            .map_err(error)?;
        if phase == "preview" {
            *workspace.preview.lock().expect("split preview") = Some(SplitPreview {
                origin,
                scale,
                owner: webview.label().into(),
                id,
                ratio,
                revision: expected_revision,
            });
            apply_bounds(&app)?;
            redraw(&app);
            Ok(state)
        } else {
            *workspace.preview.lock().expect("split preview") = None;
            *workspace.state.lock().expect("layout state") = next.clone();
            let result = apply_bounds(&app).and_then(|_| workspace.save());
            notify(&app);
            result?;
            Ok(next)
        }
    })
    .await
    .map_err(|_| error("internal_error"))?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preferences_roundtrip_and_unknown_versions_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("layout.json");
        let w = Workspace::load(path.clone());
        let next = w
            .snapshot()
            .apply(
                Command::Dock {
                    panel: Panel::Notebook,
                    target: None,
                    zone: super::super::layout::Zone::Floating,
                },
                0,
            )
            .unwrap();
        *w.state.lock().unwrap() = next;
        w.save().unwrap();
        let restored = Workspace::load(path.clone()).snapshot();
        assert!(restored.floating.contains(&Panel::Notebook));
        assert_eq!(restored.revision, 0);
        let invalid = b"{\"version\":99}";
        std::fs::write(&path, invalid).unwrap();
        Workspace::load(path.clone()).save().unwrap();
        assert_eq!(std::fs::read(path).unwrap(), invalid);
    }
    #[test]
    fn split_preview_never_persists_or_consumes_revision_and_stale_preview_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("layout.json");
        let w = Workspace::load(path.clone());
        let before = w.snapshot();
        *w.preview.lock().unwrap() = Some(SplitPreview {
            owner: "split-0".into(),
            id: 1,
            ratio: 0.6,
            revision: 0,
            origin: [280., 88.],
            scale: 1.,
        });
        assert_ne!(w.effective().root, before.root);
        assert_eq!(w.snapshot(), before);
        w.save().unwrap();
        assert_eq!(Workspace::load(path).snapshot(), before);
        w.preview.lock().unwrap().as_mut().unwrap().revision = 1;
        assert_eq!(w.effective(), before);
        *w.preview.lock().unwrap() = None;
        assert_eq!(w.effective(), before);
    }
    #[test]
    fn transition_guard_rejects_reentry_and_releases() {
        let dir = tempfile::tempdir().unwrap();
        let w = Workspace::load(dir.path().join("layout.json"));
        let guard = w.begin().unwrap();
        assert!(w.begin().is_err());
        drop(guard);
        assert!(w.begin().is_ok());
    }
}
