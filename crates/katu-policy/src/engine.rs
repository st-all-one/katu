//! Aplicação de regras a factos (E02-T03): âmbito → casamento → veredicto.
//!
//! Os helpers são puros; nada de I/O. A evidência é sempre estruturada.

use crate::decision::{ApprovalRequest, ControlId, Decision, Evidence, Reason};
use crate::facts::{BudgetState, Facts, Phase, ToolName};
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
                .map(|argument| self.evidence(&argument, "escrita sob raiz negada")),
            Enforcement::DenyDelete { root } => delete_hit(facts, root)
                .map(|argument| self.evidence(&argument, "envio para lixo sob raiz negada")),
            Enforcement::DenyCommand { tool } => facts
                .tool
                .name
                .eq(tool)
                .then(|| self.evidence(tool_name(*tool), "tool negada")),
            Enforcement::RequireBefore { phase } => (facts.phase < *phase)
                .then(|| self.evidence(phase_name(facts.phase), "fase anterior obrigatória")),
            Enforcement::RequireAfter { tool } => (!facts.completed.contains(tool))
                .then(|| self.evidence(tool_name(*tool), "dependência de tool não satisfeita")),
            Enforcement::Budget { cap } => budget_exceeded(facts.budget, *cap)
                .then(|| self.evidence("budget", "orçamento excedido")),
            Enforcement::Advisory => None,
        }
    }

    /// Converte uma aplicação em veredicto (a severidade decide entre negar e pedir aprovação).
    pub(crate) fn verdict(&self, evidence: Evidence) -> Decision {
        match &self.enforcement {
            Enforcement::DenyCommand { .. }
            | Enforcement::DenyWrite { .. }
            | Enforcement::DenyDelete { .. } => {
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
            Enforcement::RequireBefore { .. } | Enforcement::RequireAfter { .. } => {
                Decision::RequireApproval {
                    request: ApprovalRequest {
                        rule_id: self.id.clone(),
                        reason: Reason::new(self.statement.clone()),
                        scope: evidence.argument,
                    },
                }
            }
            Enforcement::Budget { .. } => Decision::NeedsHuman {
                reason: Reason::new("orçamento excedido"),
                missing_control: ControlId::new("budget"),
            },
            Enforcement::Advisory => Decision::Allow,
        }
    }
}

/// `true` se a tool escreve conteúdo.
fn is_write_tool(tool: ToolName) -> bool {
    matches!(tool, ToolName::Write | ToolName::Edit)
}

/// Nome estável de uma tool.
fn tool_name(tool: ToolName) -> &'static str {
    match tool {
        ToolName::Read => "read",
        ToolName::Write => "write",
        ToolName::Edit => "edit",
        ToolName::Trash => "trash",
        ToolName::Exec => "exec",
        ToolName::Search => "search",
        ToolName::Memory => "memory",
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
fn write_hit(facts: &Facts, root: &ResolvedPath) -> Option<String> {
    is_write_tool(facts.tool.name)
        .then(|| {
            facts
                .tool
                .resolved_paths
                .iter()
                .find(|path| path.is_under(root))
        })
        .flatten()
        .map(|path| path.as_str().to_string())
}

/// Primeiro caminho sob `root`, quando a tool envia para o lixo.
fn delete_hit(facts: &Facts, root: &ResolvedPath) -> Option<String> {
    facts
        .tool
        .name
        .eq(&ToolName::Trash)
        .then(|| {
            facts
                .tool
                .resolved_paths
                .iter()
                .find(|path| path.is_under(root))
        })
        .flatten()
        .map(|path| path.as_str().to_string())
}

#[cfg(test)]
mod tests {
    use super::Rule;
    use crate::error::PolicyError;
    use crate::facts::{BudgetState, Facts, Phase, Timestamp, ToolArgs, ToolName, ToolUse};
    use crate::paths::ResolvedPath;
    use crate::rule::{
        Enforcement, RuleCategory, RuleExamples, RuleId, RuleScope, Severity, Waiver,
    };
    use std::collections::BTreeSet;

    fn deny_write_rule(root: ResolvedPath) -> Rule {
        Rule {
            id: RuleId::from("no-write-secrets"),
            statement: "não escrever em segredos".to_string(),
            scope: RuleScope::Path { root: root.clone() },
            enforcement: Enforcement::DenyWrite { root },
            severity: Severity::Critical,
            category: RuleCategory::Enforced,
            expires_at: None,
            waiver: None,
            examples: RuleExamples::default(),
        }
    }

    fn facts(path: &str) -> Result<Facts, PolicyError> {
        let resolved = ResolvedPath::from_canonical(path)?;
        Ok(Facts {
            now_millis: 0,
            phase: Phase::Task,
            tool: ToolUse {
                name: ToolName::Write,
                args: ToolArgs::Write {
                    path: resolved.clone(),
                    bytes: 1,
                },
                resolved_paths: vec![resolved.clone()],
                argv: None,
                cwd: resolved,
            },
            capabilities: Vec::new(),
            budget: BudgetState::default(),
            completed: BTreeSet::new(),
        })
    }

    #[test]
    fn expired_rule_is_inactive() -> Result<(), PolicyError> {
        let root = ResolvedPath::from_canonical("/work/secrets")?;
        let mut rule = deny_write_rule(root);
        rule.expires_at = Some(Timestamp::from_millis(0));
        let facts = facts("/work/secrets/token")?;
        assert!(rule.applies(&facts).is_none());
        Ok(())
    }

    #[test]
    fn waived_rule_is_inactive() -> Result<(), PolicyError> {
        let root = ResolvedPath::from_canonical("/work/secrets")?;
        let mut rule = deny_write_rule(root);
        rule.waiver = Some(Waiver {
            reason: "exceção temporária".to_string(),
            expires_at: None,
        });
        let facts = facts("/work/secrets/token")?;
        assert!(rule.applies(&facts).is_none());
        Ok(())
    }
}
