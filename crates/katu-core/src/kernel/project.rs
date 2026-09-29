//! Projeções **puras** do log (E04-T03): histórico do modelo, estado e snapshot.
//!
//! Invariante central: `Model-visible ⟺ logged`. [`derive_messages`] devolve exatamente o que chega
//! ao modelo; os eventos de controlo (`TurnStart`, `TurnEnd`, `PhaseTransition`) **não** entram.

use serde::{Deserialize, Serialize};

use super::event::{CallId, Event};
use super::state::{Refusal, State};
use super::step::step;
use crate::error::ToolOutcome;
use katu_policy::{Phase, ToolName, ToolUse};

/// Mensagem visível ao modelo (projeção do log).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Message {
    /// Mensagem do utilizador.
    User {
        /// Texto.
        text: String,
    },
    /// Mensagem do assistente.
    Assistant {
        /// Texto.
        text: String,
    },
    /// Pedido de tool.
    ToolCall {
        /// Identificador da chamada.
        call: CallId,
        /// Uso de tool.
        tool: ToolUse,
    },
    /// Resultado de tool.
    ToolResult {
        /// Identificador da chamada.
        call: CallId,
        /// Efeito.
        outcome: ToolOutcome,
    },
}

/// Projeta o histórico visível ao modelo a partir dos eventos (ordem do log).
#[must_use]
pub fn derive_messages(events: &[Event]) -> Vec<Message> {
    events.iter().filter_map(project_event).collect()
}

/// Projeta um evento, se for visível ao modelo.
fn project_event(event: &Event) -> Option<Message> {
    match event {
        Event::UserMessage { text } => Some(Message::User { text: text.clone() }),
        Event::AssistantMessage { text } => Some(Message::Assistant { text: text.clone() }),
        Event::ToolCall { call, tool } => Some(Message::ToolCall {
            call: call.clone(),
            tool: tool.clone(),
        }),
        Event::ToolResult { call, outcome } => Some(Message::ToolResult {
            call: call.clone(),
            outcome: outcome.clone(),
        }),
        _ => None,
    }
}

/// Reproduz o estado a partir dos eventos (a fonte da verdade).
///
/// # Errors
/// [`Refusal`] se a sequência contiver uma transição ilegal (log corrompido ou adulterado).
pub fn state_of(events: &[Event]) -> Result<State, Refusal> {
    let mut state = State::initial();
    for event in events {
        state = step(&state, event)?;
    }
    Ok(state)
}

/// Resumo compacto e determinístico de uma sessão.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Fase final.
    pub phase: Phase,
    /// Turno final.
    pub turn: u32,
    /// Se o turno ficou aberto.
    pub turn_open: bool,
    /// Número de mensagens visíveis ao modelo.
    pub messages: usize,
    /// Tools concluídas com sucesso (ordem canônica).
    pub completed_tools: Vec<ToolName>,
}

/// Reconstrói o estado e resume-o num [`Snapshot`].
///
/// # Errors
/// [`Refusal`] se a sequência de eventos for inválida.
pub fn snapshot(events: &[Event]) -> Result<Snapshot, Refusal> {
    let state = state_of(events)?;
    Ok(Snapshot {
        phase: state.phase,
        turn: state.turn,
        turn_open: state.turn_open,
        messages: derive_messages(events).len(),
        completed_tools: state.completed_tools.iter().copied().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::{Message, derive_messages, snapshot, state_of};
    use crate::kernel::event::Event;
    use crate::kernel::{State, step};
    use katu_policy::Phase;

    fn session() -> Vec<Event> {
        vec![
            Event::TurnStart { turn: 1 },
            Event::UserMessage {
                text: "faz isto".into(),
            },
            Event::AssistantMessage { text: "ok".into() },
            Event::Waiver {
                transition: Phase::KnowledgeConsulted,
                reason: "teste de projeção".into(),
            },
            Event::PhaseTransition {
                to: Phase::KnowledgeConsulted,
                outcome: None,
            },
            Event::TurnEnd { turn: 1 },
        ]
    }

    #[test]
    fn messages_exclude_control_events() {
        let messages = derive_messages(&session());
        assert_eq!(messages.len(), 2);
        assert!(matches!(messages.first(), Some(Message::User { .. })));
        assert!(matches!(messages.get(1), Some(Message::Assistant { .. })));
    }

    #[test]
    fn state_of_replays_final_state() -> Result<(), Box<dyn std::error::Error>> {
        let events = session();
        let state = state_of(&events)?;
        assert_eq!(state.phase, Phase::KnowledgeConsulted);
        assert_eq!(state.turn, 1);
        assert!(!state.turn_open);

        let mut manual = State::initial();
        for event in &events {
            manual = step(&manual, event)?;
        }
        assert_eq!(state, manual);
        Ok(())
    }

    #[test]
    fn snapshot_counts_visible_messages() -> Result<(), Box<dyn std::error::Error>> {
        let snap = snapshot(&session())?;
        assert_eq!(snap.messages, 2);
        assert_eq!(snap.phase, Phase::KnowledgeConsulted);
        Ok(())
    }

    #[test]
    fn illegal_sequence_is_refused_and_state_unchanged() {
        let events = vec![Event::PhaseTransition {
            to: Phase::Closed,
            outcome: None,
        }];
        let state = State::initial();
        assert!(state_of(&events).is_err());
        if let Some(event) = events.first() {
            assert!(step(&state, event).is_err());
        }
        assert_eq!(state, State::initial());
    }

    #[test]
    fn model_visible_matches_logged_events() {
        let events = session();
        let visible = events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    Event::UserMessage { .. }
                        | Event::AssistantMessage { .. }
                        | Event::ToolCall { .. }
                        | Event::ToolResult { .. }
                )
            })
            .count();
        assert_eq!(derive_messages(&events).len(), visible);
    }
}
