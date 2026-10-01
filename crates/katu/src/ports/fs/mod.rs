//! Adaptador `StdFs`: sistema de ficheiros real com escrita atómica **endurecida** (E01-T02,
//! E07-T04).

use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::UNIX_EPOCH;

use katu_core::diag::{Level, events};
use katu_core::ports::{Fs, FsError, Timestamp};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

/// Contador de nomes temporários (único por processo; nomes imprevisíveis).
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Sistema de ficheiros real, com escrita atómica (`tmp` → `fsync` → `rename`).
pub(crate) struct StdFs;

impl Fs for StdFs {
    fn read(&self, path: &Path) -> Result<Vec<u8>, FsError> {
        let _span = katu_core::fn_span!(Level::Trace, events::FS_READ, "fs::read");
        fs::read(path).map_err(|err| FsError::from_io(&err))
    }

    fn read_from(&self, path: &Path, offset: u64) -> Result<Vec<u8>, FsError> {
        let _span = katu_core::fn_span!(Level::Trace, events::FS_READ, "fs::read_from");
        let mut file = fs::File::open(path).map_err(|err| FsError::from_io(&err))?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|err| FsError::from_io(&err))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|err| FsError::from_io(&err))?;
        Ok(bytes)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, FsError> {
        let _span = katu_core::fn_span!(Level::Trace, events::FS_STAT, "fs::canonicalize");
        match fs::canonicalize(path) {
            Ok(canonical) => Ok(canonical),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                // A folha pode não existir (ficheiro novo): resolve o ancestral e junta o nome.
                let parent = path.parent().ok_or(FsError::NotFound)?;
                let canonical = fs::canonicalize(parent).map_err(|err| FsError::from_io(&err))?;
                Ok(match path.file_name() {
                    Some(name) => canonical.join(name),
                    None => canonical,
                })
            }
            Err(err) => Err(FsError::from_io(&err)),
        }
    }

    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), FsError> {
        let _span = katu_core::fn_span!(Level::Trace, events::FS_WRITE, "fs::write_atomic");
        write_bytes_atomic(path, bytes)
    }

    fn write_atomic_if(&self, path: &Path, bytes: &[u8], expected: &[u8]) -> Result<(), FsError> {
        let _span = katu_core::fn_span!(Level::Trace, events::FS_WRITE, "fs::write_atomic_if");
        let current = fs::read(path).map_err(|err| FsError::from_io(&err))?;
        if current.as_slice() != expected {
            return Err(FsError::Stale);
        }
        // CAS *best-effort*: a janela entre a verificação e o `rename` é reduzida, não eliminada.
        write_bytes_atomic(path, bytes)
    }

    fn append(&self, path: &Path, bytes: &[u8]) -> Result<(), FsError> {
        let _span = katu_core::fn_span!(Level::Trace, events::FS_WRITE, "fs::append");
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|err| FsError::from_io(&err))?;
        file.write_all(bytes)
            .map_err(|err| FsError::from_io(&err))?;
        file.sync_data().map_err(|err| FsError::from_io(&err))
    }

    fn append_unsynced(&self, path: &Path, bytes: &[u8]) -> Result<(), FsError> {
        let _span = katu_core::fn_span!(Level::Trace, events::FS_WRITE, "fs::append_unsynced");
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|err| FsError::from_io(&err))?;
        file.write_all(bytes).map_err(|err| FsError::from_io(&err))
    }

    fn sync(&self, path: &Path) -> Result<(), FsError> {
        let _span = katu_core::fn_span!(Level::Trace, events::FS_WRITE, "fs::sync");
        // `fsync` num descritor aberto só para leitura é válido em POSIX e não muda o ficheiro.
        let file = fs::File::open(path).map_err(|err| FsError::from_io(&err))?;
        file.sync_data().map_err(|err| FsError::from_io(&err))
    }

    fn exists(&self, path: &Path) -> bool {
        let _span = katu_core::trace_fn!("ports::fs::exists");

        path.exists()
    }

    fn is_dir(&self, path: &Path) -> bool {
        let _span = katu_core::trace_fn!("ports::fs::is_dir");

        path.is_dir()
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<(), FsError> {
        let _span = katu_core::fn_span!(Level::Trace, events::FS_RENAME, "fs::rename");
        fs::rename(from, to).map_err(|err| FsError::from_io(&err))
    }

    fn create_dir_all(&self, path: &Path) -> Result<(), FsError> {
        let _span = katu_core::fn_span!(Level::Trace, events::FS_MKDIR, "fs::create_dir_all");
        fs::create_dir_all(path).map_err(|err| FsError::from_io(&err))
    }

    fn mtime(&self, path: &Path) -> Result<Timestamp, FsError> {
        let _span = katu_core::fn_span!(Level::Trace, events::FS_STAT, "fs::mtime");
        let meta = fs::metadata(path).map_err(|err| FsError::from_io(&err))?;
        let modified = meta.modified().map_err(|err| FsError::from_io(&err))?;
        let elapsed = modified
            .duration_since(UNIX_EPOCH)
            .map_err(|_| FsError::Io("mtime anterior à epoch".to_string()))?;
        let millis = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
        Ok(Timestamp::from_millis(millis))
    }

    fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, FsError> {
        let _span = katu_core::fn_span!(Level::Trace, events::FS_LIST, "fs::list_dir");
        let mut entries: Vec<PathBuf> = fs::read_dir(path)
            .map_err(|err| FsError::from_io(&err))?
            .map(|entry| entry.map(|item| item.path()))
            .collect::<Result<Vec<PathBuf>, std::io::Error>>()
            .map_err(|err| FsError::from_io(&err))?;
        entries.sort();
        Ok(entries)
    }

    fn remove(&self, path: &Path) -> Result<(), FsError> {
        let _span = katu_core::fn_span!(Level::Trace, events::FS_REMOVE, "fs::remove");
        // Recusa diretórios: a remoção é sempre de um ficheiro concreto (fail-closed).
        let meta = fs::symlink_metadata(path).map_err(|err| FsError::from_io(&err))?;
        if meta.is_dir() {
            return Err(FsError::Io("recusado remover um diretório".to_string()));
        }
        fs::remove_file(path).map_err(|err| FsError::from_io(&err))
    }
}

