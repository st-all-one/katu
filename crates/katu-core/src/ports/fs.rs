//! Porta de sistema de ficheiros (`Fs`) e a *fake* determinística [`MemFs`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use super::clock::Timestamp;

/// Erro de uma operação de sistema de ficheiros.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FsError {
    /// Caminho inexistente.
    NotFound,
    /// Outro erro de I/O (mensagem estável, sem segredo).
    Io(String),
}

impl FsError {
    /// Converte um erro de I/O do `std` num erro de porta.
    #[must_use]
    pub fn from_io(err: &std::io::Error) -> Self {
        if err.kind() == std::io::ErrorKind::NotFound {
            Self::NotFound
        } else {
            Self::Io(err.to_string())
        }
    }
}

impl std::fmt::Display for FsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => f.write_str("caminho não encontrado"),
            Self::Io(message) => write!(f, "erro de I/O: {message}"),
        }
    }
}

impl std::error::Error for FsError {}

/// Porta de sistema de ficheiros (I/O confinado e testável).
pub trait Fs: Send + Sync {
    /// Lê um ficheiro inteiro.
    fn read(&self, path: &Path) -> Result<Vec<u8>, FsError>;

    /// Escreve atomicamente: temporário → `fsync` → `rename`.
    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), FsError>;

    /// Anexa bytes ao fim de um ficheiro (cria se não existir), de forma durável.
    fn append(&self, path: &Path, bytes: &[u8]) -> Result<(), FsError>;

    /// `true` se o caminho existe.
    fn exists(&self, path: &Path) -> bool;

    /// Modificação (*mtime*) do caminho.
    fn mtime(&self, path: &Path) -> Result<Timestamp, FsError>;

    /// Lista as entradas diretas de um diretório, em ordem canônica.
    fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, FsError>;
}

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
    clock_ms: u64,
}

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
}

/// Bloqueia um `Mutex`, recuperando o valor mesmo que o lock esteja envenenado.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
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
        let mut entries: Vec<PathBuf> = inner
            .files
            .keys()
            .filter(|candidate| candidate.parent() == Some(path))
            .cloned()
            .collect();
        entries.sort();
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::{Fs, FsError, MemFs};
    use std::path::{Path, PathBuf};

    #[test]
    fn memfs_roundtrip() -> Result<(), FsError> {
        let fs = MemFs::new();
        let path = PathBuf::from("/a.txt");
        fs.write_atomic(&path, b"hi")?;
        assert!(fs.exists(&path));
        assert_eq!(fs.read(&path)?, b"hi".to_vec());
        Ok(())
    }

    #[test]
    fn memfs_missing_is_not_found() {
        let fs = MemFs::new();
        assert_eq!(fs.read(Path::new("/nope")), Err(FsError::NotFound));
    }

    #[test]
    fn memfs_lists_in_canonical_order() -> Result<(), FsError> {
        let fs = MemFs::new();
        fs.write_atomic(Path::new("/dir/b"), b"b")?;
        fs.write_atomic(Path::new("/dir/a"), b"a")?;
        let entries = fs.list_dir(Path::new("/dir"))?;
        assert_eq!(
            entries,
            vec![PathBuf::from("/dir/a"), PathBuf::from("/dir/b")]
        );
        Ok(())
    }

    #[test]
    fn memfs_mtime_advances_monotonically() -> Result<(), FsError> {
        let fs = MemFs::new();
        let first = Path::new("/first");
        let second = Path::new("/second");
        fs.write_atomic(first, b"1")?;
        fs.write_atomic(second, b"2")?;
        assert!(fs.mtime(second)? > fs.mtime(first)?);
        Ok(())
    }
}
