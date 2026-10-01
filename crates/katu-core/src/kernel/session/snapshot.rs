//! Snapshot do [`State`] numa fronteira (ADR 0008), escrito atomicamente.
//!
//! O snapshot é uma **otimização reconstruível**: guarda o `seq`/`offset` do log, o uso de orçamento
//! e o estado. A retomada carrega-o e reaplica só a cauda (sem ler o prefixo). Se faltar/corromper,
//! faz-se *replay* total; se divergir, [`super::Session::verify`] deteta.
//!
//! **Q-15 — cauda limitada e hash canónico.** Duas fronteiras escrevem snapshot: a transição de fase
//! (contrato) e o fim de turno **quando a cauda desde o último snapshot passa de
//! [`MAX_TAIL_BYTES`]**. É o teto que faz a retomada ser O(1) no comprimento da história: o que se
//! relê é a cauda, não "o log todo menos o prefixo". O `hash` canónico do estado (FNV-1a sobre a
//! serialização determinística) fecha o caminho: um snapshot que não casa com o seu próprio estado é
//! **descartado** (replay total), em vez de produzir um estado plausível e errado.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use katu_policy::ToolName;
use serde::{Deserialize, Serialize};

use crate::kernel::budget::Budget;
use crate::kernel::canonical_hash;
use crate::kernel::state::State;
use crate::ports::{Fs, FsError};

/// Versão do esquema do snapshot.
pub(super) const SNAPSHOT_SCHEMA_VERSION: u32 = 4;

/// Teto da cauda (bytes de log) entre snapshots: o que a retomada relê (Q-15).
///
/// Escolhido por medição (`bench/e18/resume/`): com 128 KiB a retomada de uma sessão de 20 000 turnos
/// fica em ~0,3 ms contra 2,2 ms de um snapshot só na fronteira de fase, ao custo de um snapshot a
/// cada ~1,4 MB de log. Abaixo disto o snapshot escreve demasiadas vezes; acima, a retomada volta a
/// crescer com a história.
pub const MAX_TAIL_BYTES: u64 = 128 * 1024;

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
    /// Histórico temporal `(ms, micros)` das camadas rolante/velocidade até `seq`.
    pub history: Vec<(u64, u64)>,
    /// Estado reconstruído.
    pub state: State,
    /// Hash canónico do `state` (Q-15): valida a integridade do snapshot no carregamento.
    /// Preenchido por [`save`] (derivado do estado, nunca aceite de fora).
    pub hash: u64,
}

impl StateSnapshot {
    /// Hash canónico do estado (FNV-1a da serialização determinística).
    ///
    /// # Errors
    /// Erro de serialização do estado.
    pub(super) fn state_hash(state: &State) -> Result<u64, FsError> {
        let _span = crate::trace_fn!("kernel::session::snapshot::state_hash");

        canonical_hash(state).map_err(|err| FsError::Io(err.to_string()))
    }
}

/// Caminho do snapshot numa sessão.
#[must_use]
pub(super) fn snapshot_path(dir: &Path) -> PathBuf {
    let _span = crate::trace_fn!("kernel::session::snapshot::snapshot_path");

    dir.join("snapshot.v1.json")
}

/// Grava o snapshot atomicamente.
///
/// # Errors
/// [`FsError`] se a serialização ou a escrita falharem.
pub(super) fn save(
    fs: &dyn Fs,
    dir: &Path,
    mut snapshot: StateSnapshot,
) -> Result<StateSnapshot, FsError> {
    let _span = crate::trace_fn!("kernel::session::snapshot::save");
    // O hash é derivado do estado, nunca aceite de fora: um snapshot inconsistente não se grava.
    snapshot.hash = StateSnapshot::state_hash(&snapshot.state)?;
    let bytes = serde_json::to_vec(&snapshot).map_err(|err| FsError::Io(err.to_string()))?;
    fs.write_atomic(&snapshot_path(dir), &bytes)?;
    Ok(snapshot)
}

/// Lê o snapshot, se existir e tiver a versão suportada.
#[must_use]
pub(super) fn load(fs: &dyn Fs, dir: &Path) -> Option<StateSnapshot> {
    let _span = crate::trace_fn!("kernel::session::snapshot::load");

    let bytes = fs.read(&snapshot_path(dir)).ok()?;
    let snapshot: StateSnapshot = serde_json::from_slice(&bytes).ok()?;
    if snapshot.schema_version != SNAPSHOT_SCHEMA_VERSION {
        return None;
    }
    // Um snapshot que não casa com o seu estado é **descartado** (replay total, fail-closed).
    let hash = StateSnapshot::state_hash(&snapshot.state).ok()?;
    (snapshot.hash == hash).then_some(snapshot)
}

#[cfg(test)]
mod tests;
