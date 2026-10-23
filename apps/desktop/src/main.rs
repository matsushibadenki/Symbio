mod layout;
mod plot;
mod window_placement;
mod workspace;
use serde::Serialize;
use std::sync::Mutex;
use symbio_core::{AppCore, Project, ProjectStore, Request, Snapshot};
use tauri::{Emitter, Manager};
struct Session {
    core: AppCore,
    store: ProjectStore,
}
struct Shared(Mutex<Session>);
#[derive(Default)]
struct Drafts(Mutex<std::collections::BTreeSet<String>>);
#[tauri::command]
fn set_draft(webview: tauri::Webview, pending: bool) -> Result<(), UiError> {
    authorize(&webview)?;
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
        "toolbar" | "objects" | "notebook" | "inspector"
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
        .core
        .snapshot())
}
#[tauri::command]
async fn preview_latex(
    webview: tauri::Webview,
    source: String,
) -> Result<symbio_core::latex::Preview, UiError> {
    authorize(&webview)?;
    tauri::async_runtime::spawn_blocking(move || {
        symbio_core::latex::preview(&source).map_err(UiError::from)
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
async fn edit(webview: tauri::Webview, request: Request) -> Result<Snapshot, UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Shared>();
        let mut session = state.0.lock().map_err(|_| error("internal_error"))?;
        if matches!(
            request.command,
            symbio_core::Command::Checkout { .. }
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
        let snapshot = session.core.apply(request)?;
        let _ = app.emit("symbio://changed", &snapshot);
        Ok(snapshot)
    })
    .await
    .map_err(|_| error("internal_error"))?
}
#[tauri::command]
async fn save(webview: tauri::Webview) -> Result<Snapshot, UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    let _ = app.emit("symbio://prepare-save", ());
    tauri::async_runtime::spawn_blocking(move || {
        for _ in 0..100 {
            if app
                .state::<Drafts>()
                .0
                .lock()
                .map_err(|_| error("internal_error"))?
                .is_empty()
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        if !app
            .state::<Drafts>()
            .0
            .lock()
            .map_err(|_| error("internal_error"))?
            .is_empty()
        {
            return Err(error("draft_pending"));
        }
        let state = app.state::<Shared>();
        let mut s = state.0.lock().map_err(|_| error("internal_error"))?;
        let Session { core, store } = &mut *s;
        store.save(core.project())?;
        core.mark_saved();
        let out = core.snapshot();
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
) -> Result<serde_json::Value, UiError> {
    authorize(&webview)?;
    Ok(state
        .0
        .lock()
        .map_err(|_| error("internal_error"))?
        .core
        .inspect(id)?)
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
        .manage(plot::Plot::default())
        .manage(Drafts::default())
        .setup(|app| {
            let args: Vec<_> = std::env::args().skip(1).collect();
            let path = if args.len() == 2 && args[0] == "--project" {
                std::path::PathBuf::from(&args[1])
            } else if args.is_empty() {
                let dir = app.path().app_data_dir()?;
                std::fs::create_dir_all(&dir)?;
                dir.join("workspace.symbio")
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
            app.manage(Shared(Mutex::new(Session {
                core: AppCore::new(project)?,
                store,
            })));
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
            preview_latex,
            convert_source,
            set_draft,
            snapshot,
            edit,
            save,
            inspect,
            export_document,
            plot::show_plot,
            workspace::workspace_state,
            workspace::workspace_command,
            workspace::split_resize,
            workspace::panel_drag
        ])
        .on_window_event(|window, event| {
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
                if window.state::<workspace::Workspace>().busy() {
                    api.prevent_close();
                    return;
                }
                if window
                    .state::<Shared>()
                    .0
                    .lock()
                    .map(|s| {
                        s.core.dirty()
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
