//! Veredicto tipado e auditável (E02-T02). `Evidence` é sempre estruturada, nunca prosa.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::rule::RuleId;

/// Motivo legível de uma decisão.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Reason(String);

impl Reason {
    /// Constrói um motivo.
    pub fn new(reason: impl Into<String>) -> Self {
        Self(reason.into())
    }

    /// Texto do motivo.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Identificador do controlo em falta (ex.: `budget`, `approval`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ControlId(String);

impl ControlId {
    /// Constrói um identificador de controlo.
    pub fn new(control: impl Into<String>) -> Self {
        Self(control.into())
    }

    /// Nome do controlo.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Evidência estruturada de uma negação (§29): `file:line`, facto, argumento, `rule_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    /// Localização `ficheiro:linha`, quando aplicável.
    pub file_line: Option<String>,
    /// Facto que disparou a regra.
    pub fact: String,
    /// Argumento concreto (caminho, tool, fase).
    pub argument: String,
    /// Regra que decidiu.
    pub rule_id: RuleId,
    /// O que **passaria** (Q-08), copiado da regra: remédio acionável que o modelo recebe junto da
    /// negação. Ausente quando a regra não o declara (o `policy:audit` exige-o às `Enforced`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remedy: Option<String>,
}

impl Evidence {
    /// Constrói uma evidência sem localização (`file:line`).
    #[must_use]
    pub fn new(fact: impl Into<String>, argument: impl Into<String>, rule_id: RuleId) -> Self {
        Self {
            file_line: None,
            fact: fact.into(),
            argument: argument.into(),
            rule_id,
            remedy: None,
        }
    }

    /// Acrescenta o remédio da regra (Q-08).
    ///
    /// Sem span: `katu-policy` é instrumentada pelo **chamador** (firewall LLM-free, S-03).
    #[must_use]
    pub fn with_remedy(mut self, remedy: Option<String>) -> Self {
        self.remedy = remedy;
        self
    }
}

/// Pedido de aprovação humana.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRequest {
    /// Regra que exige aprovação.
    pub rule_id: RuleId,
    /// Motivo.
    pub reason: Reason,
    /// Âmbito do pedido.
    pub scope: String,
}

/// Veredicto tipado e auditável.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "outcome")]
#[non_exhaustive]
pub enum Decision {
    /// Permitido.
    Allow,
    /// Negado.
    Deny {
        /// Motivo.
        reason: Reason,
        /// Regra que negou.
        rule_id: RuleId,
        /// Evidência estruturada.
        evidence: Evidence,
    },
    /// Exige aprovação humana.
    RequireApproval {
        /// Pedido de aprovação.
        request: ApprovalRequest,
    },
    /// Faltam controlos: precisa de humano.
    NeedsHuman {
        /// Motivo.
        reason: Reason,
        /// Controlo em falta.
        missing_control: ControlId,
    },
}

impl Decision {
    /// `true` se a ação é permitida.
    #[must_use]
    pub const fn is_allow(&self) -> bool {
        matches!(self, Self::Allow)
    }

    /// Prioridade do veredicto (maior vence): `Allow < RequireApproval < Deny < NeedsHuman`.
    #[must_use]
    pub const fn rank(&self) -> u8 {
        match self {
            Self::Allow => 1,
            Self::RequireApproval { .. } => 2,
            Self::Deny { .. } => 3,
            Self::NeedsHuman { .. } => 4,
        }
    }
}
