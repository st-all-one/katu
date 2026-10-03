//! Dados da lixeira no protocolo (E10-T07/E06-T09).

use serde::{Deserialize, Serialize};

/// Entrada da lixeira mostrada na UI (subset do índice, E06-T09).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrashEntry {
    /// Caminho original.
    pub original: String,
    /// Token guardado (usado para restaurar).
    pub stored: String,
}