/// Caminho temporário **imprevisível**, no **mesmo diretório** do alvo (rename no mesmo FS).
fn temp_path(path: &Path) -> PathBuf {
    let _span = katu_core::trace_fn!("ports::fs::temp_path");

    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".{pid}.{counter}.tmp"));
    PathBuf::from(name)
}

/// Escrita atómica endurecida: temporário **exclusivo** (`O_EXCL`) com `0600` → `fsync` → `rename`.
///
/// O `O_EXCL` recusa um temporário pré-existente (inclusive um **symlink plantado**), pelo que a
/// escrita nunca segue links; o nome imprevisível evita a corrida de adivinhação (E07-T04).
fn write_atomic_via(path: &Path, temporary: &Path, bytes: &[u8]) -> Result<(), FsError> {
    let _span = katu_core::trace_fn!("ports::fs::write_atomic_via");

    {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options
            .open(temporary)
            .map_err(|err| FsError::from_io(&err))?;
        file.write_all(bytes)
            .map_err(|err| FsError::from_io(&err))?;
        file.sync_all().map_err(|err| FsError::from_io(&err))?;
    }
    fs::rename(temporary, path).map_err(|err| FsError::from_io(&err))
}

/// Escrita atómica com um temporário derivado do alvo.
fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<(), FsError> {
    let _span = katu_core::trace_fn!("ports::fs::write_bytes_atomic");

    write_atomic_via(path, &temp_path(path), bytes)
}

#[cfg(test)]
mod tests;
