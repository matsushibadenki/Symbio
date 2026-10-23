//! Native file dialogs; the shared Rust session owns the active document and lock.
use super::{Drafts, Shared, UiError, authorize, error};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
use symbio_core::{AppCore, ProjectStore};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

pub struct Session {
    pub core: AppCore,
    pub store: ProjectStore,
    generation: u64,
}
#[derive(Serialize, Clone)]
pub struct DocumentSnapshot {
    #[serde(flatten)]
    snapshot: symbio_core::Snapshot,
    generation: u64,
    path: String,
}
impl Session {
    pub fn new(core: AppCore, store: ProjectStore) -> Self {
        Self {
            core,
            store,
            generation: 1,
        }
    }
    pub fn snapshot(&self) -> DocumentSnapshot {
        DocumentSnapshot {
            snapshot: self.core.snapshot(),
            generation: self.generation,
            path: self.store.path().to_string_lossy().into_owned(),
        }
    }
    pub fn check_generation(&self, generation: u64) -> Result<(), UiError> {
        if generation == self.generation {
            Ok(())
        } else {
            Err(error("document_changed"))
        }
    }
    pub fn save(&mut self) -> Result<(), UiError> {
        self.store.save(self.core.project())?;
        self.core.mark_saved();
        Ok(())
    }
    fn replace(&mut self, core: AppCore, store: ProjectStore) -> Result<(), UiError> {
        let next = self
            .generation
            .checked_add(1)
            .ok_or_else(|| error("resource_limit"))?;
        self.core = core;
        self.store = store;
        self.generation = next;
        Ok(())
    }
    fn save_as(&mut self, path: &Path) -> Result<(), UiError> {
        if same_path(path, self.store.path()) {
            return self.save();
        }
        let next = self
            .generation
            .checked_add(1)
            .ok_or_else(|| error("resource_limit"))?;
        let mut destination = ProjectStore::open(path)?;
        destination.save(self.core.project())?;
        // Publish the new location only after a successful atomic save.
        self.store = destination;
        self.generation = next;
        self.core.mark_saved();
        Ok(())
    }
}
fn same_path(a: &Path, b: &Path) -> bool {
    a.parent()
        .and_then(|p| p.canonicalize().ok())
        .zip(a.file_name())
        .is_some_and(|(parent, name)| parent.join(name) == b)
}
#[derive(Default)]
pub struct FileOperation(AtomicBool);
pub struct Guard<'a>(&'a AtomicBool);
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
impl FileOperation {
    pub fn busy(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    pub fn acquire(&self) -> Result<Guard<'_>, UiError> {
        self.0
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| error("file_busy"))?;
        Ok(Guard(&self.0))
    }
}
fn no_drafts(app: &tauri::AppHandle) -> Result<(), UiError> {
    if app
        .state::<Drafts>()
        .0
        .lock()
        .map_err(|_| error("internal_error"))?
        .is_empty()
    {
        Ok(())
    } else {
        Err(error("draft_pending"))
    }
}
pub fn prepare(app: &tauri::AppHandle) -> Result<(), UiError> {
    let _ = app.emit("symbio://prepare-save", ());
    for _ in 0..100 {
        if no_drafts(app).is_ok() {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    no_drafts(app)
}
struct Freeze(tauri::AppHandle);
impl Freeze {
    fn new(app: &tauri::AppHandle) -> Self {
        let _ = app.emit("symbio://file-busy", true);
        Self(app.clone())
    }
}
impl Drop for Freeze {
    fn drop(&mut self) {
        let _ = self.0.emit("symbio://file-busy", false);
    }
}
fn labels(app: &tauri::AppHandle) -> [&'static str; 4] {
    match app
        .state::<super::workspace::Workspace>()
        .snapshot()
        .locale
        .as_str()
    {
        "ja" => [
            "開く",
            "名前を付けて保存",
            "現在の文書を保存してから開きますか？",
            "保存して開く",
        ],
        "zh-CN" => ["打开", "另存为", "是否保存当前文档后再打开？", "保存并打开"],
        _ => [
            "Open",
            "Save As",
            "Save the current document before opening another?",
            "Save and Open",
        ],
    }
}
fn remember(app: &tauri::AppHandle, path: &Path) {
    // Preference failure must not turn a successfully saved document into an error.
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        use std::io::Write;
        let dir = app.path().app_config_dir()?;
        std::fs::create_dir_all(&dir)?;
        let mut file = tempfile::NamedTempFile::new_in(&dir)?;
        file.write_all(&serde_json::to_vec(path)?)?;
        file.persist(dir.join("last-project.json"))?;
        Ok(())
    })();
    if let Err(e) = result {
        eprintln!("Symbio: could not remember project location: {e}");
    }
}
pub fn last_project(app: &tauri::AppHandle) -> Option<PathBuf> {
    let path: PathBuf = serde_json::from_slice(
        &std::fs::read(app.path().app_config_dir().ok()?.join("last-project.json")).ok()?,
    )
    .ok()?;
    // Never silently create an empty replacement for a moved or deleted document.
    path.is_file().then_some(path)
}
#[tauri::command]
pub async fn save_as(
    webview: tauri::Webview,
    generation: u64,
) -> Result<Option<DocumentSnapshot>, UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let op = app.state::<FileOperation>();
        let _guard = op.acquire()?;
        prepare(&app)?;
        let _freeze = Freeze::new(&app);
        let path = {
            let state = app.state::<Shared>();
            let session = state.0.lock().map_err(|_| error("internal_error"))?;
            session.check_generation(generation)?;
            session.store.path().to_path_buf()
        };
        let mut dialog = app
            .dialog()
            .file()
            .set_title(labels(&app)[1])
            .add_filter("Symbio", &["symbio"])
            .set_directory(path.parent().unwrap())
            .set_file_name(path.file_name().unwrap().to_string_lossy());
        if let Some(window) = app.get_window("main") {
            dialog = dialog.set_parent(&window);
        }
        let Some(file) = dialog.blocking_save_file() else {
            return Ok(None);
        };
        let mut path = file.into_path().map_err(|_| error("io_error"))?;
        if path.extension().is_none() {
            path.set_extension("symbio");
        }
        let state = app.state::<Shared>();
        let mut session = state.0.lock().map_err(|_| error("internal_error"))?;
        session.check_generation(generation)?;
        no_drafts(&app)?;
        session.save_as(&path)?;
        remember(&app, session.store.path());
        let out = session.snapshot();
        let _ = app.emit("symbio://changed", &out);
        Ok(Some(out))
    })
    .await
    .map_err(|_| error("internal_error"))?
}
#[tauri::command]
pub async fn open_document(
    webview: tauri::Webview,
    generation: u64,
) -> Result<Option<DocumentSnapshot>, UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let op = app.state::<FileOperation>();
        let _guard = op.acquire()?;
        prepare(&app)?;
        let _freeze = Freeze::new(&app);
        let path = {
            let state = app.state::<Shared>();
            let session = state.0.lock().map_err(|_| error("internal_error"))?;
            session.check_generation(generation)?;
            session.store.path().to_path_buf()
        };
        let mut dialog = app
            .dialog()
            .file()
            .set_title(labels(&app)[0])
            .add_filter("Symbio", &["symbio"])
            .set_directory(path.parent().unwrap());
        if let Some(window) = app.get_window("main") {
            dialog = dialog.set_parent(&window);
        }
        let Some(file) = dialog.blocking_pick_file() else {
            return Ok(None);
        };
        let path = file.into_path().map_err(|_| error("io_error"))?;
        if same_path(&path, &path_for(&app)?) {
            return Ok(None);
        }
        // Load and validate first: a broken or locked target cannot disturb this session.
        let mut store = ProjectStore::open(&path)?;
        let project = store.load()?.ok_or_else(|| error("io_error"))?;
        let core = AppCore::new(project)?;
        let (dirty, revision) = {
            let state = app.state::<Shared>();
            let session = state.0.lock().map_err(|_| error("internal_error"))?;
            session.check_generation(generation)?;
            no_drafts(&app)?;
            (session.core.dirty(), session.core.project().revision)
        };
        if dirty {
            let text = labels(&app);
            let cancel = match app
                .state::<super::workspace::Workspace>()
                .snapshot()
                .locale
                .as_str()
            {
                "ja" => "キャンセル",
                "zh-CN" => "取消",
                _ => "Cancel",
            };
            let mut dialog = app.dialog().message(text[2]).title("Symbio").buttons(
                MessageDialogButtons::OkCancelCustom(text[3].into(), cancel.into()),
            );
            if let Some(window) = app.get_window("main") {
                dialog = dialog.parent(&window);
            }
            if !dialog.blocking_show() {
                return Ok(None);
            }
        }
        let out = {
            let state = app.state::<Shared>();
            let mut session = state.0.lock().map_err(|_| error("internal_error"))?;
            session.check_generation(generation)?;
            no_drafts(&app)?;
            if session.core.project().revision != revision {
                return Err(error("revision_conflict"));
            }
            if dirty {
                session.save()?;
            }
            session.replace(core, store)?;
            remember(&app, session.store.path());
            session.snapshot()
        };
        let _ = app.emit("symbio://changed", &out);
        super::surface3d::document_changed(&app);
        Ok(Some(out))
    })
    .await
    .map_err(|_| error("internal_error"))?
}
fn path_for(app: &tauri::AppHandle) -> Result<PathBuf, UiError> {
    Ok(app
        .state::<Shared>()
        .0
        .lock()
        .map_err(|_| error("internal_error"))?
        .store
        .path()
        .to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use symbio_core::{Command, Project, Request};
    fn session(dir: &Path) -> Session {
        let mut store = ProjectStore::open(dir.join("original.symbio")).unwrap();
        let project = Project::new("Original").unwrap();
        store.save(&project).unwrap();
        Session::new(AppCore::new(project).unwrap(), store)
    }
    fn rename(s: &mut Session, name: &str) {
        s.core
            .apply(Request {
                expected_revision: s.core.project().revision,
                actor: "human".into(),
                command: Command::Rename { name: name.into() },
            })
            .unwrap();
    }
    #[test]
    fn save_as_preserves_source_and_undo_then_saves_to_new_location() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = session(dir.path());
        let original = std::fs::read(s.store.path()).unwrap();
        rename(&mut s, "Changed");
        let path = dir.path().join("copy.symbio");
        s.save_as(&path).unwrap();
        assert!(!s.core.dirty());
        assert!(s.core.snapshot().can_undo);
        assert_eq!(
            std::fs::read(dir.path().join("original.symbio")).unwrap(),
            original
        );
        assert_eq!(s.store.path(), path);
        assert!(s.check_generation(1).is_err());
        assert!(s.check_generation(2).is_ok());
        rename(&mut s, "New destination");
        s.save().unwrap();
        let p: Project = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(p.name, "New destination");
        assert_eq!(
            std::fs::read(dir.path().join("original.symbio")).unwrap(),
            original
        );
    }
    #[test]
    fn failed_save_as_keeps_active_location_dirty_state_and_destination() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = session(dir.path());
        rename(&mut s, "Unsaved");
        let path = dir.path().join("locked.symbio");
        let mut other = ProjectStore::open(&path).unwrap();
        other.save(&Project::new("Other").unwrap()).unwrap();
        let before = std::fs::read(&path).unwrap();
        assert_eq!(s.save_as(&path).unwrap_err().code, "project_locked");
        assert!(s.core.dirty());
        assert_eq!(s.generation, 1);
        assert_eq!(s.store.path(), dir.path().join("original.symbio"));
        assert_eq!(std::fs::read(path).unwrap(), before);
        assert!(s.save_as(&dir.path().join("missing/copy.symbio")).is_err());
    }
    #[test]
    fn same_path_saves_without_relocking_and_external_changes_are_protected() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = session(dir.path());
        rename(&mut s, "Saved");
        s.save_as(&dir.path().join("./original.symbio")).unwrap();
        assert!(!s.core.dirty());
        assert_eq!(s.generation, 1);
        rename(&mut s, "Keep unsaved");
        std::fs::write(s.store.path(), b"external edit").unwrap();
        assert_eq!(s.save().unwrap_err().code, "external_change");
        assert!(s.core.dirty());
        assert_eq!(std::fs::read(s.store.path()).unwrap(), b"external edit");
    }
    #[test]
    fn replacing_document_resets_undo_and_accepts_lower_revision() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = session(dir.path());
        rename(&mut s, "Old edit");
        let path = dir.path().join("another.symbio");
        let mut store = ProjectStore::open(&path).unwrap();
        store.save(&Project::new("Another").unwrap()).unwrap();
        let p = store.load().unwrap().unwrap();
        s.replace(AppCore::new(p).unwrap(), store).unwrap();
        assert_eq!(s.core.project().name, "Another");
        assert_eq!(s.core.project().revision, 0);
        assert!(!s.core.snapshot().can_undo);
        assert!(s.check_generation(1).is_err());
        assert_eq!(s.snapshot().generation, 2);
    }
    #[test]
    fn operation_guard_releases_on_error_and_prevents_overlapping_dialogs() {
        let op = FileOperation::default();
        {
            let _guard = op.acquire().unwrap();
            assert!(op.busy());
            assert!(op.acquire().is_err());
        }
        assert!(!op.busy());
        assert!(op.acquire().is_ok());
    }
}
