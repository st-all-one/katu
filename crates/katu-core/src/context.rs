//! Montagem de contexto com **orçamento** (E09-T01) — **um só caminho** (S-01).
//!
//! Invariante: nenhuma mensagem entra sem **origem no log** (`Model-visible ⟺ logged`). A montagem
//! é pura (sem I/O): recebe os eventos, projeta-os com [`derive_messages`] uma única vez e decide
//! o que cabe no orçamento com a máquina partilhada de [`select`] — a mesma que escolhe as linhas do
//! digest. Antes de S-01 o turno derivava o log **três vezes** (`context` → `compact` → `assemble`)
//! e repetia a contagem de tokens; agora há uma derivação, uma partição e um teto.
//!
//! A contagem de tokens usa o rácio **medido** [`BYTES_PER_TOKEN_MILLI`] (Q-01; tokenizer do modelo
//! local, `bench/e18/tokens/`), publicado com base `measured` (DF5).

use serde::{Deserialize, Serialize};

use crate::context::prime::prime_text;
use crate::diag::{Level, events};
use crate::error::ToolOutcome;
#[cfg(feature = "instrument")]
use crate::evidence::from_f64;
use crate::evidence::{EvidenceBasis, Metric, Unit, to_f64};
use crate::kernel::{Event, Message, derive_messages};
use katu_policy::ToolUse;

mod compact;
mod prime;
mod select;
mod state;

pub use compact::{
    COMPACTION_SCHEMA_VERSION, Compaction, CompactionMode, Digest, Replacement, compact, digest,
    message_id, recover,
};
pub use prime::{PRIME_VERSION, PrimeMode, prime, prime_for, prime_long, prime_with_catalog};
pub use select::{
    CHANNELS, Candidate, SELECTION_SCHEMA_VERSION, SelectionParams, SelectionPolicy, Stats,
    chosen_units, dropped_messages, greedy, jaccard_milli, js_milli, kept_messages, message_text,
    pin_unit, suffix_start, term_counts, terms, units,
};
pub use state::{
    MAX_SECTION_BYTES, MAX_WORKING_SET, STATE_SCHEMA_VERSION, StateView, section as state_section,
};

/// Orçamento de contexto: **mínimo para o cru, teto para o resumido**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextBudget {
    /// Tokens reservados para mensagens cruas (o sufixo mais recente).
    pub raw_min: usize,
    /// Teto de tokens para o resumo/compactação (usado a partir de E09-T07).
    pub summary_max: usize,
}

/// Contexto montado: prime + (resumo) + mensagens cruas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Context {
    /// Prime compacto, determinístico e versionado (aparece **uma** vez).
    pub prime: String,
    /// Resumo do histórico antigo (preenchido pela compactação de E09-T07; `None` sem compactar).
    pub summary: Option<String>,
    /// Mensagens cruas que cabem no orçamento (ordem do log).
    pub messages: Vec<Message>,
    /// Tokens estimados das mensagens cruas.
    pub raw_tokens: usize,
    /// Tokens estimados do contexto inteiro (prime + resumo + cru).
    pub tokens: usize,
}

/// Opções de montagem — **o** caminho de orçamento (S-01).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssembleOptions<'a> {
    /// Variante do prime (compacto ou completo).
    pub prime: PrimeMode,
    /// Compactação ligada/desligada (explícito, nunca automático).
    pub compaction: CompactionMode,
    /// Política de seleção do que entra no orçamento.
    pub selection: SelectionPolicy,
    /// Parâmetros da seleção (dados versionados).
    pub params: SelectionParams,
    /// Termos do objetivo do turno (canal `objetivo` da fusão; vazio = neutro).
    pub goal: &'a str,
    /// Secção de **estado** já renderizada (Q-04); `None` = prime estático.
    pub state: Option<&'a str>,
}

impl AssembleOptions<'_> {
    /// Opções mínimas: prime no modo pedido, sem estado, política histórica.
    #[must_use]
    pub const fn new(prime: PrimeMode, compaction: CompactionMode) -> Self {
        Self {
            prime,
            compaction,
            selection: SelectionPolicy::Suffix,
            params: SelectionParams::DEFAULT,
            goal: "",
            state: None,
        }
    }
}

impl Default for AssembleOptions<'_> {
    fn default() -> Self {
        let _span = crate::trace_fn!("context::assemble_options_default");

        Self::new(PrimeMode::Compact, CompactionMode::Disabled)
    }
}

/// Resultado da montagem: o contexto e, se houve prefixo a resumir, a compactação.
#[derive(Debug, Clone, PartialEq)]
pub struct Assembly {
    /// Contexto efetivo do turno.
    pub context: Context,
    /// Compactação aplicada (`None` se desligada, sem prefixo ou redundante).
    pub compaction: Option<Compaction>,
    /// Mensagens que ficaram fora do orçamento cru (o prefixo do digest).
    pub dropped: usize,
}

