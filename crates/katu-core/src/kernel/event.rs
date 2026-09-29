//! Eventos tipados do kernel (E04-T01). O log é uma sequência destes eventos.

use serde::{Deserialize, Serialize};

use crate::error::ToolOutcome;
use katu_policy::{Phase, ToolUse};

/// Identificador de um pedido de tool (correlaciona `ToolCall` ↔ `ToolResult`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CallId(String);

impl CallId {
    /// Constrói um identificador de chamada.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Identificador textual.
    #[must_use]
    pub fn as_str(&self) -> &str {
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
    PhaseTransition {
        /// Fase destino.
        to: Phase,
    },
    /// Fim de um turno.
    TurnEnd {
        /// Número do turno.
        turn: u32,
    },
}
