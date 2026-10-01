//! Compactação determinística do histórico antigo (E09-T07).
//!
//! **Controlo do core, nunca inline no hot path.** O prefixo que não cabe no orçamento é
//! substituído por um **digest determinístico** (sem LLM): para o mesmo input, o mesmo resumo e o
//! mesmo mapeamento original→substituto — preservando o cache de prefixo do provider (§1). O
//! original **não** se perde: continua endereçável no log por [`recover`].
//!
//! O digest é uma tabela `m` (ADR 0006): `kind`, `id` e um `text` de uma linha por mensagem. Não
//! há `Debug` no caminho — o texto é estruturado (outcome → `ToolOutcome::summary`).

use serde::{Deserialize, Serialize};

use super::{Context, ContextBudget, fit_raw, message_weight, prime, tokens_from_bytes};
use crate::diag::{Level, events};
use crate::evidence::{EvidenceBasis, Metric, Unit, to_f64};
use crate::kernel::{Event, Message, derive_messages};
use crate::report::content_id;
use crate::toon::{Cell, RowTable, Section, emit};
use katu_policy::ToolUse;

/// Versão do esquema da compactação.
pub const COMPACTION_SCHEMA_VERSION: u32 = 1;

/// Bytes de excerto por linha do digest.
const DIGEST_EXCERPT_BYTES: usize = 48;

/// Custo fixo do header da tabela `m` ao emitir (`RS` + `m` + `\n`).
const DIGEST_HEADER_BYTES: usize = 3;

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

/// Uma linha do digest: tipo estável e texto estruturado.
struct DigestRow {
    /// Tipo estável da mensagem.
    kind: &'static str,
    /// Excerto determinístico de uma linha.
    text: String,
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
    let _span = crate::trace_fn!("context::compact::compact");

    if mode == CompactionMode::Disabled {
        return None;
    }
    let _span = crate::fn_span!(
        Level::Debug,
        events::CONTEXT_COMPACT,
        "context::compact::compact",
        "raw_min" => budget.raw_min,
        "summary_max" => budget.summary_max,
    );
    let all = derive_messages(events);
    let kept = fit_raw(&all, budget.raw_min);
    let prefix_len = all.len().saturating_sub(kept.len());
    let prefix = all.get(..prefix_len).unwrap_or_default();
    let (replacements, rows) = digest_rows(prefix);
    let summary = fit_digest(&rows, budget.summary_max);
    crate::event!(
        Level::Debug,
        events::CONTEXT_DIGEST,
        "rows" => rows.len(),
        "bytes" => summary.len(),
        "tokens" => tokens_from_bytes(summary.len()),
    );
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
    let _span = crate::trace_fn!("context::compact::recover");

    derive_messages(events)
        .into_iter()
        .find(|message| message_id(message) == id)
}

/// Id de conteúdo de uma mensagem (endereçável no log).
#[must_use]
pub fn message_id(message: &Message) -> String {
    let _span = crate::trace_fn!("context::compact::message_id");

    content_id("m", &serde_json::to_vec(message).unwrap_or_default())
}

/// Constrói as linhas do digest e o mapeamento original→substituto (determinístico).
fn digest_rows(prefix: &[Message]) -> (Vec<Replacement>, Vec<DigestRow>) {
    let _span = crate::fn_span!(
        Level::Trace,
        events::CONTEXT_DIGEST,
        "context::compact::digest_rows"
    );
    let mut replacements = Vec::with_capacity(prefix.len());
    let mut rows = Vec::with_capacity(prefix.len());
    for message in prefix {
        let original = message_id(message);
        let kind = message_kind(message);
        let text = excerpt(message);
        let substitute = content_id("c", format!("{kind} {original} {text}").as_bytes());
        replacements.push(Replacement {
            original,
            substitute,
        });
        rows.push(DigestRow { kind, text });
    }
    (replacements, rows)
}

/// Emite a tabela `m` com as linhas que cabem no teto de tokens.
///
/// O comprimento emitido é exato: a sanitização de células preserva bytes, pelo que o orçamento é
/// respeitado sem re-renderizar.
fn fit_digest(rows: &[DigestRow], summary_max: usize) -> String {
    let _span = crate::fn_span!(
        Level::Trace,
        events::CONTEXT_DIGEST,
        "context::compact::fit_digest",
        "rows" => rows.len(),
        "max" => summary_max,
    );
    let mut table = RowTable::new("m");
    let mut bytes = DIGEST_HEADER_BYTES;
    for row in rows {
        let row_bytes = row
            .kind
            .len()
            .saturating_add(row.text.len())
            .saturating_add(2); // `US` + `\n`
        if tokens_from_bytes(bytes.saturating_add(row_bytes)) > summary_max {
            break;
        }
        bytes = bytes.saturating_add(row_bytes);
        table.push(vec![Cell::text(row.kind), Cell::text(row.text.clone())]);
    }
    emit(&[Section::Rows(table)])
}

/// Tipo estável de uma mensagem.
fn message_kind(message: &Message) -> &'static str {
    let _span = crate::trace_fn!("context::compact::message_kind");

    match message {
        Message::User { .. } => "user",
        Message::Assistant { .. } => "assistant",
        Message::ToolCall { .. } => "tool_call",
        Message::ToolResult { .. } => "tool_result",
    }
}

/// Excerto curto e determinístico (sem `Debug`).
fn excerpt(message: &Message) -> String {
    let _span = crate::trace_fn!("context::compact::excerpt");

    let text = match message {
        Message::User { text } | Message::Assistant { text } => text.clone(),
        Message::ToolCall { tool, .. } => tool_call_text(tool),
        Message::ToolResult { outcome, .. } => outcome.summary(),
    };
    truncate(&text, DIGEST_EXCERPT_BYTES)
}

/// Texto de um pedido de tool: nome estável + primeiro caminho resolvido.
fn tool_call_text(tool: &ToolUse) -> String {
    let _span = crate::trace_fn!("context::compact::tool_call_text");

    let name = tool.name.as_str();
    match tool.resolved_paths.first() {
        Some(path) => format!("{name} {}", path.as_str()),
        None => name.to_string(),
    }
}

/// Corta em fronteira de caractere e marca a elipse.
fn truncate(text: &str, max_bytes: usize) -> String {
    let _span = crate::trace_fn!("context::compact::truncate");

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
