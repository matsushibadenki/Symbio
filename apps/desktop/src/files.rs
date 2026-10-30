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
    store: Option<ProjectStore>,
    generation: u64,
    active_document: u64,
    next_document: u64,
    retained: Vec<RetainedDocument>,
}
const MAX_DOCUMENTS: usize = 16;
struct RetainedDocument {
    id: u64,
    core: AppCore,
    store: Option<ProjectStore>,
}
#[derive(Serialize, Clone)]
struct DocumentSummary {
    id: u64,
    name: String,
    dirty: bool,
    path: Option<String>,
}
#[derive(Serialize, Clone)]
pub struct DocumentSnapshot {
    #[serde(flatten)]
    snapshot: symbio_core::Snapshot,
    generation: u64,
    path: Option<String>,
    active_document: u64,
    documents: Vec<DocumentSummary>,
}
impl Session {
    pub fn new(core: AppCore, store: ProjectStore) -> Self {
        Self {
            core,
            store: Some(store),
            generation: 1,
            active_document: 1,
            next_document: 2,
            retained: vec![],
        }
    }
    fn path(&self) -> Option<&Path> {
        self.store.as_ref().map(ProjectStore::path)
    }
    fn documents(&self) -> Vec<DocumentSummary> {
        let summary = |id, core: &AppCore, store: Option<&ProjectStore>| DocumentSummary {
            id,
            name: core.project().name.clone(),
            dirty: core.dirty(),
            path: store.map(|s| s.path().to_string_lossy().into_owned()),
        };
        let mut documents = vec![summary(
            self.active_document,
            &self.core,
            self.store.as_ref(),
        )];
        documents.extend(
            self.retained
                .iter()
                .map(|d| summary(d.id, &d.core, d.store.as_ref())),
        );
        documents.sort_by_key(|d| d.id);
        documents
    }
    pub fn has_unsaved_changes(&self) -> bool {
        self.core.dirty() || self.retained.iter().any(|d| d.core.dirty())
    }
    fn document_at(&self, path: &Path) -> Option<u64> {
        self.documents()
            .into_iter()
            .find(|d| {
                d.path
                    .as_deref()
                    .is_some_and(|p| same_path(path, Path::new(p)))
            })
            .map(|d| d.id)
    }
    pub fn snapshot(&self) -> DocumentSnapshot {
        DocumentSnapshot {
            snapshot: self.core.snapshot(),
            generation: self.generation,
            path: self.path().map(|p| p.to_string_lossy().into_owned()),
            active_document: self.active_document,
            documents: self.documents(),
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
        self.store
            .as_mut()
            .ok_or_else(|| error("save_as_required"))?
            .save(self.core.project())?;
        self.core.mark_saved();
        Ok(())
    }
    fn load(path: &Path) -> Result<(AppCore, ProjectStore), UiError> {
        let mut store = ProjectStore::open(path)?;
        let project = store.load()?.ok_or_else(|| error("io_error"))?;
        Ok((AppCore::new(project)?, store))
    }
    fn add_document(&mut self, core: AppCore, store: Option<ProjectStore>) -> Result<(), UiError> {
        if self.retained.len() + 1 >= MAX_DOCUMENTS {
            return Err(error("document_limit"));
        }
        let next = self
            .generation
            .checked_add(1)
            .ok_or_else(|| error("resource_limit"))?;
        let next_id = self
            .next_document
            .checked_add(1)
            .ok_or_else(|| error("resource_limit"))?;
        self.retained.push(RetainedDocument {
            id: self.active_document,
            core: std::mem::replace(&mut self.core, core),
            store: std::mem::replace(&mut self.store, store),
        });
        self.active_document = self.next_document;
        self.next_document = next_id;
        self.generation = next;
        Ok(())
    }
    fn switch(&mut self, id: u64) -> Result<(), UiError> {
        if id == self.active_document {
            return Ok(());
        }
        let index = self
            .retained
            .iter()
            .position(|d| d.id == id)
            .ok_or_else(|| error("document_changed"))?;
        let next = self
            .generation
            .checked_add(1)
            .ok_or_else(|| error("resource_limit"))?;
        let target = &mut self.retained[index];
        std::mem::swap(&mut self.core, &mut target.core);
        std::mem::swap(&mut self.store, &mut target.store);
        std::mem::swap(&mut self.active_document, &mut target.id);
        self.generation = next;
        Ok(())
    }
    fn close_active(&mut self, blank_name: &str) -> Result<(), UiError> {
        if self.core.dirty() {
            return Err(error("unsaved_changes"));
        }
        let next = self
            .generation
            .checked_add(1)
            .ok_or_else(|| error("resource_limit"))?;
        let target = if self.retained.is_empty() {
            let id = self.next_document;
            let next_id = id.checked_add(1).ok_or_else(|| error("resource_limit"))?;
            let core = AppCore::new(symbio_core::Project::empty(blank_name)?)?;
            self.next_document = next_id;
            RetainedDocument {
                id,
                core,
                store: None,
            }
        } else {
            self.retained.pop().unwrap()
        };
        self.core = target.core;
        self.store = target.store;
        self.active_document = target.id;
        self.generation = next;
        Ok(())
    }
    fn save_as(&mut self, path: &Path) -> Result<(), UiError> {
        if self.path().is_some_and(|current| same_path(path, current)) {
            return self.save();
        }
        let next = self
            .generation
            .checked_add(1)
            .ok_or_else(|| error("resource_limit"))?;
        let mut destination = ProjectStore::open(path)?;
        destination.save(self.core.project())?;
        // Publish the new location only after a successful atomic save.
        self.store = Some(destination);
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
        {
            app.state::<Shared>()
                .0
                .lock()
                .map_err(|_| error("internal_error"))?
                .check_generation(generation)?;
        }
        let Some(path) = pick_save_path(&app)? else {
            return Ok(None);
        };
        let state = app.state::<Shared>();
        let mut session = state.0.lock().map_err(|_| error("internal_error"))?;
        session.check_generation(generation)?;
        no_drafts(&app)?;
        session.save_as(&path)?;
        remember(&app, session.path().expect("saved document"));
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
            session.path().map(Path::to_path_buf)
        };
        let mut dialog = app
            .dialog()
            .file()
            .set_title(labels(&app)[0])
            .add_filter("Symbio", &["symbio"])
            .set_directory(
                path.as_deref()
                    .and_then(Path::parent)
                    .map(Path::to_path_buf)
                    .unwrap_or(app.path().app_data_dir().map_err(|_| error("io_error"))?),
            );
        if let Some(window) = app.get_window("main") {
            dialog = dialog.set_parent(&window);
        }
        let Some(file) = dialog.blocking_pick_file() else {
            return Ok(None);
        };
        let path = file.into_path().map_err(|_| error("io_error"))?;
        let existing = {
            let state = app.state::<Shared>();
            let session = state.0.lock().map_err(|_| error("internal_error"))?;
            session.check_generation(generation)?;
            session.document_at(&path)
        };
        // Validate and lock a new candidate before touching the open documents.
        let candidate = if existing.is_none() {
            Some(Session::load(&path)?)
        } else {
            None
        };
        let out = {
            let state = app.state::<Shared>();
            let mut session = state.0.lock().map_err(|_| error("internal_error"))?;
            session.check_generation(generation)?;
            no_drafts(&app)?;
            if let Some(id) = existing {
                session.switch(id)?;
            } else if let Some((core, store)) = candidate {
                session.add_document(core, Some(store))?;
            }
            remember(&app, session.path().expect("saved document"));
            session.snapshot()
        };
        let _ = app.emit("symbio://changed", &out);
        if let Some(window) = app.get_window("plot") {
            let _ = window.hide();
        }
        super::surface3d::document_changed(&app);
        Ok(Some(out))
    })
    .await
    .map_err(|_| error("internal_error"))?
}
fn path_for(app: &tauri::AppHandle) -> Result<Option<PathBuf>, UiError> {
    Ok(app
        .state::<Shared>()
        .0
        .lock()
        .map_err(|_| error("internal_error"))?
        .path()
        .map(Path::to_path_buf))
}
fn pick_save_path(app: &tauri::AppHandle) -> Result<Option<PathBuf>, UiError> {
    let path = path_for(app)?;
    let directory = path
        .as_deref()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or(app.path().app_data_dir().map_err(|_| error("io_error"))?);
    let mut dialog = app
        .dialog()
        .file()
        .set_title(labels(app)[1])
        .add_filter("Symbio", &["symbio"])
        .set_directory(directory)
        .set_file_name(
            path.as_deref()
                .and_then(Path::file_name)
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or("Untitled.symbio".into()),
        );
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
    Ok(Some(path))
}
// Return a token checked again at publication, including after any Save As dialog.
fn confirm_save(app: &tauri::AppHandle, generation: u64) -> Result<Option<(u64, u64)>, UiError> {
    let (dirty, revision, untitled) = {
        let state = app.state::<Shared>();
        let session = state.0.lock().map_err(|_| error("internal_error"))?;
        session.check_generation(generation)?;
        no_drafts(app)?;
        (
            session.core.dirty(),
            session.core.project().revision,
            session.path().is_none(),
        )
    };
    if !dirty {
        return Ok(Some((generation, revision)));
    }
    let locale = app.state::<super::workspace::Workspace>().snapshot().locale;
    let (message, save, cancel) = match locale.as_str() {
        "ja" => (
            "現在の文書を保存して続けますか？",
            "保存して続ける",
            "キャンセル",
        ),
        "zh-CN" => ("是否保存当前文档后继续？", "保存并继续", "取消"),
        _ => (
            "Save the current document before continuing?",
            "Save and Continue",
            "Cancel",
        ),
    };
    let mut dialog = app.dialog().message(message).title("Symbio").buttons(
        MessageDialogButtons::OkCancelCustom(save.into(), cancel.into()),
    );
    if let Some(window) = app.get_window("main") {
        dialog = dialog.parent(&window);
    }
    if !dialog.blocking_show() {
        return Ok(None);
    }
    let destination = if untitled {
        let Some(path) = pick_save_path(app)? else {
            return Ok(None);
        };
        Some(path)
    } else {
        None
    };
    let state = app.state::<Shared>();
    let mut session = state.0.lock().map_err(|_| error("internal_error"))?;
    session.check_generation(generation)?;
    no_drafts(app)?;
    if session.core.project().revision != revision {
        return Err(error("revision_conflict"));
    }
    if let Some(path) = destination {
        session.save_as(&path)?;
        remember(app, session.path().unwrap());
    } else {
        session.save()?;
    }
    // Publish this save even if the following switch fails its final token check.
    let _ = app.emit("symbio://changed", session.snapshot());
    Ok(Some((session.generation, revision)))
}
#[tauri::command]
pub async fn new_document(
    webview: tauri::Webview,
    generation: u64,
) -> Result<Option<DocumentSnapshot>, UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let operation = app.state::<FileOperation>();
        let _guard = operation.acquire()?;
        prepare(&app)?;
        let _freeze = Freeze::new(&app);
        let name = blank_name(&app);
        let core = AppCore::new(symbio_core::Project::empty(name)?)?;
        let out = {
            let state = app.state::<Shared>();
            let mut session = state.0.lock().map_err(|_| error("internal_error"))?;
            session.check_generation(generation)?;
            no_drafts(&app)?;
            session.add_document(core, None)?;
            session.snapshot()
        };
        let _ = app.emit("symbio://changed", &out);
        if let Some(window) = app.get_window("plot") {
            let _ = window.hide();
        }
        super::surface3d::document_changed(&app);
        Ok(Some(out))
    })
    .await
    .map_err(|_| error("internal_error"))?
}

