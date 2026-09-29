//! Erros da porta `Memory` (contrato próprio do katu; o binário mapeia `kind`→exit code).

use serde::{Deserialize, Serialize};

/// Natureza do erro de memória.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum MemoryErrorKind {
    /// Backend indisponível.
    Unavailable,
    /// Operação excedeu o tempo-limite.
    Timeout,
    /// Pedido inválido.
    Invalid,
    /// Falha interna.
    Internal,
}

/// Erro da porta `Memory`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{message}")]
pub struct MemoryError {
    /// Natureza do erro.
    pub kind: MemoryErrorKind,
    /// Mensagem legível (sem segredos).
    pub message: String,
}

impl MemoryError {
    /// Constrói um erro com a natureza e a mensagem dadas.
    pub fn new(kind: MemoryErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// Backend indisponível.
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(MemoryErrorKind::Unavailable, message)
    }

    /// Operação excedeu o tempo-limite.
    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(MemoryErrorKind::Timeout, message)
    }

    /// Pedido inválido.
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(MemoryErrorKind::Invalid, message)
    }

    /// Falha interna.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(MemoryErrorKind::Internal, message)
    }

    /// Natureza do erro.
    #[must_use]
    pub const fn kind(&self) -> MemoryErrorKind {
        self.kind
    }

    /// `true` apenas para [`MemoryErrorKind::Timeout`] (o único erro retentável).
    #[must_use]
    pub const fn retryable(&self) -> bool {
        matches!(self.kind, MemoryErrorKind::Timeout)
    }
}
