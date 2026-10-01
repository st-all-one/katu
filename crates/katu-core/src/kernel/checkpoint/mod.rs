//! Checkpoint tipado no limite de fase (E04-T07), validado contra esquema e escrito atomicamente.
//!
//! O checkpoint é o `.STAGING.md` do maxima promovido de prosa a **artefacto de fase**
//! (`plan/10` §2): `temp → fsync → rename` pela porta [`Fs`] (`write_atomic`). O validador é
//! **zero-dep** (só `serde`): verifica a versão de esquema, rejeita campos desconhecidos e só
//! depois desserializa para o tipo [`Checkpoint`]. Um checkpoint no limite de fase é o gatilho
//! (§51.8) — nunca um limiar heurístico de NLU.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::state::State;
use crate::diag::{Level, events};
use crate::ports::{Fs, FsError};
use crate::validate::{Issue, Issues};
use katu_policy::Phase;

/// Versão do esquema do checkpoint.
pub const CHECKPOINT_SCHEMA_VERSION: u32 = 1;

/// Checkpoint de fase: estado tipado no limite entre fases do caminho único.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    /// Versão do esquema.
    pub schema_version: u32,
    /// Objetivo (inalterado ao longo da tarefa).
    pub goal: String,
    /// Resumo do estado corrente.
    pub state: String,
    /// Pendências em aberto (ordem canónica).
    pub pending: Vec<String>,
    /// Próxima ação declarada.
    pub next_action: String,
    /// Constatações acumuladas.
    pub findings: Vec<String>,
    /// Fase do caminho único no momento do checkpoint.
    pub phase: Phase,
}

impl Checkpoint {
    /// Constrói um checkpoint a partir do estado, no limite de uma fase.
    #[must_use]
    pub fn from_state(
        state: &State,
        goal: impl Into<String>,
        next_action: impl Into<String>,
    ) -> Self {
        let _span = crate::fn_span!(
            Level::Trace,
            events::CONTEXT_CHECKPOINT,
            "kernel::checkpoint::from_state"
        );
        let pending: Vec<String> = state
            .pending
            .keys()
            .map(|call| call.as_str().to_string())
            .collect();
        let phase = state.phase;
        let turn = state.turn;
        Self {
            schema_version: CHECKPOINT_SCHEMA_VERSION,
            goal: goal.into(),
            state: format!("{phase:?} turn={turn}"),
            pending,
            next_action: next_action.into(),
            findings: Vec::new(),
            phase,
        }
    }
}

/// Erro de checkpoint.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CheckpointError {
    /// Conteúdo ilegível (JSON inválido).
    #[error("checkpoint ilegível: {0}")]
    Parse(String),
    /// Versão de esquema não suportada.
    #[error("esquema de checkpoint não suportado: {found} (esperado {expected})")]
    SchemaVersion {
        /// Versão encontrada.
        found: u64,
        /// Versão esperada.
        expected: u32,
    },
    /// Conteúdo sintaticamente válido mas fora do esquema (campos em falta ou desconhecidos).
    #[error("checkpoint inválido: {0}")]
    Invalid(Issues),
    /// Falha de I/O.
    #[error("I/O do checkpoint: {0}")]
    Io(#[from] FsError),
}

/// Caminho canónico do checkpoint de uma sessão.
#[must_use]
pub fn checkpoint_path(dir: &Path) -> PathBuf {
    let _span = crate::trace_fn!("kernel::checkpoint::checkpoint_path");

    dir.join("checkpoint.json")
}

/// Valida um `Value` contra o esquema e devolve o checkpoint tipado.
///
/// # Errors
/// [`CheckpointError`] se não for um objeto, faltar `schema_version`, a versão não casar, houver
/// campos desconhecidos ou algum campo tiver o tipo errado.
pub fn validate(value: &Value) -> Result<Checkpoint, CheckpointError> {
    let _span = crate::trace_fn!("kernel::checkpoint::validate");

    let Some(object) = value.as_object() else {
        return Err(CheckpointError::Invalid(Issues::new(vec![Issue::new(
            "$",
            "não é um objeto JSON",
        )])));
    };
    check_schema_version(object)?;
    let issues = collect_issues(object);
    if !issues.is_empty() {
        return Err(CheckpointError::Invalid(Issues::new(issues)));
    }
    serde_json::from_value(value.clone()).map_err(|err| {
        CheckpointError::Invalid(Issues::new(vec![Issue::new("$", err.to_string())]))
    })
}

