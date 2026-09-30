//! Snapshot do [`State`] no limite de fase (ADR 0008), escrito atomicamente.
//!
//! O snapshot é uma **otimização reconstruível**: guarda o `seq` do log até onde representa o
//! estado. A retomada carrega-o e reaplica só os eventos seguintes. Se faltar/corromper, faz-se
//! *replay* total; se divergir, [`super::Session::verify`] deteta.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::kernel::state::State;
use crate::ports::{Fs, FsError};

/// Versão do esquema do snapshot.
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// Estado persistido numa fronteira de fase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateSnapshot {
    /// Versão do esquema.
    pub schema_version: u32,
    /// Último `seq` do log incluído neste estado.
    pub seq: u64,
    /// Estado reconstruído.
    pub state: State,
}

/// Caminho do snapshot numa sessão.
#[must_use]
pub fn snapshot_path(dir: &Path) -> PathBuf {
    dir.join("snapshot.v1.json")
}

/// Grava o snapshot atomicamente.
///
/// # Errors
/// [`FsError`] se a serialização ou a escrita falharem.
pub fn save(fs: &dyn Fs, dir: &Path, snapshot: &StateSnapshot) -> Result<(), FsError> {
    let bytes = serde_json::to_vec(snapshot).map_err(|err| FsError::Io(err.to_string()))?;
    fs.write_atomic(&snapshot_path(dir), &bytes)
}

/// Lê o snapshot, se existir e tiver a versão suportada.
#[must_use]
pub fn load(fs: &dyn Fs, dir: &Path) -> Option<StateSnapshot> {
    let bytes = fs.read(&snapshot_path(dir)).ok()?;
    let snapshot: StateSnapshot = serde_json::from_slice(&bytes).ok()?;
    (snapshot.schema_version == SNAPSHOT_SCHEMA_VERSION).then_some(snapshot)
}

#[cfg(test)]
mod tests;
