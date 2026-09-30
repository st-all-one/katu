//! Auditoria de regras (E02-T04): categorias, exemplos negativos e duplicados.
//!
//! O audit é **puro** e determinístico: recebe um [`RuleSet`] e devolve o que está `Enforced` e o
//! que é `Advisory`/`Perception`, além de uma lista de problemas. A linha dura: uma regra
//! `Enforced` **tem** de trazer um exemplo negativo que ela nega (§51.7); sem isso, ou se classifica
//! como `Advisory`, ou o audit falha.

use std::collections::BTreeSet;
use std::fmt;

use crate::rule::{Enforcement, RuleCategory, RuleId, RuleSet};

/// Resumo auditável de uma regra.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSummary {
    /// Identificador.
    pub id: RuleId,
    /// Categoria declarada.
    pub category: RuleCategory,
    /// Vigência da regra.
    pub activity: Activity,
    /// Cobertura de exemplo negativo.
    pub examples: ExampleCoverage,
}

/// Vigência de uma regra no instante da auditoria.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    /// Ativa (não expirada nem suspensa).
    Active,
    /// Expirada ou suspensa por `waiver`.
    Inactive,
}

/// Cobertura de exemplo negativo (obrigatória para `Enforced`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExampleCoverage {
    /// Declara pelo menos um exemplo negativo.
    HasNegative,
    /// Não declara exemplo negativo.
    MissingNegative,
}

/// Problema encontrado pelo audit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditIssue {
    /// Regra `Enforced` sem exemplo negativo que ela negue (§51.7).
    EnforcedWithoutNegativeExample {
        /// Regra em falta.
        id: RuleId,
    },
    /// Regra com `enforcement = Advisory` mas categoria `Enforced` (contradição).
    AdvisoryEnforcementMarkedEnforced {
        /// Regra contraditória.
        id: RuleId,
    },
    /// Dois regras partilham o mesmo `id`.
    DuplicateRuleId {
        /// Identificador duplicado.
        id: RuleId,
    },
    /// `statement` vazio (texto de instrução sem conteúdo).
    EmptyStatement {
        /// Regra sem enunciado.
        id: RuleId,
    },
}

impl fmt::Display for AuditIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EnforcedWithoutNegativeExample { id } => {
                write!(
                    f,
                    "regra `{}` é Enforced mas não tem exemplo negativo",
                    id.as_str()
                )
            }
            Self::AdvisoryEnforcementMarkedEnforced { id } => write!(
                f,
                "regra `{}` tem enforcement Advisory mas categoria Enforced",
                id.as_str()
            ),
            Self::DuplicateRuleId { id } => {
                write!(f, "identificador de regra duplicado: `{}`", id.as_str())
            }
            Self::EmptyStatement { id } => {
                write!(f, "regra `{}` tem enunciado vazio", id.as_str())
            }
        }
    }
}

/// Relatório de auditoria: listas `Enforced`/`Advisory` e problemas.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuditReport {
    /// Regras `Enforced`.
    pub enforced: Vec<RuleSummary>,
    /// Regras `Advisory`/`Perception`.
    pub advisory: Vec<RuleSummary>,
    /// Problemas encontrados.
    pub issues: Vec<AuditIssue>,
}

impl AuditReport {
    /// `true` se não há problemas.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.issues.is_empty()
    }
}

