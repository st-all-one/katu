//! Checkpoint tipado no limite de fase (E04-T07), validado contra esquema e escrito atomicamente.
//!
//! O checkpoint é o `.STAGING.md` do maxima promovido de prosa a **artefacto de fase**
//! (`plan/10` §2): `temp → fsync → rename` pela porta [`Fs`] (`write_atomic`). O validador é
//! **zero-dep** (só `serde`): verifica a versão de esquema, rejeita campos desconhecidos e só
//! depois desserializa para o tipo [`Checkpoint`]. Um checkpoint no limite de fase é o gatilho
//! (§51.8) — nunca um limiar heurístico de NLU.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::state::{CallStatus, State};
use crate::diag::{Level, events};
use crate::ports::{Fs, FsError};
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
        let pending: Vec<String> = state
            .calls
            .iter()
            .filter(|(_, status)| matches!(status, CallStatus::Pending { .. }))
            .map(|(call, _)| call.as_str().to_string())
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
    /// Conteúdo sintaticamente válido mas fora do esquema (campo em falta ou desconhecido).
    #[error("checkpoint inválido: {0}")]
    Invalid(String),
    /// Falha de I/O.
    #[error("I/O do checkpoint: {0}")]
    Io(#[from] FsError),
}

/// Caminho canónico do checkpoint de uma sessão.
#[must_use]
pub fn checkpoint_path(dir: &Path) -> PathBuf {
    dir.join("checkpoint.json")
}

/// Valida um `Value` contra o esquema e devolve o checkpoint tipado.
///
/// # Errors
/// [`CheckpointError`] se não for um objeto, faltar `schema_version`, a versão não casar, houver
/// campos desconhecidos ou algum campo tiver o tipo errado.
pub fn validate(value: &Value) -> Result<Checkpoint, CheckpointError> {
    let object = value
        .as_object()
        .ok_or_else(|| CheckpointError::Invalid("não é um objeto JSON".to_string()))?;
    let found = object
        .get("schema_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| CheckpointError::Invalid("schema_version ausente ou não inteiro".into()))?;
    if found != u64::from(CHECKPOINT_SCHEMA_VERSION) {
        return Err(CheckpointError::SchemaVersion {
            found,
            expected: CHECKPOINT_SCHEMA_VERSION,
        });
    }
    serde_json::from_value(value.clone()).map_err(|err| CheckpointError::Invalid(err.to_string()))
}

/// Escreve o checkpoint de forma atómica (`temp → fsync → rename`).
///
/// # Errors
/// [`CheckpointError`] se a serialização ou o I/O falharem.
pub fn save(fs: &dyn Fs, dir: &Path, checkpoint: &Checkpoint) -> Result<(), CheckpointError> {
    let _span = crate::span!(Level::Trace, events::CONTEXT_CHECKPOINT);
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
mod tests {
    use super::{CHECKPOINT_SCHEMA_VERSION, Checkpoint, CheckpointError, load, save, validate};
    use crate::kernel::event::{CallId, Event};
    use crate::kernel::state::State;
    use crate::kernel::step::step;
    use crate::ports::{Fs, MemFs};
    use katu_policy::{Phase, ResolvedPath, ToolArgs, ToolName, ToolUse};
    use serde_json::{Value, json};
    use std::path::Path;

    fn write_tool() -> Result<ToolUse, katu_policy::PolicyError> {
        let path = ResolvedPath::from_canonical("/work/src/main.rs")?;
        Ok(ToolUse {
            name: ToolName::Write,
            args: ToolArgs::Write {
                path: path.clone(),
                bytes: 1,
            },
            resolved_paths: vec![path.clone()],
            argv: None,
            cwd: path,
        })
    }

    fn valid_value() -> Value {
        json!({
            "schema_version": CHECKPOINT_SCHEMA_VERSION,
            "goal": "provar a tese",
            "state": "Planned turn=1",
            "pending": ["c1"],
            "next_action": "implementar",
            "findings": [],
            "phase": "planned",
        })
    }

    #[test]
    fn valid_value_parses() -> Result<(), CheckpointError> {
        let checkpoint = validate(&valid_value())?;
        assert_eq!(checkpoint.schema_version, CHECKPOINT_SCHEMA_VERSION);
        assert_eq!(checkpoint.phase, Phase::Planned);
        assert_eq!(checkpoint.pending, vec!["c1".to_string()]);
        Ok(())
    }

    #[test]
    fn missing_field_is_invalid() {
        let mut value = valid_value();
        if let Some(object) = value.as_object_mut() {
            object.remove("goal");
        }
        assert!(matches!(validate(&value), Err(CheckpointError::Invalid(_))));
    }

    #[test]
    fn unknown_field_is_rejected() {
        let mut value = valid_value();
        if let Some(object) = value.as_object_mut() {
            object.insert("surpresa".to_string(), json!(true));
        }
        assert!(matches!(validate(&value), Err(CheckpointError::Invalid(_))));
    }

    #[test]
    fn wrong_schema_version_is_rejected() {
        let mut value = valid_value();
        if let Some(object) = value.as_object_mut() {
            object.insert("schema_version".to_string(), json!(99));
        }
        assert!(matches!(
            validate(&value),
            Err(CheckpointError::SchemaVersion { found: 99, .. })
        ));
    }

    #[test]
    fn absent_and_corrupt_are_distinct() -> Result<(), CheckpointError> {
        let fs = MemFs::new();
        let dir = Path::new("/sessions");
        assert_eq!(load(&fs, dir)?, None);

        fs.write_atomic(&super::checkpoint_path(dir), b"{nao json")?;
        assert!(matches!(load(&fs, dir), Err(CheckpointError::Parse(_))));
        Ok(())
    }

    #[test]
    fn round_trip_write_and_read() -> Result<(), Box<dyn std::error::Error>> {
        let fs = MemFs::new();
        let dir = Path::new("/sessions");
        let checkpoint = Checkpoint {
            schema_version: CHECKPOINT_SCHEMA_VERSION,
            goal: "g".to_string(),
            state: "s".to_string(),
            pending: Vec::new(),
            next_action: "n".to_string(),
            findings: vec!["f".to_string()],
            phase: Phase::Verified,
        };
        save(&fs, dir, &checkpoint)?;
        assert_eq!(load(&fs, dir)?, Some(checkpoint));
        Ok(())
    }

    #[test]
    fn from_state_lists_pending_calls() -> Result<(), Box<dyn std::error::Error>> {
        let mut state = State::initial();
        state = step(&state, &Event::TurnStart { turn: 1 })?;
        state = step(
            &state,
            &Event::ToolCall {
                call: CallId::new("c1"),
                tool: write_tool()?,
            },
        )?;
        let checkpoint = Checkpoint::from_state(&state, "objetivo", "próxima");
        assert_eq!(checkpoint.phase, Phase::Task);
        assert_eq!(checkpoint.pending, vec!["c1".to_string()]);
        assert_eq!(checkpoint.goal, "objetivo");
        Ok(())
    }
}
