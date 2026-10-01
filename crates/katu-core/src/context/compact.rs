//! Compactação determinística do histórico antigo (E09-T07) — guiada por **informação** (Q-03).
//!
//! **Controlo do core, nunca inline no hot path.** O prefixo que não cabe no orçamento é
//! substituído por um **digest determinístico** (sem LLM): para o mesmo input, o mesmo resumo e o
//! mesmo mapeamento original→substituto — preservando o cache de prefixo do provider (§1). O
//! original **não** se perde: continua endereçável no log por [`recover`].
//!
//! O digest é uma tabela `m` (ADR 0006): `kind`, `id` e um `text` de uma linha por mensagem. Não
//! há `Debug` no caminho — o texto é estruturado (outcome → `ToolOutcome::summary`).
//!
//! **Q-03:** com [`SelectionPolicy::Utility`] as linhas são escolhidas pela **mesma** máquina da
//! seleção de unidades (utilidade submodular + MMR + RRF, [`super::select`]) e a compactação só se
//! aplica se o prefixo disser algo que o sufixo não diz — `JS(prefixo ‖ sufixo) ≥ τ_JS`. A política
//! histórica (`Suffix`) mantém a truncagem cronológica, byte a byte.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::select::{self, Candidate, SelectionParams, SelectionPolicy};
use super::{AssembleOptions, Context, ContextBudget, PrimeMode, assemble_all, tokens_from_bytes};
use crate::diag::{Level, events};
use crate::evidence::{EvidenceBasis, EvidenceError, Metric, Unit, to_f64};
use crate::kernel::{Event, Message, derive_messages};
use crate::report::content_id;
use crate::toon::{Cell, RowTable, Section, emit};

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

/// Digest do prefixo (parte pura da compactação) e as métricas que o justificam.
#[derive(Debug, Clone, PartialEq)]
pub struct Digest {
    /// Tabela `m` emitida (TOON colunar).
    pub summary: String,
    /// Mapeamento original→substituto (determinístico).
    pub replacements: Vec<Replacement>,
    /// `I_ret`: massa de informação retida (milésimos de nat; base `inferred`).
    pub information: Metric,
    /// `JS(prefixo ‖ sufixo)`: quanto o prefixo diz que o sufixo já não diz (gatilho `τ_JS`).
    pub divergence: Metric,
    /// Linhas retidas.
    pub kept_rows: usize,
    /// Linhas do prefixo.
    pub total_rows: usize,
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
/// Delega no **caminho único** de orçamento ([`assemble_all`], S-01): o prefixo é exatamente o que
/// não coube em `raw_min`, e a compactação é o que sobra dessa decisão.
///
/// Devolve `None` quando a compactação está desligada, quando não há prefixo ou quando o resumo não
/// se paga (política de utilidade e `JS < τ_JS`). O original continua endereçável no log
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
    let assembly = assemble_all(
        events,
        budget,
        AssembleOptions::new(PrimeMode::Compact, mode),
    );
    match assembly.compaction {
        Some(compaction) => Some(compaction),
        // Contrato histórico: com a compactação ligada e **nada** fora do orçamento, devolve-se a
        // pré-visualização vazia (o gatilho olha para as substituições, não para o `Option`).
        None if assembly.dropped == 0 => empty_preview(assembly.context).ok(),
        None => None,
    }
}

/// Pré-visualização vazia (nada a compactar) — mantém o contrato de `compact`.
fn empty_preview(context: Context) -> Result<Compaction, EvidenceError> {
    let _span = crate::trace_fn!("context::compact::empty_preview");

    let original_tokens = context.raw_tokens;
    Ok(Compaction {
        schema_version: COMPACTION_SCHEMA_VERSION,
        context,
        replacements: Vec::new(),
        original_tokens,
        gain: Metric::new(
            "context.gain_tokens",
            0.0,
            Unit::Tokens,
            EvidenceBasis::Inferred,
            None,
        )?,
    })
}