/// Campos conhecidos do esquema (rejeita o resto).
const KNOWN_FIELDS: &[&str] = &[
    "schema_version",
    "goal",
    "state",
    "pending",
    "next_action",
    "findings",
    "phase",
];

/// Recusa uma versão de esquema presente mas não suportada (falha-fechado dedicada).
fn check_schema_version(object: &Map<String, Value>) -> Result<(), CheckpointError> {
    let _span = crate::trace_fn!("kernel::checkpoint::check_schema_version");

    match object.get("schema_version").and_then(Value::as_u64) {
        Some(found) if found != u64::from(CHECKPOINT_SCHEMA_VERSION) => {
            Err(CheckpointError::SchemaVersion {
                found,
                expected: CHECKPOINT_SCHEMA_VERSION,
            })
        }
        _ => Ok(()),
    }
}

/// Agrega **todos** os problemas de esquema, com o caminho exato de cada um.
fn collect_issues(object: &Map<String, Value>) -> Vec<Issue> {
    let _span = crate::trace_fn!("kernel::checkpoint::collect_issues");

    let mut issues = Vec::new();
    if object
        .get("schema_version")
        .and_then(Value::as_u64)
        .is_none()
    {
        issues.push(Issue::new("schema_version", "inteiro obrigatório em falta"));
    }
    for key in object.keys() {
        if !KNOWN_FIELDS.contains(&key.as_str()) {
            issues.push(Issue::new(key.clone(), "campo desconhecido"));
        }
    }
    for field in ["goal", "state", "next_action"] {
        if object.get(field).and_then(Value::as_str).is_none() {
            issues.push(Issue::new(field, "texto obrigatório em falta"));
        }
    }
    for field in ["pending", "findings"] {
        if !is_string_array(object.get(field)) {
            issues.push(Issue::new(field, "lista de texto obrigatória"));
        }
    }
    match object.get("phase") {
        Some(phase) if serde_json::from_value::<Phase>(phase.clone()).is_ok() => {}
        Some(_) => issues.push(Issue::new("phase", "fase inválida")),
        None => issues.push(Issue::new("phase", "fase obrigatória em falta")),
    }
    issues
}

/// `true` se o valor é uma lista de textos.
fn is_string_array(value: Option<&Value>) -> bool {
    let _span = crate::trace_fn!("kernel::checkpoint::is_string_array");

    value
        .and_then(Value::as_array)
        .is_some_and(|items| items.iter().all(Value::is_string))
}

/// Escreve o checkpoint de forma atómica (`temp → fsync → rename`).
///
/// # Errors
/// [`CheckpointError`] se a serialização ou o I/O falharem.
pub fn save(fs: &dyn Fs, dir: &Path, checkpoint: &Checkpoint) -> Result<(), CheckpointError> {
    let _span = crate::fn_span!(
        Level::Trace,
        events::CONTEXT_CHECKPOINT,
        "kernel::checkpoint::save"
    );
    let mut bytes = serde_json::to_vec_pretty(checkpoint)
        .map_err(|err| CheckpointError::Parse(err.to_string()))?;
    bytes.push(b'\n');
    fs.write_atomic(&checkpoint_path(dir), &bytes)?;
    Ok(())
}

/// Lê e valida o checkpoint, se existir.
///
/// # Errors
/// [`CheckpointError`] se o ficheiro existir mas não validar.
pub fn load(fs: &dyn Fs, dir: &Path) -> Result<Option<Checkpoint>, CheckpointError> {
    let _span = crate::fn_span!(
        Level::Trace,
        events::CONTEXT_CHECKPOINT,
        "kernel::checkpoint::load"
    );
    let path = checkpoint_path(dir);
    if !fs.exists(&path) {
        return Ok(None);
    }
    let bytes = fs.read(&path)?;
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|err| CheckpointError::Parse(err.to_string()))?;
    validate(&value).map(Some)
}

#[cfg(test)]
mod tests;
