//! Projeções **puras** do log (E04-T03): histórico do modelo, estado e snapshot.
//!
//! Invariante central: `Model-visible ⟺ logged`. [`derive_messages`] devolve exatamente o que chega
//! ao modelo; os eventos de controlo (`TurnStart`, `TurnEnd`, `PhaseTransition`) **não** entram.

use serde::{Deserialize, Serialize};

use super::event::{CallId, Event, Visibility};
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
        /// Visibilidade (G3): o modelo vê todas; a transcrição só as `User`.
        #[serde(default)]
        visibility: Visibility,
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

/// Normaliza a projeção (L-Q5/G6): pareamento `ToolCall`↔`ToolResult`, mensagens vazias,
/// adjacência (resultado depois do pedido) e início em `User`.
///
/// `Model-visible ⟺ logged`: **não** inventa nem reescreve mensagens, só descarta o que o
/// protocolo do endpoint recusaria (um par desalinhado, uma mensagem sem conteúdo).
fn normalize(messages: Vec<Message>) -> Vec<Message> {
    let _span = crate::fn_span!(
        Level::Trace,
        events::MODEL_PROJECT,
        "kernel::project::normalize"
    );
    // 1. Pareamento: uma call sem resultado (e vice-versa) não chega ao modelo.
    let paired = keep_pairs(messages);
    // 2. Vazios: uma mensagem de texto sem conteúdo é ruído para o endpoint.
    // 3. Adjacência: um resultado tem de vir **depois** do seu pedido.
    let mut seen: std::collections::BTreeSet<CallId> = std::collections::BTreeSet::new();
    let ordered: Vec<Message> = paired
        .into_iter()
        .filter(|message| match message {
            Message::User { text, .. } | Message::Assistant { text } => !text.trim().is_empty(),
            _ => true,
        })
        .filter(|message| match message {
            Message::ToolCall { call, .. } => {
                seen.insert(call.clone());
                true
            }
            Message::ToolResult { call, .. } => seen.contains(call),
            _ => true,
        })
        .collect();
    // 4. Um resultado fora de ordem deixa o pedido órfão: re-pareia antes do lead.
    // 5. Lead: a lista começa sempre numa mensagem do utilizador.
    keep_pairs(ordered)
        .into_iter()
        .skip_while(|message| !matches!(message, Message::User { .. }))
        .collect()
}

/// Mantém só os pares `ToolCall`↔`ToolResult` completos (descarta órfãos).
fn keep_pairs(messages: Vec<Message>) -> Vec<Message> {
    let _span = crate::trace_fn!("kernel::project::keep_pairs");

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
        Event::UserMessage { text, visibility } => Some(Message::User {
            text: text.clone(),
            visibility: *visibility,
        }),
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
mod tests;
