//! Aplicação de regras a factos (E02-T03): âmbito → casamento → veredicto.
//!
//! Os helpers são puros; nada de I/O. A evidência é sempre estruturada.

use crate::decision::{ApprovalRequest, ControlId, Decision, Evidence, Reason};
use crate::facts::{BudgetState, Capability, Facts, Phase, ToolName};
use crate::paths::ResolvedPath;
use crate::rule::{BudgetCap, Enforcement, Rule, RuleScope, Severity};

impl Rule {
    /// Constrói uma evidência a partir de uma regra.
    fn evidence(&self, argument: &str, fact: &str) -> Evidence {
        Evidence {
            file_line: None,
            fact: fact.to_string(),
            argument: argument.to_string(),
            rule_id: self.id.clone(),
        }
    }

    /// `true` se o âmbito da regra cobre os factos.
    pub(crate) fn scope_matches(&self, facts: &Facts) -> bool {
        match &self.scope {
            RuleScope::Path { root } => facts
                .tool
                .resolved_paths
                .iter()
                .any(|path| path.is_under(root)),
            RuleScope::Command { tool } => facts.tool.name == *tool,
            RuleScope::Phase { phase } => facts.phase == *phase,
            RuleScope::Budget { .. } => true,
        }
    }

    /// Devolve evidência se a regra se aplica aos factos.
    pub(crate) fn applies(&self, facts: &Facts) -> Option<Evidence> {
        if !self.is_active(facts.now_millis) || !self.scope_matches(facts) {
            return None;
        }
        match &self.enforcement {
            Enforcement::DenyWrite { root } => write_hit(facts, root)
                .filter(|path| !write_capability_covers(facts, path))
                .map(|path| self.evidence(path.as_str(), "escrita sob raiz negada sem capacidade")),
            Enforcement::DenyRead { root } => read_hit(facts, root)
                .filter(|path| !read_capability_covers(facts, path))
                .map(|path| self.evidence(path.as_str(), "leitura sob raiz negada sem capacidade")),
            Enforcement::DenySensitiveRead { globs } => sensitive_read_hit(facts, globs)
                .filter(|path| !explicit_read_capability_covers(facts, path))
                .map(|path| {
                    self.evidence(path.as_str(), "caminho sensível negado sem autorização")
                }),
            Enforcement::DenyDelete { root } => delete_hit(facts, root)
                .filter(|path| !delete_capability_covers(facts, path))
                .map(|path| {
                    self.evidence(
                        path.as_str(),
                        "envio para lixo sob raiz negada sem capacidade",
                    )
                }),
            Enforcement::DenyCommand { tool } => (facts.tool.name == *tool
                && !command_capability(facts, *tool))
            .then(|| self.evidence(tool_name(*tool), "comando negado sem capacidade")),
            Enforcement::RequireBefore { phase } => (facts.phase < *phase)
                .then(|| self.evidence(phase_name(facts.phase), "fase anterior obrigatória")),
            Enforcement::RequireAfter { tool } => (!facts.completed.contains(tool))
                .then(|| self.evidence(tool_name(*tool), "dependência de tool não satisfeita")),
            Enforcement::Budget { cap } => budget_exceeded(facts.budget, *cap)
                .then(|| self.evidence("budget", "orçamento excedido")),
            Enforcement::Advisory => None,
        }
    }

    /// Converte uma aplicação em veredicto: a **severidade** decide entre negar e pedir aprovação
    /// (DF11). `Budget` é sempre `NeedsHuman`; `Advisory` é `Allow`.
    pub(crate) fn verdict(&self, evidence: Evidence) -> Decision {
        match &self.enforcement {
            Enforcement::DenyCommand { .. }
            | Enforcement::DenyWrite { .. }
            | Enforcement::DenyRead { .. }
            | Enforcement::DenySensitiveRead { .. }
            | Enforcement::DenyDelete { .. }
            | Enforcement::RequireBefore { .. }
            | Enforcement::RequireAfter { .. } => self.severity_verdict(evidence),
            Enforcement::Budget { .. } => Decision::NeedsHuman {
                reason: Reason::new("orçamento excedido"),
                missing_control: ControlId::new("budget"),
            },
            Enforcement::Advisory => Decision::Allow,
        }
    }

    /// `critical` nega (muro); `warn` pede aprovação (soft). DF11.
    fn severity_verdict(&self, evidence: Evidence) -> Decision {
        if self.severity == Severity::Warn {
            Decision::RequireApproval {
                request: ApprovalRequest {
                    rule_id: self.id.clone(),
                    reason: Reason::new(self.statement.clone()),
                    scope: evidence.argument,
                },
            }
        } else {
            Decision::Deny {
                reason: Reason::new(self.statement.clone()),
                rule_id: self.id.clone(),
                evidence,
            }
        }
    }
}

/// `true` se a tool escreve conteúdo.
fn is_write_tool(tool: ToolName) -> bool {
    matches!(tool, ToolName::Write | ToolName::Edit | ToolName::Move)
}

