//! Contabilização de tokens (E12-T03), com a base de evidência (DF5).
//!
//! Vive fora de `provider.rs` para respeitar o limite de 300 linhas por ficheiro; faz parte da
//! **porta** (`Provider`) e é reexportado por ela.

use serde::{Deserialize, Serialize};

use crate::evidence::EvidenceBasis;

/// Contabilização de tokens de uma chamada, com a sua base (DF5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    /// Tokens de entrada.
    pub input: Option<u64>,
    /// Tokens de saída.
    pub output: Option<u64>,
    /// Tokens de entrada servidos por prefix-cache (E18/F4), quando reportado.
    pub cached_input: Option<u64>,
    /// Tokens de raciocínio (thinking), quando reportado.
    pub reasoning: Option<u64>,
    /// Base de evidência dos números.
    pub basis: EvidenceBasis,
}

impl TokenUsage {
    /// Nova contabilização com a base indicada.
    #[must_use]
    pub const fn new(basis: EvidenceBasis) -> Self {
        Self {
            input: None,
            output: None,
            cached_input: None,
            reasoning: None,
            basis,
        }
    }
}
