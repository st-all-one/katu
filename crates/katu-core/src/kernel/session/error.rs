//! Erro de uma operação de sessão.

use crate::kernel::checkpoint::CheckpointError;
use crate::kernel::cost::CostRefusal;
use crate::kernel::log::LogError;
use crate::kernel::memory_gate::MemoryWriteError;
use crate::kernel::state::Refusal;
use crate::ports::FsError;
use katu_policy::PolicyError;

/// Erro de uma operação de sessão.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SessionError {
    /// Falha no log.
    #[error("log: {0}")]
    Log(#[from] LogError),
    /// Transição recusada (fail-closed).
    #[error("recusa: {0}")]
    Refusal(#[from] Refusal),
    /// Vocabulário de política inválido.
    #[error("política: {0}")]
    Policy(#[from] PolicyError),
    /// Teto do cost governor atingido (E09-T06).
    #[error("custo: {0}")]
    Cost(#[from] CostRefusal),
    /// Falha no checkpoint de fase.
    #[error("checkpoint: {0}")]
    Checkpoint(#[from] CheckpointError),
    /// Falha ao avaliar uma escrita de memória (E05-T04).
    #[error("memória: {0}")]
    MemoryWrite(#[from] MemoryWriteError),
    /// Falha de sistema de ficheiros.
    #[error("fs: {0}")]
    Fs(#[from] FsError),
    /// Invariante `Model-visible ⟺ logged` violada (§42).
    #[error("invariante: {0}")]
    Invariant(String),
}
