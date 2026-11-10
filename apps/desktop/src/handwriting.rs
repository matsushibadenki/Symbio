//! Thin Symbio adapter; the recognizer crate has no Tauri/document dependency.
use super::{Shared, UiError, authorize, error};
use std::{path::Path, sync::Mutex};
use symbio_handwriting::{Ink, Recognition, Recognizer};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

/// Selection is read-only: configuration is loaded only by explicit Apply.
#[tauri::command]
pub async fn handwriting_pick_path(
    webview: tauri::Webview,
    kind: String,
    generation: u64,
) -> Result<Option<String>, UiError> {
    authorize(&webview)?;
    if !matches!(kind.as_str(), "models" | "runtime") {
        return Err(error("invalid_project"));
    }
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let check = || {
            app.state::<Shared>()
                .0
                .lock()
                .map_err(|_| error("internal_error"))?
                .check_generation(generation)
        };
        check()?;
        let dialog = app.dialog().file();
        let selected = if kind == "models" {
            dialog.blocking_pick_folder()
        } else {
            dialog
                .add_filter("ONNX Runtime", &["dylib", "dll", "so"])
                .blocking_pick_file()
        };
        check()?;
        selected
            .map(|file| {
                file.into_path()
                    .map(|path| path.to_string_lossy().into_owned())
                    .map_err(|_| error("io_error"))
            })
            .transpose()
    })
    .await
    .map_err(|_| error("internal_error"))?
}
#[derive(Default)]
pub struct State {
    engine: Mutex<Option<Recognizer>>,
    ink: Mutex<Option<(u64, u64, Ink)>>,
}
#[tauri::command]
pub async fn configure(
    webview: tauri::Webview,
    models: String,
    runtime: String,
    generation: u64,
) -> Result<(), UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Shared>()
            .0
            .lock()
            .map_err(|_| error("internal_error"))?
            .check_generation(generation)?;
        if !Path::new(&models).is_absolute() || !Path::new(&runtime).is_absolute() {
            return Err(error("invalid_project"));
        }
        let model =
            Recognizer::load(Path::new(&models), Path::new(&runtime)).map_err(|e| UiError {
                code: "handwriting_error".into(),
                detail: e.to_string(),
            })?;
        let session = app.state::<Shared>();
        let session = session.0.lock().map_err(|_| error("internal_error"))?;
        session.check_generation(generation)?;
        let config_dir = app.path().app_config_dir().map_err(|_| error("io_error"))?;
        std::fs::create_dir_all(&config_dir).map_err(|_| error("io_error"))?;
        use std::io::Write;
        let mut file =
            tempfile::NamedTempFile::new_in(&config_dir).map_err(|_| error("io_error"))?;
        file.write_all(
            &serde_json::to_vec(&serde_json::json!({"models":models,"runtime":runtime}))
                .map_err(|_| error("internal_error"))?,
        )
        .map_err(|_| error("io_error"))?;
        file.persist(config_dir.join("handwriting.json"))
            .map_err(|_| error("io_error"))?;
        *app.state::<State>()
            .engine
            .lock()
            .map_err(|_| error("internal_error"))? = Some(model);
        Ok(())
    })
    .await
    .map_err(|_| error("internal_error"))?
}
#[tauri::command]
pub async fn recognize(
    webview: tauri::Webview,
    revision: u64,
    generation: u64,
) -> Result<Recognition, UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Shared>()
            .0
            .lock()
            .map_err(|_| error("internal_error"))?
            .check_generation(generation)?;
        let ink = {
            let state = app.state::<State>();
            let slot = state.ink.lock().map_err(|_| error("internal_error"))?;
            let (g, r, ink) = slot
                .as_ref()
                .ok_or_else(|| error("handwriting_unavailable"))?;
            if *g != generation || *r != revision {
                return Err(error("revision_conflict"));
            }
            ink.clone()
        };
        let result = app
            .state::<State>()
            .engine
            .lock()
            .map_err(|_| error("internal_error"))?
            .as_mut()
            .ok_or_else(|| error("handwriting_unavailable"))?
            .recognize_candidates(&ink)
            .map_err(|e| UiError {
                code: "handwriting_error".into(),
                detail: e.to_string(),
            })?;
        app.state::<Shared>()
            .0
            .lock()
            .map_err(|_| error("internal_error"))?
            .check_generation(generation)?;
        let state = app.state::<State>();
        let slot = state.ink.lock().map_err(|_| error("internal_error"))?;
        if !slot
            .as_ref()
            .is_some_and(|(g, r, _)| *g == generation && *r == revision)
        {
            return Err(error("revision_conflict"));
        }
        Ok(result)
    })
    .await
    .map_err(|_| error("internal_error"))?
}

#[tauri::command]
pub fn ink_update(
    webview: tauri::Webview,
    ink: Ink,
    revision: u64,
    generation: u64,
) -> Result<(), UiError> {
    authorize(&webview)?;
    let shared = webview.state::<Shared>();
    let session = shared.0.lock().map_err(|_| error("internal_error"))?;
    session.check_generation(generation)?;
    if !ink.strokes.is_empty() {
        ink.raster().map_err(|e| UiError {
            code: "handwriting_error".into(),
            detail: e.to_string(),
        })?;
    }
    let state = webview.state::<State>();
    let mut slot = state.ink.lock().map_err(|_| error("internal_error"))?;
    if slot
        .as_ref()
        .is_some_and(|(g, r, _)| *g == generation && *r >= revision)
    {
        return Err(error("revision_conflict"));
    }
    *slot = Some((generation, revision, ink));
    Ok(())
}

#[tauri::command]
pub fn handwriting_settings(webview: tauri::Webview) -> Result<serde_json::Value, UiError> {
    authorize(&webview)?;
    let path = webview
        .app_handle()
        .path()
        .app_config_dir()
        .map_err(|_| error("io_error"))?
        .join("handwriting.json");
    if !path.exists() {
        return Ok(serde_json::json!({"models":"","runtime":""}));
    }
    if std::fs::metadata(&path)
        .map_err(|_| error("io_error"))?
        .len()
        > 16384
    {
        return Err(error("resource_limit"));
    }
    serde_json::from_slice(&std::fs::read(path).map_err(|_| error("io_error"))?)
        .map_err(|_| error("invalid_project"))
}
