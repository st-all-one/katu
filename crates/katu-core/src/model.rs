//! Projeções **model-facing** de tipos do núcleo (ADR 0005/0006): outcome, erro e verificação.
//!
//! Cada tipo devolve um [`Value`] que a projeção colunar do TOON emite de forma determinística:
//! escalares no topo viram secção `k` (explícitos, sem defaults) e listas viram tabelas do registo
//! ([`crate::toon::schema`]). Os domínios fechados (`status`, `basis`, …) são a **mesma** fonte que
//! o prime ensina — sem drift entre o que se emite e o que se documenta ao modelo.

use crate::diag::{Level, events};
use crate::error::{Error, ToolOutcome};
use crate::toon::Value;
use crate::verify::VerificationReport;

/// Par `(chave, valor)` de um mapa TOON.
fn field(name: &str, value: Value) -> (String, Value) {
    (name.to_string(), value)
}

impl ToolOutcome {
    /// Estado estável da operação (domínio fechado; vem do registo).
    #[must_use]
    pub const fn status_str(&self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Partial => "partial",
            Self::Denied { .. } => "denied",
            Self::Timeout => "timeout",
            Self::Unavailable { .. } => "unavailable",
        }
    }

    /// Resumo de uma linha (usado no digest da compactação; nunca `Debug`).
    #[must_use]
    pub fn summary(&self) -> String {
        match self {
            Self::Ok | Self::Partial | Self::Timeout => self.status_str().to_string(),
            Self::Denied { rule_id, evidence } => {
                format!("denied {} {}", rule_id.as_str(), evidence.argument)
            }
            Self::Unavailable { control, .. } => {
                format!("unavailable {}", control.as_str())
            }
        }
    }

    /// Projeção TOON do outcome: escalares explícitos, com a regra/argumento quando acionável.
    #[must_use]
    pub fn to_value(&self) -> Value {
        let _span = crate::span!(Level::Debug, events::MODEL_PROJECT, "target" => "outcome");
        let mut entries = vec![field("status", Value::str(self.status_str()))];
        match self {
            Self::Denied { rule_id, evidence } => {
                entries.push(field("rule", Value::str(rule_id.as_str())));
                entries.push(field("arg", Value::str(evidence.argument.clone())));
                entries.push(field("fact", Value::str(evidence.fact.clone())));
                if let Some(loc) = &evidence.file_line {
                    entries.push(field("loc", Value::str(loc.clone())));
                }
            }
            Self::Unavailable { control, rule_id } => {
                entries.push(field("ctrl", Value::str(control.as_str())));
                if let Some(rule_id) = rule_id {
                    entries.push(field("rule", Value::str(rule_id.as_str())));
                }
            }
            Self::Ok | Self::Partial | Self::Timeout => {}
        }
        Value::map(entries)
    }
}

impl Error {
    /// Projeção TOON do erro: `kind` do vocabulário fechado + mensagem de uma linha.
    #[must_use]
    pub fn to_value(&self) -> Value {
        let _span = crate::span!(Level::Debug, events::MODEL_PROJECT, "target" => "error");
        Value::map(vec![
            field("kind", Value::str(self.kind().as_str())),
            field("msg", Value::str(self.to_string())),
        ])
    }
}

impl VerificationReport {
    /// Projeção TOON do gate: resumo escalar + tabela `checks` (ordem determinística).
    #[must_use]
    pub fn to_value(&self) -> Value {
        let _span = crate::span!(Level::Debug, events::MODEL_PROJECT, "target" => "verification");
        let checks = self
            .checks
            .iter()
            .map(|check| {
                Value::map(vec![
                    field("id", Value::str(check.id.clone())),
                    field("status", Value::str(check.status.as_str())),
                    field("detail", Value::str(check.detail.clone())),
                ])
            })
            .collect();
        Value::map(vec![
            field("schema", Value::int(i64::from(self.schema_version))),
            field("status", Value::str(self.status.as_str())),
            field("cov_bps", Value::int(i64::from(self.coverage_bps))),
            field("strict", Value::bool(self.strict)),
            field("checks", Value::list(checks)),
        ])
    }
}

#[cfg(test)]
mod tests;
