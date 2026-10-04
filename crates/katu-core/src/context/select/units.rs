//! Unidades de contexto: a partição do histórico e a extração do texto **model-visible**.
//!
//! Uma unidade é uma mensagem isolada ou uma **corrida maximal** de mensagens de tool — a mesma
//! fronteira do corte (Q-02a), porque o wire exige que um `role: "tool"` responda ao `tool_calls`
//! precedente. O texto sai como [`Cow`] (nada se clona só para tokenizar) e os bytes contados são os
//! do texto que o modelo vê (o *delta* de um resultado de tool, não o `ToolOutcome`).

use std::borrow::Cow;
use std::collections::BTreeSet;

use crate::error::ToolOutcome;
use crate::kernel::Message;

use super::{Candidate, Unit, terms};
use crate::context::message_weight;
use crate::diag::{Level, events};

/// Texto **model-visible** de uma mensagem, emprestado quando possível (uma só extração).
///
/// O empréstimo importa: um resultado de tool pode trazer um *delta* de vários KiB e a montagem do
/// contexto corre a cada turno — clonar o texto só para o tokenizar seria o maior custo do caminho.
#[must_use]
pub fn message_text(message: &Message) -> Cow<'_, str> {
    let _span = crate::trace_fn!("context::select::message_text");

    match message {
        Message::User { text, .. } | Message::Assistant { text } => Cow::Borrowed(text),
        Message::ToolCall { tool, .. } => Cow::Owned(tool_text(tool)),
        Message::ToolResult {
            delta: Some(delta), ..
        } => Cow::Borrowed(delta),
        Message::ToolResult {
            outcome,
            delta: None,
            tool_name,
            ..
        } => Cow::Owned(match tool_name {
            Some(name) => format!("{}: {}", name.as_str(), outcome.summary()),
            None => outcome.summary(),
        }),
    }
}

/// Texto de um pedido de tool: nome estável + primeiro caminho resolvido.
fn tool_text(tool: &katu_policy::ToolUse) -> String {
    let _span = crate::trace_fn!("context::select::tool_text");

    let name = tool.name.as_str();
    match tool.resolved_paths.first() {
        Some(path) => format!("{name} {}", path.as_str()),
        None => name.to_string(),
    }
}

/// Partição do histórico em unidades (corridas de tool ou mensagens isoladas).
#[must_use]
pub fn units(messages: &[Message]) -> Vec<Unit> {
    let _span = crate::fn_span!(
        Level::Trace,
        events::CONTEXT_TRIM,
        "context::select::units",
        "messages" => messages.len(),
    );

    let mut out = Vec::with_capacity(messages.len());
    let mut index = 0usize;
    while index < messages.len() {
        let start = index;
        if is_tool(messages.get(index)) {
            while index < messages.len() && is_tool(messages.get(index)) {
                index = index.saturating_add(1);
            }
        } else {
            index = index.saturating_add(1);
        }
        let slice = messages.get(start..index).unwrap_or_default();
        let mut merged = BTreeSet::new();
        let mut bytes = 0usize;
        let mut tokens = 0usize;
        let mut evidence = 0u64;
        for message in slice {
            merged.extend(terms(&message_text(message)));
            bytes = bytes.saturating_add(message_bytes(message));
            tokens = tokens.saturating_add(message_weight(message));
            evidence = evidence.max(evidence_of(message));
        }
        out.push(Unit {
            start,
            end: index,
            candidate: Candidate {
                terms: merged,
                bytes,
                tokens,
                evidence,
            },
        });
    }
    out
}

/// Bytes model-visible de uma mensagem (o mesmo texto que [`message_text`] devolve).
fn message_bytes(message: &Message) -> usize {
    let _span = crate::trace_fn!("context::select::message_bytes");

    match message {
        Message::User { text, .. } | Message::Assistant { text } => text.len(),
        Message::ToolCall { tool, .. } => tool_text(tool).len(),
        Message::ToolResult { outcome, delta, .. } => {
            outcome_weight(outcome).saturating_add(delta.as_deref().map_or(0, str::len))
        }
    }
}

/// Peso do efeito de uma mensagem de tool (evidência/controlo), em bytes.
fn outcome_weight(outcome: &ToolOutcome) -> usize {
    let _span = crate::trace_fn!("context::select::outcome_weight");

    match outcome {
        ToolOutcome::Denied { evidence, .. } => {
            evidence.argument.len().saturating_add(evidence.fact.len())
        }
        ToolOutcome::Unavailable { control, .. } => control.as_str().len(),
        _ => 0,
    }
}

/// Prioridade de evidência de uma mensagem: um resultado de tool traz prova; um pedido, não.
fn evidence_of(message: &Message) -> u64 {
    let _span = crate::trace_fn!("context::select::evidence_of");

    match message {
        Message::ToolResult { .. } => 3,
        Message::User { .. } => 2,
        Message::Assistant { .. } => 1,
        Message::ToolCall { .. } => 0,
    }
}

/// Mensagem de tool (`ToolCall`/`ToolResult`)?
fn is_tool(message: Option<&Message>) -> bool {
    let _span = crate::trace_fn!("context::select::is_tool");

    matches!(
        message,
        Some(Message::ToolCall { .. } | Message::ToolResult { .. })
    )
}