/// Audita um `RuleSet` num dado instante (ms desde a época), de forma pura.
#[must_use]
pub fn audit(rules: &RuleSet, now_millis: u64) -> AuditReport {
    let mut report = AuditReport::default();
    let mut seen: BTreeSet<RuleId> = BTreeSet::new();
    for rule in &rules.rules {
        if !seen.insert(rule.id.clone()) {
            report.issues.push(AuditIssue::DuplicateRuleId {
                id: rule.id.clone(),
            });
        }
        if rule.statement.trim().is_empty() {
            report.issues.push(AuditIssue::EmptyStatement {
                id: rule.id.clone(),
            });
        }
        let is_enforced = rule.category == RuleCategory::Enforced;
        if is_enforced && rule.examples.negative.is_empty() {
            report
                .issues
                .push(AuditIssue::EnforcedWithoutNegativeExample {
                    id: rule.id.clone(),
                });
        }
        if is_enforced && matches!(rule.enforcement, Enforcement::Advisory) {
            report
                .issues
                .push(AuditIssue::AdvisoryEnforcementMarkedEnforced {
                    id: rule.id.clone(),
                });
        }
        let summary = RuleSummary {
            id: rule.id.clone(),
            category: rule.category,
            activity: if rule.is_active(now_millis) {
                Activity::Active
            } else {
                Activity::Inactive
            },
            examples: if rule.examples.negative.is_empty() {
                ExampleCoverage::MissingNegative
            } else {
                ExampleCoverage::HasNegative
            },
        };
        if is_enforced {
            report.enforced.push(summary);
        } else {
            report.advisory.push(summary);
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::{AuditIssue, audit};
    use crate::facts::ToolName;
    use crate::rule::{
        Enforcement, Rule, RuleCategory, RuleExamples, RuleId, RuleScope, RuleSet, Severity,
    };

    fn rule(id: &str, category: RuleCategory, enforcement: Enforcement) -> Rule {
        Rule {
            id: RuleId::from(id),
            statement: format!("regra {id}"),
            scope: RuleScope::Command {
                tool: ToolName::Exec,
            },
            enforcement,
            severity: Severity::Critical,
            category,
            expires_at: None,
            waiver: None,
            examples: RuleExamples::default(),
        }
    }

    fn deny_exec() -> Enforcement {
        Enforcement::DenyCommand {
            tool: ToolName::Exec,
        }
    }

    #[test]
    fn enforced_without_negative_example_is_flagged() {
        let rules = RuleSet {
            vocab: 3,
            rules: vec![rule("r1", RuleCategory::Enforced, deny_exec())],
        };
        let report = audit(&rules, 0);
        assert_eq!(report.enforced.len(), 1);
        assert!(report.advisory.is_empty());
        assert!(matches!(
            report.issues.as_slice(),
            [AuditIssue::EnforcedWithoutNegativeExample { .. }]
        ));
    }

    #[test]
    fn enforced_with_negative_example_is_clean() {
        let mut rule = rule("r1", RuleCategory::Enforced, deny_exec());
        rule.examples.negative = vec!["exec bash".to_string()];
        let rules = RuleSet {
            vocab: 3,
            rules: vec![rule],
        };
        let report = audit(&rules, 0);
        assert!(report.is_clean());
    }

    #[test]
    fn duplicate_and_empty_statement_are_flagged() {
        let mut first = rule("dup", RuleCategory::Advisory, Enforcement::Advisory);
        first.statement = String::new();
        let second = rule("dup", RuleCategory::Advisory, Enforcement::Advisory);
        let rules = RuleSet {
            vocab: 3,
            rules: vec![first, second],
        };
        let report = audit(&rules, 0);
        assert!(
            report
                .issues
                .iter()
                .any(|issue| matches!(issue, AuditIssue::DuplicateRuleId { .. }))
        );
        assert!(
            report
                .issues
                .iter()
                .any(|issue| matches!(issue, AuditIssue::EmptyStatement { .. }))
        );
    }

    #[test]
    fn advisory_enforcement_in_enforced_category_is_flagged() {
        let rules = RuleSet {
            vocab: 3,
            rules: vec![rule("r1", RuleCategory::Enforced, Enforcement::Advisory)],
        };
        let report = audit(&rules, 0);
        assert!(
            report
                .issues
                .iter()
                .any(|issue| matches!(issue, AuditIssue::AdvisoryEnforcementMarkedEnforced { .. }))
        );
    }
}