/// `true` se a tool lê conteúdo diretamente (`read`).
///
/// A busca (`grep`/`find`/`ls`) fica para o chamador decidir — o `ToolUse` de leitura tem de
/// trazer os caminhos resolvidos para a política os poder avaliar (E07-T05).
fn is_read_tool(tool: ToolName) -> bool {
    tool == ToolName::Read
}

/// Nome estável de uma tool.
fn tool_name(tool: ToolName) -> &'static str {
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
fn phase_name(phase: Phase) -> &'static str {
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

/// `true` se o orçamento esgotou o teto.
fn budget_exceeded(state: BudgetState, cap: BudgetCap) -> bool {
    match cap {
        BudgetCap::Writes(limit) => state.writes >= limit,
        BudgetCap::Bytes(limit) => state.bytes >= limit,
        BudgetCap::Execs(limit) => state.execs >= limit,
    }
}

/// Primeiro caminho sob `root`, quando a tool escreve.
fn write_hit<'a>(facts: &'a Facts, root: &ResolvedPath) -> Option<&'a ResolvedPath> {
    if !is_write_tool(facts.tool.name) {
        return None;
    }
    facts
        .tool
        .resolved_paths
        .iter()
        .find(|path| path.is_under(root))
}

/// Primeiro caminho sob `root`, quando a tool lê.
fn read_hit<'a>(facts: &'a Facts, root: &ResolvedPath) -> Option<&'a ResolvedPath> {
    if !is_read_tool(facts.tool.name) {
        return None;
    }
    facts
        .tool
        .resolved_paths
        .iter()
        .find(|path| path.is_under(root))
}

/// Primeiro caminho cujo **componente** casa um glob sensível.
fn sensitive_read_hit<'a>(facts: &'a Facts, globs: &[String]) -> Option<&'a ResolvedPath> {
    if !is_read_tool(facts.tool.name) {
        return None;
    }
    facts
        .tool
        .resolved_paths
        .iter()
        .find(|path| path_has_sensitive_component(path, globs))
}

/// `true` se algum componente do caminho casa um dos globs.
fn path_has_sensitive_component(path: &ResolvedPath, globs: &[String]) -> bool {
    path.as_str().split('/').any(|component| {
        globs
            .iter()
            .any(|glob| crate::matches_glob(glob, component))
    })
}

/// `true` se uma capacidade de leitura cobre o caminho (destranca `DenyRead`).
///
/// Aceita o workspace (implícito) **ou** um `ReadPath` explícito.
fn read_capability_covers(facts: &Facts, path: &ResolvedPath) -> bool {
    facts.capabilities.iter().any(|cap| match cap {
        Capability::ReadPath { root } | Capability::Workspace { root } => path.is_under(root),
        _ => false,
    })
}

/// `true` se um `ReadPath` **explícito** cobre o caminho (destranca `DenySensitiveRead`).
///
/// O workspace **não** conta: sensíveis exigem autorização explícita (E07-T05).
fn explicit_read_capability_covers(facts: &Facts, path: &ResolvedPath) -> bool {
    facts
        .capabilities
        .iter()
        .any(|cap| matches!(cap, Capability::ReadPath { root } if path.is_under(root)))
}

/// Primeiro caminho sob `root`, quando a tool envia para o lixo.
fn delete_hit<'a>(facts: &'a Facts, root: &ResolvedPath) -> Option<&'a ResolvedPath> {
    if facts.tool.name != ToolName::Trash {
        return None;
    }
    facts
        .tool
        .resolved_paths
        .iter()
        .find(|path| path.is_under(root))
}

/// `true` se uma capacidade de escrita cobre o caminho (destranca `DenyWrite`).
fn write_capability_covers(facts: &Facts, path: &ResolvedPath) -> bool {
    facts.capabilities.iter().any(|cap| match cap {
        Capability::WritePath { root } | Capability::Workspace { root } => path.is_under(root),
        _ => false,
    })
}

/// `true` se uma capacidade de apagar cobre o caminho (destranca `DenyDelete`).
fn delete_capability_covers(facts: &Facts, path: &ResolvedPath) -> bool {
    facts
        .capabilities
        .iter()
        .any(|cap| matches!(cap, Capability::DeletePath { root } if path.is_under(root)))
}

/// `true` se o comando foi concedido (destranca `DenyCommand`).
///
/// `Capability::Command` é nominal (concede a tool inteira); `Capability::Exec { program }` só
/// destranca um `argv` **verificável** e não destrutivo cujo programa casa exatamente (E07-T02).
fn command_capability(facts: &Facts, tool: ToolName) -> bool {
    facts.capabilities.iter().any(|cap| match cap {
        Capability::Command { tool: granted } => *granted == tool,
        Capability::Exec { program } => {
            tool == ToolName::Exec && exec_program_covers(facts, program)
        }
        _ => false,
    })
}

/// `true` se `program` concede o `argv` corrente e este é **verificável** (não opaco/destrutivo).
fn exec_program_covers(facts: &Facts, program: &str) -> bool {
    facts.tool.argv.as_ref().is_some_and(|argv| {
        let inspection = crate::inspect(argv);
        inspection.program == program && inspection.is_plain()
    })
}

#[cfg(test)]
mod tests;