fn blank_name(app: &tauri::AppHandle) -> &'static str {
    match app
        .state::<super::workspace::Workspace>()
        .snapshot()
        .locale
        .as_str()
    {
        "ja" => "名称未設定",
        "zh-CN" => "未命名",
        _ => "Untitled",
    }
}
fn publish_switch(app: &tauri::AppHandle, out: &DocumentSnapshot) {
    let _ = app.emit("symbio://changed", out);
    if let Some(window) = app.get_window("plot") {
        let _ = window.hide();
    }
    super::surface3d::document_changed(app);
}
#[tauri::command]
pub async fn switch_document(
    webview: tauri::Webview,
    generation: u64,
    id: u64,
) -> Result<DocumentSnapshot, UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let operation = app.state::<FileOperation>();
        let _guard = operation.acquire()?;
        prepare(&app)?;
        let _freeze = Freeze::new(&app);
        let out = {
            let state = app.state::<Shared>();
            let mut session = state.0.lock().map_err(|_| error("internal_error"))?;
            session.check_generation(generation)?;
            no_drafts(&app)?;
            session.switch(id)?;
            if let Some(path) = session.path() {
                remember(&app, path);
            }
            session.snapshot()
        };
        publish_switch(&app, &out);
        Ok(out)
    })
    .await
    .map_err(|_| error("internal_error"))?
}
#[tauri::command]
pub async fn close_document(
    webview: tauri::Webview,
    generation: u64,
) -> Result<Option<DocumentSnapshot>, UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let operation = app.state::<FileOperation>();
        let _guard = operation.acquire()?;
        prepare(&app)?;
        let _freeze = Freeze::new(&app);
        let Some((generation, revision)) = confirm_save(&app, generation)? else {
            return Ok(None);
        };
        let out = {
            let state = app.state::<Shared>();
            let mut session = state.0.lock().map_err(|_| error("internal_error"))?;
            session.check_generation(generation)?;
            no_drafts(&app)?;
            if session.core.project().revision != revision {
                return Err(error("revision_conflict"));
            }
            session.close_active(blank_name(&app))?;
            if let Some(path) = session.path() {
                remember(&app, path);
            }
            session.snapshot()
        };
        publish_switch(&app, &out);
        Ok(Some(out))
    })
    .await
    .map_err(|_| error("internal_error"))?
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
                actor: symbio_core::model::Actor::Human,
                command: Command::Rename { name: name.into() },
            })
            .unwrap();
    }
    #[test]
    fn save_as_preserves_source_and_undo_then_saves_to_new_location() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = session(dir.path());
        let original = std::fs::read(s.path().unwrap()).unwrap();
        rename(&mut s, "Changed");
        let path = dir.path().join("copy.symbio");
        s.save_as(&path).unwrap();
        assert!(!s.core.dirty());
        assert!(s.core.snapshot().can_undo);
        assert_eq!(
            std::fs::read(dir.path().join("original.symbio")).unwrap(),
            original
        );
        assert_eq!(s.path().unwrap(), path.canonicalize().unwrap());
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
        assert_eq!(
            s.path().unwrap(),
            dir.path().join("original.symbio").canonicalize().unwrap()
        );
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
        std::fs::write(s.path().unwrap(), b"external edit").unwrap();
        assert_eq!(s.save().unwrap_err().code, "external_change");
        assert!(s.core.dirty());
        assert_eq!(std::fs::read(s.path().unwrap()).unwrap(), b"external edit");
    }
    #[test]
    fn new_active_document_has_its_own_undo_and_accepts_lower_revision() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = session(dir.path());
        rename(&mut s, "Old edit");
        let path = dir.path().join("another.symbio");
        let mut store = ProjectStore::open(&path).unwrap();
        store.save(&Project::new("Another").unwrap()).unwrap();
        let p = store.load().unwrap().unwrap();
        s.add_document(AppCore::new(p).unwrap(), Some(store))
            .unwrap();
        assert_eq!(s.core.project().name, "Another");
        assert_eq!(s.core.project().revision, 0);
        assert!(!s.core.snapshot().can_undo);
        assert!(s.check_generation(1).is_err());
        assert_eq!(s.snapshot().generation, 2);
    }
    #[test]
    fn invalid_or_missing_open_target_preserves_current_document() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = session(dir.path());
        rename(&mut s, "Unsaved original");
        let before = serde_json::to_value(s.snapshot()).unwrap();
        let bad = dir.path().join("bad.symbio");
        std::fs::write(&bad, b"not a project").unwrap();
        assert!(Session::load(&bad).is_err());
        assert!(Session::load(&dir.path().join("missing.symbio")).is_err());
        assert_eq!(serde_json::to_value(s.snapshot()).unwrap(), before);
        assert_eq!(std::fs::read(bad).unwrap(), b"not a project");
    }
    #[test]
    fn new_blank_document_has_no_samples_or_location_and_cannot_overwrite_original() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = session(dir.path());
        let original = dir.path().join("original.symbio");
        let bytes = std::fs::read(&original).unwrap();
        s.add_document(
            AppCore::new(Project::empty("Untitled").unwrap()).unwrap(),
            None,
        )
        .unwrap();
        assert!(s.path().is_none());
        assert!(s.snapshot().path.is_none());
        assert!(s.core.project().graph.nodes.is_empty());
        assert!(s.core.project().current().cells.is_empty());
        assert!(s.core.project().current().assumptions.is_empty());
        assert!(s.core.project().history.is_empty());
        assert!(s.core.project().current().selected.is_none());
        assert!(!s.core.snapshot().can_undo);
        assert_eq!(s.save().unwrap_err().code, "save_as_required");
        assert_eq!(std::fs::read(&original).unwrap(), bytes);
        // The original document stays open with its file lock.
        assert!(ProjectStore::open(original).is_err());
        assert_eq!(s.documents().len(), 2);
        let destination = dir.path().join("blank.symbio");
        s.save_as(&destination).unwrap();
        drop(s);
        let (restored, _) = Session::load(&destination).unwrap();
        assert!(restored.project().current().cells.is_empty());
        assert!(restored.project().graph.nodes.is_empty());
    }
    #[test]
    fn first_equation_in_blank_document_calculates_and_survives_first_save() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = session(dir.path());
        s.add_document(
            AppCore::new(Project::empty("Blank study").unwrap()).unwrap(),
            None,
        )
        .unwrap();
        s.core
            .apply(Request {
                expected_revision: 0,
                actor: symbio_core::model::Actor::Human,
                command: Command::DefineLatex {
                    id: None,
                    name: "f".into(),
                    source: "2+3".into(),
                },
            })
            .unwrap();
        assert_eq!(s.core.project().current().cells[0].id, 1);
        assert!(s.core.dirty());
        let path = dir.path().join("first.symbio");
        s.save_as(&path).unwrap();
        assert!(!s.core.dirty());
        drop(s);
        let (core, _) = Session::load(&path).unwrap();
        assert_eq!(core.snapshot().cells[0].result_text.as_deref(), Some("5"));
    }
    #[test]
    fn switching_preserves_unsaved_edits_independent_undo_and_file_locks() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = session(dir.path());
        rename(&mut s, "Original unsaved");
        s.add_document(
            AppCore::new(Project::empty("Second").unwrap()).unwrap(),
            None,
        )
        .unwrap();
        assert!(s.has_unsaved_changes());
        assert!(!s.core.dirty());
        assert!(!s.core.snapshot().can_undo);
        assert!(ProjectStore::open(dir.path().join("original.symbio")).is_err());
        rename(&mut s, "Second unsaved");
        let old_generation = s.generation;
        s.switch(1).unwrap();
        assert!(s.check_generation(old_generation).is_err());
        assert_eq!(s.core.project().name, "Original unsaved");
        assert!(s.core.snapshot().can_undo);
        s.core
            .apply(Request {
                expected_revision: s.core.project().revision,
                actor: symbio_core::model::Actor::Human,
                command: Command::Undo,
            })
            .unwrap();
        assert_eq!(s.core.project().name, "Original");
        s.switch(2).unwrap();
        assert_eq!(s.core.project().name, "Second unsaved");
        assert!(s.core.snapshot().can_undo);
        assert!(!s.core.snapshot().can_redo);
        assert!(s.has_unsaved_changes());
    }
    #[test]
    fn duplicate_paths_switch_existing_documents_and_closed_files_unlock() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = session(dir.path());
        s.add_document(
            AppCore::new(Project::empty("Second").unwrap()).unwrap(),
            None,
        )
        .unwrap();
        assert_eq!(
            s.document_at(&dir.path().join("./original.symbio")),
            Some(1)
        );
        s.switch(1).unwrap();
        assert_eq!(s.documents().len(), 2);
        s.close_active("Untitled").unwrap();
        assert_eq!(s.core.project().name, "Second");
        assert!(ProjectStore::open(dir.path().join("original.symbio")).is_ok());
        assert_eq!(s.document_at(&dir.path().join("original.symbio")), None);
        s.close_active("Untitled").unwrap();
        assert_eq!(s.documents().len(), 1);
        assert!(s.path().is_none());
        assert!(s.core.project().current().cells.is_empty());
    }
    #[test]
    fn failed_or_excessive_document_changes_do_not_destroy_open_work() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = session(dir.path());
        rename(&mut s, "Keep me");
        let before = serde_json::to_value(s.snapshot()).unwrap();
        assert!(s.close_active("Untitled").is_err());
        assert!(s.switch(999).is_err());
        assert_eq!(serde_json::to_value(s.snapshot()).unwrap(), before);
        for i in 1..MAX_DOCUMENTS {
            s.add_document(
                AppCore::new(Project::empty(&format!("Document {i}")).unwrap()).unwrap(),
                None,
            )
            .unwrap();
        }
        let before = serde_json::to_value(s.snapshot()).unwrap();
        assert_eq!(
            s.add_document(
                AppCore::new(Project::empty("Overflow").unwrap()).unwrap(),
                None
            )
            .unwrap_err()
            .code,
            "document_limit"
        );
        assert_eq!(serde_json::to_value(s.snapshot()).unwrap(), before);
        s.switch(1).unwrap();
        assert_eq!(s.core.project().name, "Keep me");
        assert!(s.has_unsaved_changes());
    }
    #[test]
    fn save_as_cannot_overwrite_another_retained_document() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = session(dir.path());
        let original = dir.path().join("original.symbio");
        let bytes = std::fs::read(&original).unwrap();
        s.add_document(
            AppCore::new(Project::empty("Second").unwrap()).unwrap(),
            None,
        )
        .unwrap();
        rename(&mut s, "Second unsaved");
        assert_eq!(s.save_as(&original).unwrap_err().code, "project_locked");
        assert_eq!(std::fs::read(original).unwrap(), bytes);
        assert!(s.path().is_none());
        assert!(s.core.dirty());
        assert_eq!(s.documents().len(), 2);
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