/// Digest do prefixo: linhas, mapeamentos e as duas métricas (Q-03).
///
/// # Erros
/// [`EvidenceError`] se uma métrica não for construível (não acontece com base `inferred`).
pub fn digest(
    prefix: &[Message],
    kept: &[Message],
    summary_max: usize,
    policy: SelectionPolicy,
    params: SelectionParams,
) -> Result<Digest, EvidenceError> {
    let _span = crate::fn_span!(
        Level::Debug,
        events::CONTEXT_DIGEST,
        "context::compact::digest",
        "prefix" => prefix.len(),
        "summary_max" => summary_max,
        "policy" => policy.as_str(),
    );

    let (replacements, rows) = digest_rows(prefix);
    let candidates: Vec<Candidate> = rows.iter().map(row_candidate).collect();
    let chosen = match policy {
        SelectionPolicy::Suffix => chronological_fit(&candidates, summary_max),
        SelectionPolicy::Utility => {
            let budget = summary_max.saturating_sub(tokens_from_bytes(DIGEST_HEADER_BYTES));
            select::greedy(&candidates, budget, None, &BTreeSet::new(), params)
        }
    };
    let summary = emit_rows(&rows, &chosen);
    let stats = select::Stats::of(&candidates);
    let retained: BTreeSet<String> = chosen
        .iter()
        .filter_map(|index| candidates.get(*index))
        .flat_map(|candidate| candidate.terms.iter().cloned())
        .collect();
    let divergence = select::js_milli(&term_counts(prefix), &term_counts(kept));
    crate::event!(
        Level::Debug,
        events::CONTEXT_DIGEST,
        "rows" => rows.len(),
        "kept_rows" => chosen.len(),
        "bytes" => summary.len(),
        "tokens" => tokens_from_bytes(summary.len()),
        "information_milli" => stats.mass_milli(&retained),
        "js_milli" => divergence,
    );
    Ok(Digest {
        summary,
        replacements,
        information: Metric::new(
            "context.information_milli",
            to_f64(stats.mass_milli(&retained)),
            Unit::Unspecified,
            EvidenceBasis::Inferred,
            None,
        )?,
        divergence: Metric::new(
            "context.divergence_milli",
            to_f64(divergence),
            Unit::Unspecified,
            EvidenceBasis::Inferred,
            None,
        )?,
        kept_rows: chosen.len(),
        total_rows: rows.len(),
    })
}

/// Candidato de uma linha do digest.
///
/// Os termos vêm do **texto emitido** (é o que o modelo vê) e os bytes contam o separador e o fim de
/// linha que a tabela acrescenta — o mesmo número que [`chronological_fit`] sempre somou.
fn row_candidate(row: &DigestRow) -> Candidate {
    let _span = crate::trace_fn!("context::compact::row_candidate");

    let bytes = row
        .kind
        .len()
        .saturating_add(row.text.len())
        .saturating_add(2);
    Candidate {
        terms: select::terms(&row.text),
        bytes,
        tokens: tokens_from_bytes(bytes),
        evidence: 0,
    }
}

/// Contagem de termos de um conjunto de mensagens (distribuição empírica).
fn term_counts(messages: &[Message]) -> std::collections::BTreeMap<String, u32> {
    let _span = crate::trace_fn!("context::compact::term_counts");

    select::term_counts(
        messages
            .iter()
            .map(|message| select::terms(&select::message_text(message))),
    )
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

/// Índices das primeiras linhas cujo custo cabe em `summary_max` (truncagem cronológica).
///
/// O comprimento emitido é exato: a sanitização de células preserva bytes, pelo que o orçamento é
/// respeitado sem re-renderizar.
fn chronological_fit(candidates: &[Candidate], summary_max: usize) -> Vec<usize> {
    let _span = crate::fn_span!(
        Level::Trace,
        events::CONTEXT_DIGEST,
        "context::compact::chronological_fit",
        "rows" => candidates.len(),
        "max" => summary_max,
    );
    let mut bytes = DIGEST_HEADER_BYTES;
    let mut chosen = Vec::new();
    for (index, candidate) in candidates.iter().enumerate() {
        if tokens_from_bytes(bytes.saturating_add(candidate.bytes)) > summary_max {
            break;
        }
        bytes = bytes.saturating_add(candidate.bytes);
        chosen.push(index);
    }
    chosen
}

/// Emite a tabela `m` com as linhas escolhidas, **na ordem do log**.
fn emit_rows(rows: &[DigestRow], chosen: &[usize]) -> String {
    let _span = crate::fn_span!(
        Level::Trace,
        events::CONTEXT_DIGEST,
        "context::compact::emit_rows",
        "rows" => chosen.len(),
    );
    let mut table = RowTable::new("m");
    for index in chosen {
        let Some(row) = rows.get(*index) else {
            continue;
        };
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

    truncate(&select::message_text(message), DIGEST_EXCERPT_BYTES)
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
