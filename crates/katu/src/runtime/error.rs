//! Erros do runtime (extraído para manter `runtime.rs` sob o teto de linhas).

use katu_core::error::Error;
use katu_core::kernel::{ControlError, SessionError};
use katu_core::memory::MemoryError;
use katu_policy::PolicyError;

use crate::scope::ScopeError;

/// Falha ao montar ou operar o runtime.
#[derive(Debug, thiserror::Error)]
pub(crate) enum RuntimeError {
    /// Falha da porta de memória.
    #[error("memória: {0}")]
    Memory(#[from] MemoryError),
    /// Falha da sessão (log, transição, custo ou política).
    #[error("sessão: {0}")]
    Session(#[from] SessionError),
    /// Vocabulário de política inválido.
    #[error("política: {0}")]
    Policy(#[from] PolicyError),
    /// Artefacto de plano (`scope_contract`/`feature_list`) inválido (E09-T04).
    #[error("escopo: {0}")]
    Scope(#[from] ScopeError),
    /// O gate de verificação (E09-T03) não pôde correr (falta o escopo).
    #[error("verificação: {0}")]
    Verification(String),
    /// Controlo de modelo/pensamento inválido (E12-T10).
    #[error("controlo: {0}")]
    Control(#[from] ControlError),
    /// Não foi possível retomar a sessão (id inválido/desconhecido ou nenhuma sessão).
    #[error("retomada: {0}")]
    Resume(String),
}

impl From<RuntimeError> for Error {
    fn from(error: RuntimeError) -> Self {
        match error {
            RuntimeError::Memory(source) => Self::unavailable(source.to_string()),
            RuntimeError::Policy(source) => Self::invalid_input(source.to_string()),
            RuntimeError::Scope(source) => Self::invalid_input(source.to_string()),
            RuntimeError::Verification(message) | RuntimeError::Resume(message) => {
                Self::invalid_input(message)
            }
            RuntimeError::Control(source) => Self::invalid_input(source.to_string()),
            RuntimeError::Session(source) => {
                if matches!(source, SessionError::UnknownSession(_)) {
                    Self::invalid_input(source.to_string())
                } else {
                    Self::internal(source.to_string())
                }
            }
        }
    }
}
