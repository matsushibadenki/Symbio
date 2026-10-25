mod axes3d;
mod files;
mod layout;
mod plot;
mod surface3d;
mod surface_gpu;
mod window_placement;
mod workspace;
use files::{DocumentSnapshot as Snapshot, Session};
use serde::Serialize;
use std::sync::Mutex;
use symbio_core::{AppCore, Project, ProjectStore, Request};
use tauri::{Emitter, Manager};
struct Shared(Mutex<Session>);
#[derive(Default)]
struct Drafts(Mutex<std::collections::BTreeSet<String>>);
#[tauri::command]
fn set_draft(webview: tauri::Webview, pending: bool, generation: u64) -> Result<(), UiError> {
    authorize(&webview)?;
    let shared = webview.state::<Shared>();
    let session = shared.0.lock().map_err(|_| error("internal_error"))?;
    session.check_generation(generation)?;
    let state = webview.state::<Drafts>();
    let mut views = state.0.lock().map_err(|_| error("internal_error"))?;
    if pending {
        views.insert(webview.label().into());
    } else {
        views.remove(webview.label());
    }
    Ok(())
}
#[derive(Debug, Serialize)]
struct UiError {
    code: String,
    detail: String,
}
fn error(code: &str) -> UiError {
    UiError {
        code: code.into(),
        detail: String::new(),
    }
}
impl From<symbio_core::Error> for UiError {
    fn from(e: symbio_core::Error) -> Self {
        Self {
            code: e.code().into(),
            detail: e.to_string(),
        }
    }
}
fn authorize(view: &tauri::Webview) -> Result<(), UiError> {
    if matches!(
        view.label(),
        "toolbar" | "objects" | "notebook" | "inspector" | "dependencies" | "surface"
    ) {
        Ok(())
    } else {
        Err(error("forbidden"))
    }
}
#[tauri::command]
fn snapshot(webview: tauri::Webview, state: tauri::State<Shared>) -> Result<Snapshot, UiError> {
    authorize(&webview)?;
    Ok(state
        .0
        .lock()
        .map_err(|_| error("internal_error"))?
        .snapshot())
}
#[tauri::command]
fn show_dependencies(webview: tauri::Webview) -> Result<(), UiError> {
    authorize(&webview)?;
    let app = webview.app_handle();
    if let Some(window) = app.get_webview_window("dependencies") {
        window.show().map_err(|_| error("panel_error"))?;
        window.set_focus().map_err(|_| error("panel_error"))?;
    } else {
        let locale = app.state::<workspace::Workspace>().snapshot().locale;
        let title = match locale.as_str() {
            "ja" => "Symbio · セルの依存関係",
            "zh-CN" => "Symbio · 单元依赖关系",
            _ => "Symbio · Cell dependencies",
        };
        tauri::WebviewWindowBuilder::new(
            app,
            "dependencies",
            tauri::WebviewUrl::App("index.html?view=dependencies".into()),
        )
        .title(title)
        .inner_size(960., 700.)
        .min_inner_size(480., 400.)
        .center()
        .build()
        .map_err(|_| error("panel_error"))?;
    }
    Ok(())
}
/// Cross-panel navigation is intentionally limited to revealing the notebook.
/// General dock mutations remain scoped to their owning panel.
#[tauri::command]
async fn reveal_notebook(
    webview: tauri::Webview,
    generation: u64,
) -> Result<layout::LayoutState, UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Shared>()
            .0
            .lock()
            .map_err(|_| error("internal_error"))?
            .check_generation(generation)?;
        let revision = app.state::<workspace::Workspace>().snapshot().revision;
        workspace::execute(
            &app,
            layout::Command::Select {
                panel: layout::Panel::Notebook,
            },
            revision,
        )
    })
    .await
    .map_err(|_| error("internal_error"))?
}
#[tauri::command]
async fn preview_latex(
    webview: tauri::Webview,
    source: String,
    id: Option<u64>,
    generation: u64,
) -> Result<symbio_core::latex::Preview, UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Shared>();
        let session = state.0.lock().map_err(|_| error("internal_error"))?;
        session.check_generation(generation)?;
        session
            .core
            .preview_latex(&source, id)
            .map_err(UiError::from)
    })
    .await
    .map_err(|_| error("internal_error"))?
}
#[tauri::command]
async fn convert_source(
    webview: tauri::Webview,
    source: String,
    from_format: symbio_core::SourceFormat,
    to_format: symbio_core::SourceFormat,
) -> Result<String, UiError> {
    authorize(&webview)?;
    tauri::async_runtime::spawn_blocking(move || {
        symbio_core::latex::convert(&source, from_format, to_format).map_err(UiError::from)
    })
    .await
    .map_err(|_| error("internal_error"))?
}
#[tauri::command]
async fn edit(
    webview: tauri::Webview,
    request: Request,
    generation: u64,
) -> Result<Snapshot, UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Shared>();
        let mut session = state.0.lock().map_err(|_| error("internal_error"))?;
        session.check_generation(generation)?;
        if matches!(
            request.command,
            symbio_core::Command::Compact {}
                | symbio_core::Command::Recalculate { .. }
                | symbio_core::Command::Checkout { .. }
                | symbio_core::Command::Branch { .. }
                | symbio_core::Command::Undo
                | symbio_core::Command::Redo
                | symbio_core::Command::Remove { .. }
        ) && !app
            .state::<Drafts>()
            .0
            .lock()
            .map_err(|_| error("internal_error"))?
            .is_empty()
        {
            return Err(error("draft_pending"));
        }
        session.core.apply(request)?;
        let snapshot = session.snapshot();
        let _ = app.emit("symbio://changed", &snapshot);
        drop(session);
        surface3d::refresh(&app);
        Ok(snapshot)
    })
    .await
    .map_err(|_| error("internal_error"))?
}
#[tauri::command]
async fn save(webview: tauri::Webview, generation: u64) -> Result<Snapshot, UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let op = app.state::<files::FileOperation>();
        let _guard = op.acquire()?;
        files::prepare(&app)?;
        let state = app.state::<Shared>();
        let mut s = state.0.lock().map_err(|_| error("internal_error"))?;
        s.check_generation(generation)?;
        s.save()?;
        let out = s.snapshot();
        let _ = app.emit("symbio://changed", &out);
        Ok(out)
    })
    .await
    .map_err(|_| error("internal_error"))?
}
#[tauri::command]
fn inspect(
    webview: tauri::Webview,
    state: tauri::State<Shared>,
    id: u64,
    generation: u64,
) -> Result<serde_json::Value, UiError> {
    authorize(&webview)?;
    let session = state.0.lock().map_err(|_| error("internal_error"))?;
    session.check_generation(generation)?;
    Ok(session.core.inspect(id)?)
}
#[tauri::command]
fn export_document(
    webview: tauri::Webview,
    state: tauri::State<Shared>,
) -> Result<String, UiError> {
    authorize(&webview)?;
    serde_json::to_string_pretty(
        state
            .0
            .lock()
            .map_err(|_| error("internal_error"))?
            .core
            .project(),
    )
    .map_err(|_| error("internal_error"))
}
fn main() {
    let result = tauri::Builder::default()
        .menu(|app| {
            use tauri::menu::{Menu, MenuItem, MenuItemKind};
            let menu = Menu::default(app)?;
            // AppKit's predefined Quit calls terminate: directly, bypassing ExitRequested.
            // Preserve the standard editing/window menus, but route Quit through AppHandle.
            for entry in menu.items()? {
                if let MenuItemKind::Submenu(submenu) = entry {
                    for (position, item) in submenu.items()?.into_iter().enumerate() {
                        if let MenuItemKind::Predefined(predefined) = item
                            && predefined.text()?.starts_with("Quit")
                        {
                            submenu.remove(&predefined)?;
                            submenu.insert(
                                &MenuItem::with_id(
                                    app,
                                    "symbio-quit",
                                    "Quit Symbio",
                                    true,
                                    Some("CmdOrCtrl+Q"),
                                )?,
                                position,
                            )?;
                        }
                    }
                }
            }
            Ok(menu)
        })
        .on_menu_event(|app, event| {
            if event.id().as_ref() == "symbio-quit" {
                app.exit(0);
            }
        })
        .plugin(tauri_plugin_dialog::init())
        .manage(files::FileOperation::default())
        .manage(plot::Plot::default())
        .manage(surface3d::Surface3d::default())
        .manage(Drafts::default())
        .setup(|app| {
            let args: Vec<_> = std::env::args().skip(1).collect();
            let path = if args.len() == 2 && args[0] == "--project" {
                std::path::PathBuf::from(&args[1])
            } else if args.is_empty() {
                let dir = app.path().app_data_dir()?;
                std::fs::create_dir_all(&dir)?;
                files::last_project(app.handle()).unwrap_or_else(|| dir.join("workspace.symbio"))
            } else {
                return Err("usage: symbio-desktop [--project /path/project.symbio]".into());
            };
            let mut store = ProjectStore::open(path)?;
            let project = match store.load()? {
                Some(p) => p,
                None => {
                    let p = Project::new("Polynomial study")?;
                    store.save(&p)?;
                    p
                }
            };
            app.manage(Shared(Mutex::new(Session::new(
                AppCore::new(project)?,
                store,
            ))));
            let path = std::env::var_os("SYMBIO_LAYOUT_PATH")
                .map(std::path::PathBuf::from)
                .unwrap_or(app.path().app_config_dir()?.join("dock-layout.json"));
            app.manage(window_placement::Store::load(Some(
                path.with_extension("windows.json"),
            )));
            app.manage(workspace::Workspace::load(path));
            workspace::initialize(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            reveal_notebook,
            preview_latex,
            show_dependencies,
            convert_source,
            set_draft,
            snapshot,
            edit,
            save,
            files::switch_document,
            files::close_document,
            files::new_document,
            files::open_document,
            files::save_as,
            inspect,
            export_document,
            plot::show_plot,
            surface3d::surface_state,
            surface3d::surface_command,
            workspace::workspace_state,
            workspace::workspace_command,
            workspace::split_resize,
            workspace::panel_drag
        ])
        .on_window_event(|window, event| {
            if window.label() == "dependencies" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event
                    && window
                        .state::<Drafts>()
                        .0
                        .lock()
                        .map(|drafts| drafts.contains("dependencies"))
                        .unwrap_or(true)
                {
                    api.prevent_close();
                    let _ = window.app_handle().emit("symbio://close-blocked", ());
                }
                if matches!(event, tauri::WindowEvent::Destroyed)
                    && let Ok(mut drafts) = window.state::<Drafts>().0.lock()
                {
                    drafts.remove("dependencies");
                }
                return;
            }
            if window.label() == "plot" {
                plot::event(window, event);
                return;
            }
            if matches!(
                event,
                tauri::WindowEvent::Resized(_)
                    | tauri::WindowEvent::ScaleFactorChanged { .. }
                    | tauri::WindowEvent::Focused(true)
            ) && window.try_state::<workspace::Workspace>().is_some()
            {
                window.state::<workspace::Workspace>().update_inset(window);
                workspace::resize(window.app_handle());
            }
            if matches!(
                event,
                tauri::WindowEvent::Moved(_)
                    | tauri::WindowEvent::Resized(_)
                    | tauri::WindowEvent::Focused(true)
                    | tauri::WindowEvent::ScaleFactorChanged { .. }
            ) && let Some(store) = window.try_state::<window_placement::Store>()
            {
                store.capture(window);
                window_placement::schedule_save(window.app_handle());
            }
            if let Some(host) = layout::Panel::ALL
                .into_iter()
                .find(|p| p.window() == window.label())
                && let tauri::WindowEvent::CloseRequested { api, .. } = event
            {
                api.prevent_close();
                let app = window.app_handle().clone();
                tauri::async_runtime::spawn_blocking(move || {
                    let state = app.state::<workspace::Workspace>().snapshot();
                    if let Some(group) = state.floating_groups.iter().find(|g| g.host == host)
                        && let Err(e) = workspace::execute(
                            &app,
                            layout::Command::ReturnGroup {
                                panel: group.active,
                            },
                            state.revision,
                        )
                    {
                        let _ = app.emit("symbio://panel-error", e.code);
                    }
                });
            }
            if window.label() == "main"
                && let tauri::WindowEvent::CloseRequested { api, .. } = event
            {
                if window.state::<workspace::Workspace>().busy()
                    || window.state::<files::FileOperation>().busy()
                {
                    api.prevent_close();
                    return;
                }
                if window
                    .state::<Shared>()
                    .0
                    .lock()
                    .map(|s| {
                        s.has_unsaved_changes()
                            || !window
                                .state::<Drafts>()
                                .0
                                .lock()
                                .map(|s| s.is_empty())
                                .unwrap_or(false)
                    })
                    .unwrap_or(true)
                {
                    api.prevent_close();
                    let _ = window.app_handle().emit("symbio://close-blocked", ());
                } else {
                    window.app_handle().exit(0);
                }
            }
        })
        .build(tauri::generate_context!());
    match result {
        Ok(app) => app.run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = &event {
                let pending = app
                    .state::<Shared>()
                    .0
                    .lock()
                    .map(|s| {
                        s.has_unsaved_changes()
                            || !app
                                .state::<Drafts>()
                                .0
                                .lock()
                                .map(|d| d.is_empty())
                                .unwrap_or(false)
                    })
                    .unwrap_or(true);
                if pending
                    || app.state::<files::FileOperation>().busy()
                    || app.state::<workspace::Workspace>().busy()
                {
                    api.prevent_exit();
                    let _ = app.emit("symbio://close-blocked", ());
                }
            }
            if matches!(event, tauri::RunEvent::Exit) {
                app.state::<window_placement::Store>().save();
            }
        }),
        Err(e) => {
            eprintln!("Symbio: {e}");
            std::process::exit(1)
        }
    }
}
