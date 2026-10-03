//! Projeções **puras** do log (E04-T03): histórico do modelo, estado e snapshot.
//!
//! Invariante central: `Model-visible ⟺ logged`. [`derive_messages`] devolve exatamente o que chega
//! ao modelo; os eventos de controlo (`TurnStart`, `TurnEnd`, `PhaseTransition`) **não** entram.

use serde::{Deserialize, Serialize};

use super::event::{CallId, Event};
use super::state::{Refusal, State};
use super::step::step;
use crate::diag::{Level, events};
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
        /// Delta model-visible (§18/G6): o **mesmo** texto que o provider recebe, vindo do log.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        delta: Option<String>,
        /// Nome da tool que falhou (B-03): torna o erro **auto-contido** — o modelo não precisa
        /// de correlacionar com o `ToolCall` para saber qual tool foi.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_name: Option<ToolName>,
    },
}

/// Projeta o histórico visível ao modelo a partir dos eventos (ordem do log).
///
/// A projeção é **normalizada** (L-Q5): um `ToolCall` sem `ToolResult` (ou o inverso) não chega ao
/// modelo e a lista começa sempre em [`Message::User`]. Num log saudável nada muda byte-a-byte;
/// a normalização é a rede de segurança para logs antigos ou truncados.
#[must_use]
pub fn derive_messages(events: &[Event]) -> Vec<Message> {
    let _span = crate::trace_fn!("kernel::project::derive_messages");

    let mut tool_names: std::collections::BTreeMap<CallId, ToolName> =
        std::collections::BTreeMap::new();
    let messages: Vec<Message> = events
        .iter()
        .filter_map(|event| project_event(event, &mut tool_names))
        .collect();
    normalize(messages)
}

/// Remove pares desalinhados e garante que a lista começa em `User` (L-Q5).
///
/// `Model-visible ⟺ logged`: **não** inventa nem reescreve mensagens, só descarta o que o
/// protocolo do endpoint recusaria (uma tool call sem resultado e vice-versa).
fn normalize(messages: Vec<Message>) -> Vec<Message> {
    let _span = crate::fn_span!(
        Level::Trace,
        events::MODEL_PROJECT,
        "kernel::project::normalize"
    );
    let calls: std::collections::BTreeSet<CallId> = messages
        .iter()
        .filter_map(|message| match message {
            Message::ToolCall { call, .. } => Some(call.clone()),
            _ => None,
        })
        .collect();
    let results: std::collections::BTreeSet<CallId> = messages
        .iter()
        .filter_map(|message| match message {
            Message::ToolResult { call, .. } => Some(call.clone()),
            _ => None,
        })
        .collect();
    messages
        .into_iter()
        .filter(|message| match message {
            Message::ToolCall { call, .. } => results.contains(call),
            Message::ToolResult { call, .. } => calls.contains(call),
            _ => true,
        })
        .skip_while(|message| !matches!(message, Message::User { .. }))
        .collect()
}

/// Projeta um evento, se for visível ao modelo.
fn project_event(
    event: &Event,
    tool_names: &mut std::collections::BTreeMap<CallId, ToolName>,
) -> Option<Message> {
    let _span = crate::fn_span!(
        Level::Trace,
        events::MODEL_PROJECT,
        "kernel::project::project_event"
    );
    match event {
        Event::UserMessage { text } => Some(Message::User { text: text.clone() }),
        Event::AssistantMessage { text } => Some(Message::Assistant { text: text.clone() }),
        Event::ToolCall { call, tool } => {
            tool_names.insert(call.clone(), tool.name);
            Some(Message::ToolCall {
                call: call.clone(),
                tool: tool.clone(),
            })
        }
        Event::ToolResult {
            call,
            outcome,
            delta,
        } => Some(Message::ToolResult {
            call: call.clone(),
            outcome: outcome.clone(),
            delta: delta.clone(),
            tool_name: tool_names.get(call).copied(),
        }),
        _ => None,
    }
}

