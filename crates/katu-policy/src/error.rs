//! Erros do motor de política (E02). Sem I/O: só validade de facto e de vocabulário.

/// Erro do motor de política.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PolicyError {
    /// Caminho não absoluto (a resolução é feita antes do veredicto).
    #[error("caminho não absoluto: {0}")]
    NonAbsolutePath(String),
    /// `argv` vazio ou inválido.
    #[error("argv inválido: {0}")]
    InvalidArgv(String),
    /// Vocabulário de política desconhecido (fail-closed).
    #[error("vocabulário de política desconhecido: {found} (esperado {expected})")]
    UnknownVocab {
        /// Versão encontrada.
        found: u32,
        /// Versão esperada pelo motor.
        expected: u32,
    },
    /// TOML inválido.
    #[error("TOML inválido: {0}")]
    Toml(String),
}
