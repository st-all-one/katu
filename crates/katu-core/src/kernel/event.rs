//! Eventos tipados do kernel (E04-T01). O log é uma sequência destes eventos.

use serde::{Deserialize, Serialize};

use crate::error::ToolOutcome;
use crate::feedback::CommandRecord;
use crate::plan::Plan;
use crate::verify::VerificationReport;
use katu_policy::{Capability, Phase, ResolvedPath, RuleId, ToolUse};

use super::control::Control;

/// Identificador de um pedido de tool (correlaciona `ToolCall` ↔ `ToolResult`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CallId(String);

impl CallId {
    /// Constrói um identificador de chamada.
    pub fn new(id: impl Into<String>) -> Self {
        let _span = crate::trace_fn!("kernel::event::new");

        Self(id.into())
    }

    /// Identificador textual.
    #[must_use]
    pub fn as_str(&self) -> &str {
        let _span = crate::trace_fn!("kernel::event::as_str");

        &self.0
    }
}

/// Evento append-only de uma sessão (E04-T01).
///
/// A ordem no log é a ordem causal; a projeção para o modelo ignora os eventos de controlo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Event {
    /// Início de um turno.
    TurnStart {
        /// Número do turno.
        turn: u32,
    },
    /// Mensagem do utilizador.
    UserMessage {
        /// Texto.
        text: String,
    },
    /// Mensagem do assistente.
    AssistantMessage {
        /// Texto.
        text: String,
    },
    /// Pedido de tool do modelo (logado **antes** de executar).
    ToolCall {
        /// Identificador da chamada.
        call: CallId,
        /// Uso de tool resolvido (avaliado pela política).
        tool: ToolUse,
    },
    /// Resultado de uma tool.
    ToolResult {
        /// Identificador da chamada.
        call: CallId,
        /// Efeito da operação.
        outcome: ToolOutcome,
    },
    /// Transição de fase do caminho único.
    ///
    /// `outcome` é a evidência de fecho (§51.2): obrigatória quando `to == Closed`, ignorada nas
    /// restantes fases. A evidência fica no log (é o próprio evento).
    PhaseTransition {
        /// Fase destino.
        to: Phase,
        /// Evidência de fecho, exigida pela pré-condição de `Closed` (E05-T04).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        outcome: Option<String>,
    },
    /// `Waiver` explícito: dispensa a pré-condição da fase indicada (§47).
    Waiver {
        /// Fase cuja pré-condição é dispensada.
        transition: Phase,
        /// Motivo legível (campo tipado, nunca interpolação).
        reason: String,
    },
    /// Plano registado/atualizado (E06-T06). Evento de controlo (não vai ao modelo).
    PlanRecorded {
        /// Plano validado.
        plan: Plan,
    },
    /// Feedback de um comando executado (E06-T07). Evento de controlo.
    CommandRecorded {
        /// Registo redigido + truncado.
        record: CommandRecord,
    },
    /// Raiz do workspace definida (E07-T05). Evento de controlo.
    WorkspaceSet {
        /// Raiz canonicalizada.
        root: ResolvedPath,
    },
    /// Aprovação **humana** explícita (E07-T05, §33): concede a capacidade mínima que destranca a
    /// regra. Evento de controlo, append-only e auditável; o agente **não** o assina.
    ApprovalGranted {
        /// Regra que exigiu a aprovação.
        rule_id: RuleId,
        /// Capacidade concedida (mínima, derivada da regra).
        capability: Capability,
        /// Justificação (`override_reason`).
        reason: String,
        /// Quem assinou (`granted_by`).
        granted_by: String,
    },
    /// Relatório do gate de verificação registado (E09-T03). Evento de controlo.
    VerificationRecorded {
        /// Relatório determinístico.
        report: VerificationReport,
    },
    /// Controlo do utilizador em runtime (E12-T10): modelo/pensamento. Evento de controlo.
    Control {
        /// Controlo pedido (já validado pela borda contra o catálogo).
        control: Control,
    },
    /// Fim de um turno.
    TurnEnd {
        /// Número do turno.
        turn: u32,
    },
}

impl Event {
    /// Rótulo estável do variante, para campos de diagnóstico (nunca muda).
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::TurnStart { .. } => "turn_start",
            Self::UserMessage { .. } => "user_message",
            Self::AssistantMessage { .. } => "assistant_message",
            Self::ToolCall { .. } => "tool_call",
            Self::ToolResult { .. } => "tool_result",
            Self::PhaseTransition { .. } => "phase_transition",
            Self::Waiver { .. } => "waiver",
            Self::PlanRecorded { .. } => "plan_recorded",
            Self::CommandRecorded { .. } => "command_recorded",
            Self::WorkspaceSet { .. } => "workspace_set",
            Self::ApprovalGranted { .. } => "approval_granted",
            Self::VerificationRecorded { .. } => "verification_recorded",
            Self::Control { .. } => "control",
            Self::TurnEnd { .. } => "turn_end",
        }
    }
}