/// Reproduz o estado a partir dos eventos (a fonte da verdade).
///
/// # Errors
/// [`Refusal`] se a sequência contiver uma transição ilegal (log corrompido ou adulterado).
pub fn state_of(events: &[Event]) -> Result<State, Refusal> {
    let _span = crate::fn_span!(
        Level::Trace,
        events::LOG_REPLAY,
        "kernel::project::state_of"
    );
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
    let _span = crate::trace_fn!("kernel::project::snapshot");

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
    use crate::error::ToolOutcome;
    use crate::kernel::event::Event;
    use crate::kernel::{CallId, State, step};
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

    #[test]
    fn orphan_tool_call_is_dropped_from_the_projection() -> Result<(), Box<dyn std::error::Error>> {
        let path = katu_policy::ResolvedPath::from_canonical("/work/src/main.rs")?;
        let events = vec![
            Event::UserMessage { text: "lê".into() },
            Event::ToolCall {
                call: CallId::new("c1"),
                tool: katu_policy::ToolUse {
                    name: katu_policy::ToolName::Read,
                    args: katu_policy::ToolArgs::Read { path: path.clone() },
                    resolved_paths: vec![path.clone()],
                    argv: None,
                    cwd: path,
                },
            },
        ];
        let messages = derive_messages(&events);
        assert_eq!(
            messages.len(),
            1,
            "a call sem resultado não chega ao modelo"
        );
        assert!(matches!(messages.first(), Some(Message::User { .. })));
        Ok(())
    }

    #[test]
    fn orphan_tool_result_is_dropped_from_the_projection() {
        let events = vec![
            Event::UserMessage { text: "lê".into() },
            Event::ToolResult {
                call: CallId::new("c1"),
                outcome: ToolOutcome::Ok,
                delta: Some("conteúdo".into()),
            },
        ];
        let messages = derive_messages(&events);
        assert_eq!(
            messages.len(),
            1,
            "o resultado sem pedido não chega ao modelo"
        );
        assert!(matches!(messages.first(), Some(Message::User { .. })));
    }

    #[test]
    fn the_projection_starts_at_the_first_user_message() {
        let events = vec![
            Event::AssistantMessage {
                text: "pré-rolo".into(),
            },
            Event::UserMessage {
                text: "olá".into()
            },
            Event::AssistantMessage { text: "oi".into() },
        ];
        let messages = derive_messages(&events);
        assert_eq!(messages.len(), 2);
        assert!(matches!(messages.first(), Some(Message::User { .. })));
        assert!(matches!(messages.get(1), Some(Message::Assistant { .. })));
    }

    #[test]
    fn tool_result_carries_the_tool_name_from_the_call() -> Result<(), Box<dyn std::error::Error>> {
        let path = katu_policy::ResolvedPath::from_canonical("/work/src/main.rs")?;
        let events = vec![
            Event::UserMessage { text: "lê".into() },
            Event::ToolCall {
                call: CallId::new("c1"),
                tool: katu_policy::ToolUse {
                    name: katu_policy::ToolName::Read,
                    args: katu_policy::ToolArgs::Read { path: path.clone() },
                    resolved_paths: vec![path.clone()],
                    argv: None,
                    cwd: path,
                },
            },
            Event::ToolResult {
                call: CallId::new("c1"),
                outcome: ToolOutcome::Ok,
                delta: Some("conteúdo".into()),
            },
        ];
        let messages = derive_messages(&events);
        let Some(Message::ToolResult { tool_name, .. }) = messages.get(2) else {
            return Err("esperado um ToolResult".into());
        };
        assert_eq!(*tool_name, Some(katu_policy::ToolName::Read));
        Ok(())
    }

    #[test]
    fn a_blind_retry_gets_a_fix_that_points_elsewhere() -> Result<(), Box<dyn std::error::Error>> {
        // Cenário canónico: o modelo lê fora da raiz, é negado, e a negação ensina a ler sob a
        // raiz — uma tentativa cega (repetir o mesmo caminho) continuaria a ser negada.
        let rule = katu_policy::RuleId::from("contain-read-outside-workspace");
        let evidence = katu_policy::Evidence::new("fora da raiz", "/etc/passwd", rule)
            .with_remedy(Some("leia só sob a raiz do workspace".to_string()));
        let denied = ToolOutcome::Denied {
            rule_id: katu_policy::RuleId::from("contain-read-outside-workspace"),
            evidence,
        };
        let summary = denied.summary();
        assert!(
            summary.contains("fix: leia só sob a raiz do workspace"),
            "{summary}"
        );
        // O remédio aponta para uma ação **diferente** da que foi negada (ler sob a raiz, não fora).
        let fix = denied.fix().ok_or("esperado um remédio")?;
        assert!(
            !fix.contains("/etc/passwd"),
            "o remédio não deve repetir o caminho negado"
        );
        Ok(())
    }
}