/// Monta o contexto a partir do log, respeitando o orçamento (prime compacto).
#[must_use]
pub fn assemble(events: &[Event], budget: ContextBudget) -> Context {
    let _span = crate::trace_fn!("context::assemble");

    assemble_all(events, budget, AssembleOptions::default()).context
}

/// Monta o contexto no modo de prime pedido (`--long` usa a spec completa — E09-T01).
#[must_use]
pub fn assemble_with_prime(events: &[Event], budget: ContextBudget, mode: PrimeMode) -> Context {
    let _span = crate::trace_fn!("context::assemble_with_prime");

    assemble_all(
        events,
        budget,
        AssembleOptions::new(mode, CompactionMode::Disabled),
    )
    .context
}

/// Monta contexto **e** compactação numa só passagem pelo log (S-01).
///
/// O que não cabe em `raw_min` é o prefixo; é sobre ele — e só sobre ele — que o digest trabalha.
/// Com [`SelectionPolicy::Utility`] o digest segue a mesma utilidade da seleção e só se aplica se
/// `JS(prefixo ‖ sufixo) ≥ τ_JS` (um resumo que não acrescenta informação não se paga).
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "é o caminho único de orçamento: separar em helpers esconderia a ordem das decisões"
)]
pub fn assemble_all(
    events: &[Event],
    budget: ContextBudget,
    options: AssembleOptions<'_>,
) -> Assembly {
    let _span = crate::fn_span!(
        Level::Debug,
        events::CONTEXT_BUILD,
        "context::assemble_all",
        "raw_min" => budget.raw_min,
        "summary_max" => budget.summary_max,
        "policy" => options.selection.as_str(),
    );
    let all = derive_messages(events);
    let units = units(&all);
    let mut chosen = chosen_units(
        &units,
        budget.raw_min,
        options.selection,
        options.goal,
        options.params,
    );
    // A instrução corrente (a última mensagem do utilizador) é **sempre** visível: o sufixo pode
    // evictá-la quando os resultados das tools enchem o orçamento, e o modelo perde a tarefa.
    // Reserva-se o seu custo (o sufixo recua o suficiente) sem exceder `raw_min`; se nem ela cabe,
    // o orçamento manda e nada é pinado.
    if let Some(pin) = pin_unit(&all, &units) {
        let pin_tokens = units.get(pin).map_or(0, |unit| unit.candidate.tokens);
        if !chosen.contains(&pin) && pin_tokens <= budget.raw_min {
            chosen = chosen_units(
                &units,
                budget.raw_min.saturating_sub(pin_tokens),
                options.selection,
                options.goal,
                options.params,
            );
            chosen.push(pin);
            chosen.sort_unstable();
        }
    }
    let messages = kept_messages(&all, &units, &chosen);
    let prefix = dropped_messages(&all, &units, &chosen);
    let raw_tokens = messages.iter().map(message_weight).sum::<usize>();

    let compaction = if options.compaction == CompactionMode::Enabled && !prefix.is_empty() {
        compact_prefix(&prefix, &messages, budget, options)
    } else {
        None
    };
    let summary = compaction
        .as_ref()
        .and_then(|compaction| compaction.context.summary.clone());
    let summary_tokens = summary
        .as_deref()
        .map_or(0, |text| tokens_from_bytes(text.len()));
    let prime = prime_text(options);
    let tokens = tokens_from_bytes(prime.len())
        .saturating_add(summary_tokens)
        .saturating_add(raw_tokens);
    crate::event!(
        Level::Debug,
        events::CONTEXT_BUILD,
        "messages" => messages.len(),
        "units" => units.len(),
        "kept" => chosen.len(),
        "raw_tokens" => raw_tokens,
        "tokens" => tokens,
    );
    Assembly {
        context: Context {
            prime,
            summary,
            messages,
            raw_tokens,
            tokens,
        },
        compaction,
        dropped: prefix.len(),
    }
}

