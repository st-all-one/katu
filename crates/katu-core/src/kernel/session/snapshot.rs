//! Snapshot do [`State`] no limite de fase (ADR 0008), escrito atomicamente.
//!
//! O snapshot é uma **otimização reconstruível**: guarda o `seq`/`offset` do log, o uso de orçamento
//! e o estado. A retomada carrega-o e reaplica só a cauda (sem ler o prefixo). Se faltar/corromper,
//! faz-se *replay* total; se divergir, [`super::Session::verify`] deteta.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use katu_policy::ToolName;
use serde::{Deserialize, Serialize};

use crate::kernel::budget::Budget;
use crate::kernel::state::State;
use crate::ports::{Fs, FsError};

/// Versão do esquema do snapshot.
pub(super) const SNAPSHOT_SCHEMA_VERSION: u32 = 2;

/// Estado persistido numa fronteira de fase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateSnapshot {
    /// Versão do esquema.
    pub schema_version: u32,
    /// Último `seq` do log incluído neste estado.
    pub seq: u64,
    /// Offset (bytes) no log do início da linha `seq+1` (`0` = sem offset fiável).
    pub offset: u64,
    /// Uso global de orçamento reconstruído até `seq`.
    pub budget: Budget,
    /// Chamadas por ferramenta até `seq`.
    pub per_tool: BTreeMap<ToolName, u32>,
    /// Estado reconstruído.
    pub state: State,
}

/// Caminho do snapshot numa sessão.
#[must_use]
pub(super) fn snapshot_path(dir: &Path) -> PathBuf {
    dir.join("snapshot.v1.json")
}

/// Grava o snapshot atomicamente.
///
/// # Errors
/// [`FsError`] se a serialização ou a escrita falharem.
pub(super) fn save(fs: &dyn Fs, dir: &Path, snapshot: &StateSnapshot) -> Result<(), FsError> {
    let bytes = serde_json::to_vec(snapshot).map_err(|err| FsError::Io(err.to_string()))?;
    fs.write_atomic(&snapshot_path(dir), &bytes)
}

/// Lê o snapshot, se existir e tiver a versão suportada.
#[must_use]
pub(super) fn load(fs: &dyn Fs, dir: &Path) -> Option<StateSnapshot> {
    let bytes = fs.read(&snapshot_path(dir)).ok()?;
    let snapshot: StateSnapshot = serde_json::from_slice(&bytes).ok()?;
    (snapshot.schema_version == SNAPSHOT_SCHEMA_VERSION).then_some(snapshot)
}

#[cfg(test)]
mod tests;