#[tauri::command]
pub async fn export_reproduction(
    webview: tauri::Webview,
    generation: u64,
    revision: u64,
) -> Result<Option<String>, UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let op = app.state::<FileOperation>();
        let _guard = op.acquire()?;
        prepare(&app)?;
        let _freeze = Freeze::new(&app);
        let package = {
            let state = app.state::<Shared>();
            let session = state.0.lock().map_err(|_| error("internal_error"))?;
            session.check_generation(generation)?;
            if session.core.project().revision != revision {
                return Err(error("revision_conflict"));
            }
            symbio_core::reproduction::capture(session.core.project())?
        };
        let locale = app.state::<super::workspace::Workspace>().snapshot().locale;
        let title = match locale.as_str() {
            "ja" => "再現パッケージを書き出す",
            "zh-CN" => "导出重现包",
            _ => "Export reproduction package",
        };
        let Some(file) = app
            .dialog()
            .file()
            .set_title(title)
            .add_filter("Symbio reproduction", &["json"])
            .set_file_name(format!(
                "research-r{}.symbio-repro.json",
                package.payload.revision
            ))
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = file.into_path().map_err(|_| error("io_error"))?;
        let state = app.state::<Shared>();
        let session = state.0.lock().map_err(|_| error("internal_error"))?;
        session.check_generation(generation)?;
        if session.core.project().revision != revision {
            return Err(error("revision_conflict"));
        }
        no_drafts(&app)?;
        if path.exists() {
            return Err(error("reproduction_exists"));
        }
        symbio_core::reproduction::write_new(&path, &package)?;
        Ok(Some(path.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|_| error("internal_error"))?
}
#[tauri::command]
pub async fn verify_reproduction(
    webview: tauri::Webview,
    generation: u64,
) -> Result<Option<symbio_core::reproduction::Report>, UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let op = app.state::<FileOperation>();
        let _guard = op.acquire()?;
        let _freeze = Freeze::new(&app);
        app.state::<Shared>()
            .0
            .lock()
            .map_err(|_| error("internal_error"))?
            .check_generation(generation)?;
        let locale = app.state::<super::workspace::Workspace>().snapshot().locale;
        let title = match locale.as_str() {
            "ja" => "再現パッケージを検証",
            "zh-CN" => "验证重现包",
            _ => "Verify reproduction package",
        };
        let Some(file) = app
            .dialog()
            .file()
            .set_title(title)
            .add_filter("Symbio reproduction", &["json"])
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = file.into_path().map_err(|_| error("io_error"))?;
        let report = symbio_core::reproduction::verify(&symbio_core::reproduction::read(path)?)?;
        app.state::<Shared>()
            .0
            .lock()
            .map_err(|_| error("internal_error"))?
            .check_generation(generation)?;
        Ok(Some(report))
    })
    .await
    .map_err(|_| error("internal_error"))?
}

