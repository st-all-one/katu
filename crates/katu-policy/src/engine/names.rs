//! Nomes estáveis de tools e fases (E02-T03), extraídos para manter `engine/mod.rs` sob o teto.

use crate::facts::{Phase, ToolName};

/// Nome estável de uma tool.
pub(super) fn tool_name(tool: ToolName) -> &'static str {
    match tool {
        ToolName::Read => "read",
        ToolName::Write => "write",
        ToolName::Edit => "edit",
        ToolName::Move => "move",
        ToolName::Trash => "trash",
        ToolName::Exec => "exec",
        ToolName::Search => "search",
        ToolName::MemoryRecall => "memory_recall",
        ToolName::MemoryWrite => "memory_write",
        ToolName::MemoryOutcome => "memory_outcome",
        ToolName::MemoryClose => "memory_close",
        ToolName::Plan => "plan",
        ToolName::Compact => "compact",
        ToolName::Model => "model",
        ToolName::Thinking => "thinking",
    }
}

/// Nome estável de uma fase.
pub(super) fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Task => "task",
        Phase::KnowledgeConsulted => "knowledge_consulted",
        Phase::Planned => "planned",
        Phase::Implemented => "implemented",
        Phase::Verified => "verified",
        Phase::Persisted => "persisted",
        Phase::Closed => "closed",
    }
}