/// Digest do prefixo, com o gatilho `τ_JS` da política de utilidade (Q-03).
fn compact_prefix(
    prefix: &[Message],
    kept: &[Message],
    budget: ContextBudget,
    options: AssembleOptions<'_>,
) -> Option<Compaction> {
    let _span = crate::fn_span!(
        Level::Debug,
        events::CONTEXT_COMPACT,
        "context::compact_prefix"
    );

    let digest = digest(
        prefix,
        kept,
        budget.summary_max,
        options.selection,
        options.params,
    )
    .ok()?;
    if options.selection == SelectionPolicy::Utility
        && digest.divergence.value < to_f64(u64::from(options.params.tau_js_milli))
    {
        // O resumo não cobre nada que o sufixo já não diga: não se gasta o orçamento.
        crate::event!(
            Level::Debug,
            events::CONTEXT_DIGEST,
            "js_milli" => from_f64(digest.divergence.value),
            "tau_milli" => options.params.tau_js_milli,
            "applied" => 0,
        );
        return None;
    }
    let Digest {
        summary,
        replacements,
        ..
    } = digest;
    let kept_tokens = kept.iter().map(message_weight).sum::<usize>();
    let summary_tokens = tokens_from_bytes(summary.len());
    let compacted = summary_tokens.saturating_add(kept_tokens);
    let original = prefix
        .iter()
        .map(message_weight)
        .fold(kept_tokens, usize::saturating_add);
    let saved = original.saturating_sub(compacted);
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
            prime: prime_text(options),
            summary: Some(summary),
            messages: kept.to_vec(),
            raw_tokens: kept_tokens,
            tokens: tokens_from_bytes(prime_text(options).len()).saturating_add(compacted),
        },
        replacements,
        original_tokens: original,
        gain,
    })
}

/// Gatilho do kernel (E09-T07): há prefixo fora do orçamento a compactar?
#[must_use]
pub fn needs_compaction(events: &[Event], budget: ContextBudget) -> bool {
    let _span = crate::trace_fn!("context::needs_compaction");

    assemble_all(
        events,
        budget,
        AssembleOptions::new(PrimeMode::Compact, CompactionMode::Enabled),
    )
    .compaction
    .is_some_and(|compaction| !compaction.replacements.is_empty())
}

/// Estimativa determinística de tokens de uma mensagem (sem tokenizer).
fn message_weight(message: &Message) -> usize {
    let _span = crate::trace_fn!("context::message_weight");

    let bytes = match message {
        Message::User { text, .. } | Message::Assistant { text } => text.len(),
        Message::ToolCall { tool, .. } => tool_weight(tool),
        // O **delta** é o payload model-visible (§18/G6): conta para o orçamento como qualquer
        // outra mensagem — se não contasse, o contexto excedia o teto em silêncio.
        Message::ToolResult { outcome, delta, .. } => {
            outcome_weight(outcome).saturating_add(delta.as_deref().map_or(0, str::len))
        }
    };
    tokens_from_bytes(bytes)
}

/// Peso estimado de um `ToolUse` (caminhos + `argv` + `cwd`).
fn tool_weight(tool: &ToolUse) -> usize {
    let _span = crate::trace_fn!("context::tool_weight");

    let paths: usize = tool
        .resolved_paths
        .iter()
        .map(|path| path.as_str().len())
        .sum();
    let argv: usize = tool
        .argv
        .as_ref()
        .map_or(0, |argv| argv.as_slice().iter().map(String::len).sum());
    paths
        .saturating_add(argv)
        .saturating_add(tool.cwd.as_str().len())
}

/// Peso estimado de um resultado de tool (evidência/controlo).
fn outcome_weight(outcome: &ToolOutcome) -> usize {
    let _span = crate::trace_fn!("context::outcome_weight");

    match outcome {
        ToolOutcome::Denied { evidence, .. } => {
            evidence.argument.len().saturating_add(evidence.fact.len())
        }
        ToolOutcome::Unavailable { control, .. } => control.as_str().len(),
        _ => 0,
    }
}

/// Rácio **medido** bytes/token (Q-01), em milésimos (`3.631`).
///
/// Medido com o tokenizer do modelo local (`qwen2.5-coder-1.5b`, llama.cpp) sobre um corpus fixo e
/// misto (prosa PT, código Rust, TOON, JSON Schema, caminhos) — protocolo e artefacto em
/// `bench/e18/tokens/` (base `measured`, DF5). Substitui a estimativa `bytes/4` do E09-T01, que
/// **subestimava** os tokens ~9 % (orçamento admitia mais texto do que julgava). Outro modelo ⇒
/// outro rácio: o desvio fica observável em `provider.request` (`input_tokens`).
pub const BYTES_PER_TOKEN_MILLI: u64 = 3_631;

/// Estimativa determinística de tokens: `ceil(bytes · 1000 / rácio_medido)`.
///
/// Pública porque o gate `xtask gate:prompt` (Q-20) mede a composição do prompt com **o mesmo**
/// rácio que o orçamento do kernel usa — uma segunda constante seria *drift* garantido.
#[must_use]
pub fn tokens_from_bytes(bytes: usize) -> usize {
    let _span = crate::fn_span!(
        Level::Trace,
        events::CONTEXT_BUILD,
        "context::tokens_from_bytes",
        "bytes" => bytes,
    );
    let scaled = u64::try_from(bytes)
        .unwrap_or(u64::MAX)
        .saturating_mul(1_000)
        .div_ceil(BYTES_PER_TOKEN_MILLI);
    usize::try_from(scaled).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests;
