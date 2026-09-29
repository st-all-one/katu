//! Registry fechado de tools (E06-T01).
//!
//! A superfície é **exatamente** as famílias de `00b` §1.1; qualquer adição exige uma decisão
//! registada (DF12). A ordem é canónica (estável entre execuções).

use katu_policy::ToolName;

/// Família de tool (§1.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// Escrita: `write`, `edit`, `move`, `trash`.
    Write,
    /// Leitura: `read`.
    Read,
    /// Execução: `bash`.
    Exec,
    /// Pesquisa: `grep`, `find`, `ls`.
    Search,
    /// Planeamento: `plan`.
    Plan,
}

impl Family {
    /// Nome estável.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Write => "write",
            Self::Read => "read",
            Self::Exec => "exec",
            Self::Search => "search",
            Self::Plan => "plan",
        }
    }
}

/// Identidade **do modelo** de uma tool (a superfície fechada).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolId {
    /// Escrever ficheiro.
    Write,
    /// Editar ficheiro (patch).
    Edit,
    /// Mover/renomear.
    Move,
    /// Lixeira recuperável.
    Trash,
    /// Ler ficheiro (views).
    Read,
    /// Executar comando.
    Bash,
    /// Busca por texto.
    Grep,
    /// Busca por ficheiro.
    Find,
    /// Listar diretório.
    Ls,
    /// Planeamento.
    Plan,
}

impl ToolId {
    /// Nome estável ao modelo.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Write => "write",
            Self::Edit => "edit",
            Self::Move => "move",
            Self::Trash => "trash",
            Self::Read => "read",
            Self::Bash => "bash",
            Self::Grep => "grep",
            Self::Find => "find",
            Self::Ls => "ls",
            Self::Plan => "plan",
        }
    }
}

/// Entrada do registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolSpec {
    /// Identidade ao modelo.
    pub id: ToolId,
    /// Nome no vocabulário de política.
    pub name: ToolName,
    /// Família.
    pub family: Family,
}

/// O registry **fechado**, em ordem canónica.
pub const TOOLS: &[ToolSpec] = &[
    ToolSpec {
        id: ToolId::Write,
        name: ToolName::Write,
        family: Family::Write,
    },
    ToolSpec {
        id: ToolId::Edit,
        name: ToolName::Edit,
        family: Family::Write,
    },
    ToolSpec {
        id: ToolId::Move,
        name: ToolName::Move,
        family: Family::Write,
    },
    ToolSpec {
        id: ToolId::Trash,
        name: ToolName::Trash,
        family: Family::Write,
    },
    ToolSpec {
        id: ToolId::Read,
        name: ToolName::Read,
        family: Family::Read,
    },
    ToolSpec {
        id: ToolId::Bash,
        name: ToolName::Exec,
        family: Family::Exec,
    },
    ToolSpec {
        id: ToolId::Grep,
        name: ToolName::Search,
        family: Family::Search,
    },
    ToolSpec {
        id: ToolId::Find,
        name: ToolName::Search,
        family: Family::Search,
    },
    ToolSpec {
        id: ToolId::Ls,
        name: ToolName::Search,
        family: Family::Search,
    },
    ToolSpec {
        id: ToolId::Plan,
        name: ToolName::Plan,
        family: Family::Plan,
    },
];

/// `true` se a tool pertence à superfície fechada.
#[must_use]
pub fn is_registered(name: ToolName) -> bool {
    TOOLS.iter().any(|spec| spec.name == name)
}

#[cfg(test)]
mod tests {
    use super::{Family, TOOLS, ToolId, is_registered};
    use katu_policy::ToolName;
    use std::collections::BTreeSet;

    #[test]
    fn surface_is_exactly_the_ten_tools() {
        assert_eq!(TOOLS.len(), 10);
        let ids: BTreeSet<&str> = TOOLS.iter().map(|spec| spec.id.as_str()).collect();
        assert_eq!(ids.len(), TOOLS.len(), "ids duplicados");
    }

    #[test]
    fn families_match_the_core_surface() {
        let count = |family: Family| TOOLS.iter().filter(|spec| spec.family == family).count();
        assert_eq!(count(Family::Write), 4);
        assert_eq!(count(Family::Read), 1);
        assert_eq!(count(Family::Exec), 1);
        assert_eq!(count(Family::Search), 3);
        assert_eq!(count(Family::Plan), 1);
    }

    #[test]
    fn registered_names_and_foreign_names() {
        for spec in TOOLS {
            assert!(is_registered(spec.name));
        }
        assert!(!is_registered(ToolName::MemoryRecall));
        assert!(!is_registered(ToolName::Compact));
        assert!(!is_registered(ToolName::Model));
    }

    #[test]
    fn tool_ids_are_stable_lowercase() {
        for spec in TOOLS {
            let id = spec.id.as_str();
            assert!(id.chars().all(|c| c.is_ascii_lowercase()), "{id}");
        }
        assert_eq!(ToolId::Bash.as_str(), "bash");
    }
}
