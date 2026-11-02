//! Application-owned CAS configuration. Research documents never select executables.
use super::{UiError, authorize, error};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::PathBuf,
    sync::Mutex,
};
use tauri::{Emitter, Manager};
#[derive(Default)]
pub struct Settings(Mutex<()>);
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Preference {
    python: Option<PathBuf>,
}
fn path(app: &tauri::AppHandle) -> Result<PathBuf, UiError> {
    Ok(app
        .path()
        .app_config_dir()
        .map_err(|_| error("io_error"))?
        .join("cas-runtime.json"))
}
pub fn initialize(app: &tauri::AppHandle) {
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let p = path(app).map_err(|_| "CAS configuration path")?;
        if !p.exists() {
            return Ok(());
        }
        let mut bytes = Vec::new();
        std::fs::File::open(p)?.take(8193).read_to_end(&mut bytes)?;
        if bytes.len() > 8192 {
            return Err("CAS configuration size".into());
        }
        let preference: Preference = serde_json::from_slice(&bytes)?;
        if preference.python.as_ref().is_some_and(|p| !p.is_absolute()) {
            return Err("CAS absolute path required".into());
        }
        symbio_core::cas::configure(preference.python, None);
        Ok(())
    })();
    if result.is_err() {
        eprintln!("Symbio: CAS preferences unavailable; no configuration was overwritten");
    }
}
#[tauri::command]
pub fn cas_state(webview: tauri::Webview) -> Result<symbio_core::cas::State, UiError> {
    if webview.label() != "comparison" {
        authorize(&webview)?;
    }
    Ok(symbio_core::cas::state())
}
#[tauri::command]
pub async fn configure_cas(
    webview: tauri::Webview,
    python: Option<String>,
) -> Result<symbio_core::cas::State, UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let settings = app.state::<Settings>();
        let _guard = settings.0.lock().map_err(|_| error("internal_error"))?;
        if python.as_ref().is_some_and(|s| s.len() > 4096) {
            return Err(error("invalid_project"));
        }
        let python = python.map(PathBuf::from);
        let version = python
            .as_ref()
            .map(|p| symbio_core::cas::probe(p))
            .transpose()?;
        let destination = path(&app)?;
        let parent = destination.parent().unwrap();
        std::fs::create_dir_all(parent).map_err(symbio_core::Error::from)?;
        let mut file = tempfile::NamedTempFile::new_in(parent).map_err(symbio_core::Error::from)?;
        file.write_all(
            &serde_json::to_vec(&Preference {
                python: python.clone(),
            })
            .map_err(symbio_core::Error::from)?,
        )
        .map_err(symbio_core::Error::from)?;
        file.as_file()
            .sync_all()
            .map_err(symbio_core::Error::from)?;
        file.persist(destination)
            .map_err(|e| symbio_core::Error::from(e.error))?;
        symbio_core::cas::configure(python, version);
        let state = symbio_core::cas::state();
        let _ = app.emit("symbio://cas", &state);
        Ok(state)
    })
    .await
    .map_err(|_| error("internal_error"))?
}
