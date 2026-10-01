//! Observações de confiança extraídas do log (Q-11/F6): *a negação foi honrada?*
//!
//! `katu-policy` tem a estatística ([`katu_policy::Trials`]/[`katu_policy::verdict`]) mas **não**
//! conhece o log (firewall LLM-free). Aqui vive a ponte: uma passagem pura sobre os [`Event`] que
//! conta, por regra, quantas vezes a regra recusou e quantas dessas recusas foram **honradas** — o
//! `ToolResult` da mesma `CallId` não mostra um efeito executado.
//!
//! O ensaio é o invariante duro de `Enforced`: *recusou ⇒ não correu*. Uma violação (a chamada
//! recusada aparece executada sob o **mesmo** `CallId`) é uma contradição medida, não uma opinião. A
//! reaprovação humana (`ApprovalGranted`) conta como ensaio honrado: a regra fez o seu papel de
//! portão, e a re-execução usa um id próprio (`{call}#approved`), pelo que não colide com a recusa.
//!
//! Por tool mede-se outra coisa — o **contrato de conclusão**: ensaio = chamada, sucesso = não
//! expirou (`Timeout`). Não decide política: é diagnóstico.

use std::collections::{BTreeMap, BTreeSet};

use katu_policy::{RuleCategory, RuleId, RuleSet, Threshold, ToolName, Trials, Verdict, verdict};

use super::event::{CallId, Event};
use crate::error::ToolOutcome;

/// `true` se o desfecho mostra um efeito que **correu** (a negação não foi honrada).
fn ran(outcome: &ToolOutcome) -> bool {
    let _span = crate::trace_fn!("kernel::confidence::ran");

    matches!(
        outcome,
        ToolOutcome::Ok | ToolOutcome::Partial | ToolOutcome::Timeout
    )
}

/// Regra que recusou, quando a recusa a nomeia (DF10).
fn refusing_rule(outcome: &ToolOutcome) -> Option<&RuleId> {
    let _span = crate::trace_fn!("kernel::confidence::refusing_rule");

    match outcome {
        ToolOutcome::Denied { rule_id, .. } => Some(rule_id),
        ToolOutcome::Unavailable { rule_id, .. } => rule_id.as_ref(),
        _ => None,
    }
}

/// Ensaios por regra: recusa honrada vs recusa violada.
#[must_use]
pub fn rule_trials(events: &[Event]) -> BTreeMap<RuleId, Trials> {
    let _span = crate::trace_fn!("kernel::confidence::rule_trials");

    let executed = executed_calls(events);
    let mut trials: BTreeMap<RuleId, Trials> = BTreeMap::new();
    for event in events {
        match event {
            Event::ToolResult { call, outcome, .. } => {
                let Some(rule) = refusing_rule(outcome) else {
                    continue;
                };
                let entry = trials.entry(rule.clone()).or_default();
                if executed.contains(call) {
                    entry.observe_violation();
                } else {
                    entry.observe_honored();
                }
            }
            Event::ApprovalGranted { rule_id, .. } => {
                trials.entry(rule_id.clone()).or_default().observe_honored();
            }
            _ => {}
        }
    }
    trials
}

/// Chamadas que correram (qualquer desfecho com efeito).
fn executed_calls(events: &[Event]) -> BTreeSet<&CallId> {
    let _span = crate::trace_fn!("kernel::confidence::executed_calls");

    let mut executed: BTreeSet<&CallId> = BTreeSet::new();
    for event in events {
        if let Event::ToolResult { call, outcome, .. } = event
            && ran(outcome)
        {
            executed.insert(call);
        }
    }
    executed
}

/// Ensaios por tool: chamada concluída vs chamada que expirou.
#[must_use]
pub fn tool_trials(events: &[Event]) -> BTreeMap<ToolName, Trials> {
    let _span = crate::trace_fn!("kernel::confidence::tool_trials");

    let mut names: BTreeMap<&CallId, &ToolName> = BTreeMap::new();
    for event in events {
        if let Event::ToolCall { call, tool } = event {
            names.insert(call, &tool.name);
        }
    }
    let mut trials: BTreeMap<ToolName, Trials> = BTreeMap::new();
    for event in events {
        let Event::ToolResult { call, outcome, .. } = event else {
            continue;
        };
        let Some(name) = names.get(call) else {
            continue;
        };
        let entry = trials.entry(**name).or_default();
        if matches!(outcome, ToolOutcome::Timeout) {
            entry.observe_violation();
        } else {
            entry.observe_honored();
        }
    }
    trials
}

/// Veredictos das regras **declaradas** `Enforced`, pela ordem do `RuleSet` (determinístico).
///
/// A categoria declarada é a referência: o veredicto diz se o log a **sustenta**. Regras
/// `Advisory`/`Perception` não entram (não prometem aplicação).
#[must_use]
pub fn enforced_verdicts(events: &[Event], rules: &RuleSet, threshold: &Threshold) -> Vec<Verdict> {
    let _span = crate::trace_fn!("kernel::confidence::enforced_verdicts");

    let trials = rule_trials(events);
    rules
        .rules
        .iter()
        .filter(|rule| rule.category == RuleCategory::Enforced)
        .map(|rule| {
            let observed = trials.get(&rule.id).copied().unwrap_or_default();
            verdict(&rule.id, observed, threshold)
        })
        .collect()
}

#[cfg(test)]
mod tests;
