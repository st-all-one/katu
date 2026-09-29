//! Estado do kernel como **valor** (E04-T01) e a forma do caminho único (DF1).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::event::CallId;
use crate::error::ToolOutcome;
use crate::plan::Plan;
use katu_policy::{BudgetState, Capability, Phase, ToolName, ToolUse};

/// Estado de um pedido de tool (pendente ou concluído).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
#[non_exhaustive]
pub enum CallStatus {
    /// Pedido aceite pela política, à espera de efeito.
    Pending {
        /// Uso de tool.
        tool: ToolUse,
    },
    /// Efeito observado.
    Done {
        /// Efeito.
        outcome: ToolOutcome,
    },
}

/// Estado do agente. `Clone`/`Eq`/`Serialize`; mapas ordenados (`BTreeMap`), nunca `HashMap`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    /// Fase corrente do caminho único.
    pub phase: Phase,
    /// Número do turno corrente.
    pub turn: u32,
    /// `true` se há um turno aberto.
    pub turn_open: bool,
    /// Chamadas de tool indexadas por identificador.
    pub calls: BTreeMap<CallId, CallStatus>,
    /// Tools efetivamente concluídas com sucesso (para `RequireAfter` da política).
    pub completed_tools: BTreeSet<ToolName>,
    /// Fases com pré-condição dispensada por um `waiver` explícito (E05-T02, §47).
    pub waivers: BTreeSet<Phase>,
    /// Capacidades concedidas no contexto corrente (DF2/DF4; a política é falha-fechado sem elas).
    pub capabilities: Vec<Capability>,
    /// Consumo de orçamento dentro da tarefa (alimenta `Facts::budget`).
    pub budget: BudgetState,
    /// Plano registado (E06-T06); exigido para transitar para [`Phase::Planned`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<Plan>,
}

impl State {
    /// Estado inicial: caminho único em [`Phase::Task`], sem turno aberto.
    #[must_use]
    pub fn initial() -> Self {
        Self {
            phase: Phase::Task,
            turn: 0,
            turn_open: false,
            calls: BTreeMap::new(),
            completed_tools: BTreeSet::new(),
            waivers: BTreeSet::new(),
            capabilities: Vec::new(),
            budget: BudgetState::default(),
            plan: None,
        }
    }
}

/// Motivo de uma recusa determinística (fail-closed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "reason")]
#[non_exhaustive]
pub enum RefusalReason {
    /// Já há um turno aberto.
    TurnAlreadyOpen,
    /// Não há turno aberto.
    NoOpenTurn,
    /// Chamada com identificador repetido.
    DuplicateCall {
        /// Identificador repetido.
        call: CallId,
    },
    /// Resultado para uma chamada desconhecida ou já fechada.
    UnknownCall {
        /// Identificador desconhecido.
        call: CallId,
    },
    /// Turno diferente do aberto.
    TurnMismatch {
        /// Turno aberto.
        expected: u32,
        /// Turno recebido.
        got: u32,
    },
    /// Transição de fase fora da forma do caminho único.
    IllegalTransition {
        /// Fase de origem.
        from: Phase,
        /// Fase destino.
        to: Phase,
    },
    /// Pré-condição da fase destino não satisfeita (E05-T02/T04, fail-closed).
    UnmetPrecondition {
        /// Fase destino cuja pré-condição falhou.
        to: Phase,
    },
}

/// Recusa de uma transição: o estado **não** muda (§42).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("recusa em {phase:?}: {reason:?}")]
pub struct Refusal {
    /// Motivo.
    pub reason: RefusalReason,
    /// Fase em que ocorreu.
    pub phase: Phase,
}

/// Fase seguinte do caminho único, se existir.
#[must_use]
pub const fn next_phase(phase: Phase) -> Option<Phase> {
    match phase {
        Phase::Task => Some(Phase::KnowledgeConsulted),
        Phase::KnowledgeConsulted => Some(Phase::Planned),
        Phase::Planned => Some(Phase::Implemented),
        Phase::Implemented => Some(Phase::Verified),
        Phase::Verified => Some(Phase::Persisted),
        Phase::Persisted => Some(Phase::Closed),
        _ => None,
    }
}

/// `true` se a transição `from → to` respeita a forma do caminho único.
///
/// É permitida a transição de **qualquer** fase de volta a [`Phase::Task`] (recusa/replan, §51.2);
/// o avanço é estritamente de um passo.
#[must_use]
pub fn can_transition(from: Phase, to: Phase) -> bool {
    if from == to {
        return false;
    }
    if to == Phase::Task {
        return true;
    }
    next_phase(from) == Some(to)
}

#[cfg(test)]
mod tests {
    use super::{State, can_transition, next_phase};
    use katu_policy::Phase;

    #[test]
    fn forward_transitions_are_single_step() {
        assert!(can_transition(Phase::Task, Phase::KnowledgeConsulted));
        assert!(!can_transition(Phase::Task, Phase::Planned));
        assert!(can_transition(Phase::Persisted, Phase::Closed));
        assert_eq!(next_phase(Phase::Closed), None);
    }

    #[test]
    fn replan_returns_to_task() {
        assert!(can_transition(Phase::Implemented, Phase::Task));
        assert!(!can_transition(Phase::Task, Phase::Task));
    }

    #[test]
    fn initial_state_is_task_and_closed_turn() {
        let state = State::initial();
        assert_eq!(state.phase, Phase::Task);
        assert!(!state.turn_open);
        assert!(state.calls.is_empty());
    }
}
