//! Compactação determinística do histórico antigo (E09-T07).
//!
//! **Controlo do core, nunca inline no hot path.** O prefixo que não cabe no orçamento é
//! substituído por um **digest determinístico** (sem LLM): para o mesmo input, o mesmo resumo e o
//! mesmo mapeamento original→substituto — preservando o cache de prefixo do provider (§1). O
//! original **não** se perde: continua endereçável no log por [`recover`].

use serde::{Deserialize, Serialize};

use super::{Context, ContextBudget, fit_raw, message_weight, prime, tokens_from_bytes};
use crate::diag::{Level, events};
use crate::evidence::{EvidenceBasis, Metric, Unit, to_f64};
use crate::kernel::{Event, Message, derive_messages};
use crate::report::content_id;

/// Versão do esquema da compactação.
pub const COMPACTION_SCHEMA_VERSION: u32 = 1;

/// Bytes de excerto por linha do digest.
const DIGEST_EXCERPT_BYTES: usize = 48;

/// Modo de compactação (default **desligado**: nada silencioso no caminho built-in).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompactionMode {
    /// Desligada (comportamento original de `assemble`).
    #[default]
    Disabled,
    /// Ligada (o prefixo antigo é substituído por um digest).
    Enabled,
}

/// Substituição determinística de uma mensagem original.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Replacement {
    /// Id de conteúdo da mensagem original (endereçável no log).
    pub original: String,
    /// Id de conteúdo do substituto (a linha do digest).
    pub substitute: String,
}

/// Compactação determinística do histórico antigo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Compaction {
    /// Versão do esquema.
    pub schema_version: u32,
    /// Contexto resultante (summary preenchido, sufixo cru mantido).
    pub context: Context,
    /// Mapeamento original→substituto (determinístico).
    pub replacements: Vec<Replacement>,
    /// Tokens do histórico original (antes da compactação).
    pub original_tokens: usize,
    /// Ganho em tokens (base `inferred`).
    pub gain: Metric,
}

/// Compacta o histórico antigo (determinístico; **nunca** inline no hot path).
///
/// Devolve `None` quando a compactação está desligada. O original continua endereçável no log
/// ([`recover`]).
#[must_use]
pub fn compact(
    events: &[Event],
    budget: ContextBudget,
    mode: CompactionMode,
) -> Option<Compaction> {
    if mode == CompactionMode::Disabled {
        return None;
    }
    let _span = crate::span!(
        Level::Debug,
        events::CONTEXT_COMPACT,
        "raw_min" => budget.raw_min,
        "summary_max" => budget.summary_max,
    );
    let all = derive_messages(events);
    let kept = fit_raw(&all, budget.raw_min);
    let prefix_len = all.len().saturating_sub(kept.len());
    let prefix = all.get(..prefix_len).unwrap_or_default();
    let mut replacements = Vec::with_capacity(prefix.len());
    let mut lines = Vec::with_capacity(prefix.len());
    for message in prefix {
        let original = message_id(message);
        let line = digest_line(message, &original);
        replacements.push(Replacement {
            substitute: content_id("c", line.as_bytes()),
            original,
        });
        lines.push(line);
    }
    let summary = fit_summary(&lines, budget.summary_max);
    let kept_tokens = kept.iter().map(message_weight).sum();
    let summary_tokens = tokens_from_bytes(summary.len());
    let compacted_tokens = summary_tokens.saturating_add(kept_tokens);
    let original_tokens = prefix
        .iter()
        .map(message_weight)
        .fold(kept_tokens, usize::saturating_add);
    let saved = original_tokens.saturating_sub(compacted_tokens);
    let gain = Metric::new(
        "context.gain_tokens",
        to_f64(u64::try_from(saved).unwrap_or(u64::MAX)),
        Unit::Tokens,
        EvidenceBasis::Inferred,
        None,
    )
    .ok()?;
    Some(Compaction {
        schema_version: COMPACTION_SCHEMA_VERSION,
        context: Context {
            prime: prime(),
            summary: Some(summary),
            messages: kept,
            raw_tokens: kept_tokens,
            tokens: tokens_from_bytes(prime().len()).saturating_add(compacted_tokens),
        },
        replacements,
        original_tokens,
        gain,
    })
}

/// Recupera a mensagem original pelo seu id de conteúdo (o log é a fonte).
#[must_use]
pub fn recover(events: &[Event], id: &str) -> Option<Message> {
    derive_messages(events)
        .into_iter()
        .find(|message| message_id(message) == id)
}

/// Id de conteúdo de uma mensagem (endereçável no log).
#[must_use]
pub fn message_id(message: &Message) -> String {
    content_id("m", &serde_json::to_vec(message).unwrap_or_default())
}

/// Junta as linhas do digest até ao teto de tokens.
fn fit_summary(lines: &[String], summary_max: usize) -> String {
    let mut out = String::new();
    let mut used = 0usize;
    for line in lines {
        let cost = tokens_from_bytes(line.len().saturating_add(1));
        if used.saturating_add(cost) > summary_max {
            break;
        }
        out.push_str(line);
        out.push('\n');
        used = used.saturating_add(cost);
    }
    out
}

/// Uma linha determinística do digest para uma mensagem.
fn digest_line(message: &Message, id: &str) -> String {
    let kind = message_kind(message);
    let excerpt = excerpt(message);
    format!("{kind} {id} {excerpt}")
}

/// Tipo estável de uma mensagem.
fn message_kind(message: &Message) -> &'static str {
    match message {
        Message::User { .. } => "user",
        Message::Assistant { .. } => "assistant",
        Message::ToolCall { .. } => "tool_call",
        Message::ToolResult { .. } => "tool_result",
    }
}

/// Excerto curto e determinístico (primeiros `DIGEST_EXCERPT_BYTES`).
fn excerpt(message: &Message) -> String {
    let text = match message {
        Message::User { text } | Message::Assistant { text } => text.clone(),
        Message::ToolCall { tool, .. } => {
            let name = tool.name;
            format!("{name:?}")
        }
        Message::ToolResult { outcome, .. } => format!("{outcome:?}"),
    };
    truncate(&text, DIGEST_EXCERPT_BYTES)
}

/// Corta em fronteira de caractere e marca a elipse.
fn truncate(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    let mut out = text.get(..end).unwrap_or_default().to_string();
    out.push('…');
    out
}
