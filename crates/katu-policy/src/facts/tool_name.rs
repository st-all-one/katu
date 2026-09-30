//! Nome de tool (vocabulário fechado; novas tools = decisão de kernel).

use serde::{Deserialize, Serialize};

/// Nome de tool (vocabulário fechado; novas tools = decisão de kernel).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToolName {
    /// Leitura.
    Read,
    /// Escrita.
    Write,
    /// Edição.
    Edit,
    /// Mover/renomear.
    Move,
    /// Lixo recuperável.
    Trash,
    /// Execução de comando.
    Exec,
    /// Busca.
    Search,
    /// Memória: consulta (recall).
    MemoryRecall,
    /// Memória: gravação de nota.
    MemoryWrite,
    /// Memória: registo de `outcome`.
    MemoryOutcome,
    /// Memória: fecho de tarefa.
    MemoryClose,
    /// Planeamento.
    Plan,
    /// Compactação.
    Compact,
    /// Modelo (controlo do utilizador).
    Model,
    /// Thinking (controlo do utilizador).
    Thinking,
}

impl ToolName {
    /// Nome estável (`snake_case`) — vocabulário do modelo, do log e do registo.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Edit => "edit",
            Self::Move => "move",
            Self::Trash => "trash",
            Self::Exec => "exec",
            Self::Search => "search",
            Self::MemoryRecall => "memory_recall",
            Self::MemoryWrite => "memory_write",
            Self::MemoryOutcome => "memory_outcome",
            Self::MemoryClose => "memory_close",
            Self::Plan => "plan",
            Self::Compact => "compact",
            Self::Model => "model",
            Self::Thinking => "thinking",
        }
    }

    /// `true` se a tool altera ficheiros no disco (entra no *diff* da verificação, E09-T03).
    #[must_use]
    pub const fn is_file_change(self) -> bool {
        matches!(self, Self::Write | Self::Edit | Self::Move | Self::Trash)
    }

    /// Converte um nome estável de volta para o enum (vocabulário fechado).
    ///
    /// Devolve `None` para um nome desconhecido — o adaptador de provider decide o que fazer
    /// (no katu, uma tool fora do catálogo é recusada, fail-closed §51.9).
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "read" => Self::Read,
            "write" => Self::Write,
            "edit" => Self::Edit,
            "move" => Self::Move,
            "trash" => Self::Trash,
            "exec" => Self::Exec,
            "search" => Self::Search,
            "memory_recall" => Self::MemoryRecall,
            "memory_write" => Self::MemoryWrite,
            "memory_outcome" => Self::MemoryOutcome,
            "memory_close" => Self::MemoryClose,
            "plan" => Self::Plan,
            "compact" => Self::Compact,
            "model" => Self::Model,
            "thinking" => Self::Thinking,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ToolName;

    #[test]
    fn only_disk_mutating_tools_are_file_changes() {
        assert!(ToolName::Write.is_file_change());
        assert!(ToolName::Edit.is_file_change());
        assert!(ToolName::Move.is_file_change());
        assert!(ToolName::Trash.is_file_change());
        assert!(!ToolName::Read.is_file_change());
        assert!(!ToolName::Exec.is_file_change());
    }
}
