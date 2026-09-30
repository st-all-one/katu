//! [`MemFs`] — sistema de ficheiros em memória para testes determinísticos.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use super::{Fs, FsError};
use crate::ports::Timestamp;

/// Entrada do sistema de ficheiros em memória.
#[derive(Debug, Clone)]
struct Entry {
    bytes: Vec<u8>,
    mtime: Timestamp,
}

/// Estado interno da [`MemFs`].
#[derive(Debug, Default)]
struct Inner {
    files: BTreeMap<PathBuf, Entry>,
    links: BTreeMap<PathBuf, PathBuf>,
    clock_ms: u64,
}

/// Profundidade máxima de resolução de symlinks (fail-closed contra ciclos).
const MAX_SYMLINKS: u8 = 40;

/// Sistema de ficheiros em memória, para testes determinísticos.
#[derive(Debug, Default)]
pub struct MemFs {
    inner: Mutex<Inner>,
}

impl MemFs {
    /// Cria um sistema de ficheiros em memória vazio.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Regista um symlink `link` → `target` (auxiliar de teste, E07-T02).
    pub fn symlink(&self, link: &Path, target: &Path) {
        let mut inner = lock(&self.inner);
        inner.links.insert(link.to_path_buf(), target.to_path_buf());
    }
}

/// Bloqueia um `Mutex`, recuperando o valor mesmo que o lock esteja envenenado.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Normaliza lexicalmente um caminho (resolve `.`/`..`, sem tocar no SO).
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::from("/");
    for component in path.components() {
        match component {
            Component::RootDir | Component::CurDir | Component::Prefix(_) => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(part) => out.push(part),
        }
    }
    out
}

impl Fs for MemFs {
    fn read(&self, path: &Path) -> Result<Vec<u8>, FsError> {
        let inner = lock(&self.inner);
        inner
            .files
            .get(path)
            .map(|entry| entry.bytes.clone())
            .ok_or(FsError::NotFound)
    }

    fn read_from(&self, path: &Path, offset: u64) -> Result<Vec<u8>, FsError> {
        let inner = lock(&self.inner);
        let entry = inner.files.get(path).ok_or(FsError::NotFound)?;
        let start =
            usize::try_from(offset).map_err(|_| FsError::Io("offset inválido".to_string()))?;
        let tail = entry
            .bytes
            .get(start..)
            .ok_or_else(|| FsError::Io("offset além do fim".to_string()))?;
        Ok(tail.to_vec())
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, FsError> {
        let inner = lock(&self.inner);
        let mut current = normalize(path);
        for _ in 0..MAX_SYMLINKS {
            let mut acc = PathBuf::from("/");
            let mut replaced = false;
            for component in current.components() {
                if let Component::Normal(part) = component {
                    acc.push(part);
                    if let Some(target) = inner.links.get(&acc) {
                        let base = acc.parent().unwrap_or_else(|| Path::new("/"));
                        current = if target.is_absolute() {
                            target.clone()
                        } else {
                            base.join(target)
                        };
                        replaced = true;
                        break;
                    }
                }
            }
            if !replaced {
                return Ok(current);
            }
            current = normalize(&current);
        }
        Err(FsError::Io(
            "demasiados níveis de symlink (ciclo?)".to_string(),
        ))
    }

    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), FsError> {
        let mut inner = lock(&self.inner);
        let next = inner.clock_ms.saturating_add(1);
        inner.clock_ms = next;
        let entry = Entry {
            bytes: bytes.to_vec(),
            mtime: Timestamp::from_millis(next),
        };
        inner.files.insert(path.to_path_buf(), entry);
        Ok(())
    }

    fn write_atomic_if(&self, path: &Path, bytes: &[u8], expected: &[u8]) -> Result<(), FsError> {
        let mut inner = lock(&self.inner);
        let matches = inner
            .files
            .get(path)
            .is_some_and(|entry| entry.bytes.as_slice() == expected);
        if !matches {
            return Err(FsError::Stale);
        }
        let next = inner.clock_ms.saturating_add(1);
        inner.clock_ms = next;
        inner.files.insert(
            path.to_path_buf(),
            Entry {
                bytes: bytes.to_vec(),
                mtime: Timestamp::from_millis(next),
            },
        );
        Ok(())
    }

    fn append(&self, path: &Path, bytes: &[u8]) -> Result<(), FsError> {
        let mut inner = lock(&self.inner);
        let next = inner.clock_ms.saturating_add(1);
        inner.clock_ms = next;
        let entry = inner
            .files
            .entry(path.to_path_buf())
            .or_insert_with(|| Entry {
                bytes: Vec::new(),
                mtime: Timestamp::from_millis(0),
            });
        entry.bytes.extend_from_slice(bytes);
        entry.mtime = Timestamp::from_millis(next);
        Ok(())
    }

    fn exists(&self, path: &Path) -> bool {
        let inner = lock(&self.inner);
        inner.files.contains_key(path)
            || inner
                .files
                .keys()
                .any(|key| key.starts_with(path) && key != path)
    }

    fn is_dir(&self, path: &Path) -> bool {
        let inner = lock(&self.inner);
        !inner.files.contains_key(path)
            && inner
                .files
                .keys()
                .any(|key| key.starts_with(path) && key != path)
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<(), FsError> {
        let mut inner = lock(&self.inner);
        let Some(mut entry) = inner.files.remove(from) else {
            return Err(FsError::NotFound);
        };
        let next = inner.clock_ms.saturating_add(1);
        inner.clock_ms = next;
        entry.mtime = Timestamp::from_millis(next);
        inner.files.insert(to.to_path_buf(), entry);
        Ok(())
    }

    fn create_dir_all(&self, _path: &Path) -> Result<(), FsError> {
        Ok(())
    }

    fn mtime(&self, path: &Path) -> Result<Timestamp, FsError> {
        let inner = lock(&self.inner);
        inner
            .files
            .get(path)
            .map(|entry| entry.mtime)
            .ok_or(FsError::NotFound)
    }

    fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, FsError> {
        let inner = lock(&self.inner);
        let mut entries: BTreeSet<PathBuf> = BTreeSet::new();
        for key in inner.files.keys() {
            if let Ok(rest) = key.strip_prefix(path)
                && let Some(first) = rest.components().next()
            {
                entries.insert(path.join(first.as_os_str()));
            }
        }
        Ok(entries.into_iter().collect())
    }

    fn remove(&self, path: &Path) -> Result<(), FsError> {
        let mut inner = lock(&self.inner);
        if inner.links.remove(path).is_some() {
            return Ok(());
        }
        match inner.files.remove(path) {
            Some(_) => Ok(()),
            None => Err(FsError::NotFound),
        }
    }
}