#[tauri::command]
pub async fn export_mathjson(
    webview: tauri::Webview,
    generation: u64,
    revision: u64,
) -> Result<Option<String>, UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let operation = app.state::<FileOperation>();
        let _guard = operation.acquire()?;
        prepare(&app)?;
        let _freeze = Freeze::new(&app);
        let exchange = {
            let state = app.state::<Shared>();
            let session = state.0.lock().map_err(|_| error("internal_error"))?;
            session.check_generation(generation)?;
            if session.core.project().revision != revision {
                return Err(error("revision_conflict"));
            }
            let id = session
                .core
                .project()
                .current()
                .selected
                .ok_or_else(|| error("invalid_project"))?;
            symbio_core::mathjson::export(session.core.project(), id)?
        };
        let locale = app.state::<super::workspace::Workspace>().snapshot().locale;
        let title = match locale.as_str() {
            "ja" => "数式をMathJSONで書き出す",
            "zh-CN" => "导出MathJSON表达式",
            _ => "Export expression as MathJSON",
        };
        let Some(file) = app
            .dialog()
            .file()
            .set_title(title)
            .add_filter("MathJSON", &["json"])
            .set_file_name(format!("{}.mathjson.json", exchange.name))
            .blocking_save_file()
        else {
            return Ok(None);
        };
        let path = file.into_path().map_err(|_| error("io_error"))?;
        let state = app.state::<Shared>();
        let session = state.0.lock().map_err(|_| error("internal_error"))?;
        session.check_generation(generation)?;
        no_drafts(&app)?;
        if session.core.project().revision != revision {
            return Err(error("revision_conflict"));
        }
        if path.exists() {
            return Err(error("reproduction_exists"));
        }
        symbio_core::mathjson::write_new(&path, &exchange)?;
        Ok(Some(path.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|_| error("internal_error"))?
}
#[tauri::command]
pub async fn import_mathjson(
    webview: tauri::Webview,
    generation: u64,
) -> Result<Option<DocumentSnapshot>, UiError> {
    authorize(&webview)?;
    let app = webview.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let operation = app.state::<FileOperation>();
        let _guard = operation.acquire()?;
        prepare(&app)?;
        let _freeze = Freeze::new(&app);
        app.state::<Shared>()
            .0
            .lock()
            .map_err(|_| error("internal_error"))?
            .check_generation(generation)?;
        let locale = app.state::<super::workspace::Workspace>().snapshot().locale;
        let title = match locale.as_str() {
            "ja" => "MathJSONを新しい文書に読み込む",
            "zh-CN" => "导入MathJSON到新文档",
            _ => "Import MathJSON into a new document",
        };
        let Some(file) = app
            .dialog()
            .file()
            .set_title(title)
            .add_filter("MathJSON", &["json"])
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let core = symbio_core::mathjson::import(&symbio_core::mathjson::read(
            file.into_path().map_err(|_| error("io_error"))?,
        )?)?;
        let out = {
            let state = app.state::<Shared>();
            let mut session = state.0.lock().map_err(|_| error("internal_error"))?;
            session.check_generation(generation)?;
            no_drafts(&app)?;
            session.add_document(core, None)?;
            session.snapshot()
        };
        let _ = app.emit("symbio://changed", &out);
        if let Some(window) = app.get_window("plot") {
            let _ = window.hide();
        }
        super::surface3d::document_changed(&app);
        Ok(Some(out))
    })
    .await
    .map_err(|_| error("internal_error"))?
}
