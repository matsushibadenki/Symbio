use crate::{Error, Project, Result};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_BYTES: u64 = 8 * 1024 * 1024;
/// An advisory lock protects cooperating Symbio processes. External edits are detected on save.
pub struct ProjectStore {
    path: PathBuf,
    _lock: File,
    baseline: Option<Vec<u8>>,
}
impl ProjectStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let parent = parent.canonicalize()?;
        let name = path
            .file_name()
            .ok_or_else(|| Error::Invalid("project path".into()))?;
        let path = parent.join(name);
        if path
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
        {
            return Err(Error::Invalid("symlink project".into()));
        }
        let lock_path = path.with_file_name(format!("{}.lock", name.to_string_lossy()));
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)?;
        lock.try_lock().map_err(|_| Error::Locked)?;
        let baseline = read_optional(&path)?;
        Ok(Self {
            path,
            _lock: lock,
            baseline,
        })
    }
    pub fn load(&mut self) -> Result<Option<Project>> {
        let bytes = read_optional(&self.path)?;
        let project = bytes
            .as_deref()
            .map(serde_json::from_slice::<Project>)
            .transpose()?;
        if let Some(p) = &project {
            p.validate()?;
        }
        self.baseline = bytes;
        Ok(project)
    }
    pub fn save(&mut self, project: &Project) -> Result<()> {
        project.validate()?;
        if read_optional(&self.path)? != self.baseline {
            return Err(Error::ExternalChange);
        }
        let bytes = serde_json::to_vec_pretty(project)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(Error::Invalid("project size".into()));
        }
        let parent = self.path.parent().expect("canonical parent");
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(&self.path)
            .map_err(|e| Error::Io(e.error))?;
        // Rename has committed even if directory sync fails; track that disk state.
        self.baseline = Some(bytes);
        #[cfg(unix)]
        File::open(parent)?.sync_all()?;
        Ok(())
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
}
fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(Error::Invalid("project size".into()));
    }
    Ok(Some(bytes))
}
