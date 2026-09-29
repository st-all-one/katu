//! Montagem de contexto com **orçamento** (E09-T01): prime determinístico + mensagens do log.
//!
//! Invariante: nenhuma mensagem entra sem **origem no log** (`Model-visible ⟺ logged`). A montagem
//! é pura (sem I/O): recebe os eventos, projeta-os com [`derive_messages`] e mantém o **sufixo mais
//! recente** que cabe no orçamento. A contagem de tokens é uma **estimativa determinística**
//! (`bytes/4`), não um tokenizer de provider — a base fica `inferred` quando publicada (DF5).

use serde::{Deserialize, Serialize};

use crate::diag::{Level, events};
use crate::error::ToolOutcome;
use crate::kernel::{Event, Message, derive_messages};
use katu_policy::ToolUse;

mod compact;

pub use compact::{
    COMPACTION_SCHEMA_VERSION, Compaction, CompactionMode, Replacement, compact, message_id,
    recover,
};

/// Versão do prime (DF12). Mudar o texto do prime exige incrementar isto.
pub const PRIME_VERSION: u32 = 1;

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

/// Monta o contexto a partir do log, respeitando o orçamento (prime compacto).
#[must_use]
pub fn assemble(events: &[Event], budget: ContextBudget) -> Context {
    assemble_with_prime(events, budget, PrimeMode::Compact)
}

/// Monta o contexto no modo de prime pedido (`--long` usa a spec completa — E09-T01).
#[must_use]
pub fn assemble_with_prime(events: &[Event], budget: ContextBudget, mode: PrimeMode) -> Context {
    let _span = crate::span!(
        Level::Debug,
        events::CONTEXT_BUILD,
        "raw_min" => budget.raw_min,
        "summary_max" => budget.summary_max,
    );
    let all = derive_messages(events);
    let messages = fit_raw(&all, budget.raw_min);
    let raw_tokens = messages.iter().map(message_weight).sum();
    let prime = prime_for(mode);
    let tokens = tokens_from_bytes(prime.len()).saturating_add(raw_tokens);
    Context {
        prime,
        summary: None,
        messages,
        raw_tokens,
        tokens,
    }
}

/// Gatilho do kernel (E09-T07): há prefixo fora do orçamento a compactar?
#[must_use]
pub fn needs_compaction(events: &[Event], budget: ContextBudget) -> bool {
    let all = derive_messages(events);
    fit_raw(&all, budget.raw_min).len() < all.len()
}

/// Variante do prime (E09-T01): compacto (default) ou completo (`--long`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrimeMode {
    /// Prime compacto (default).
    #[default]
    Compact,
    /// Prime completo (spec TOON) — `--long`.
    Long,
}

/// Prime compacto (DF12): ensina o envelope das tools e a gramática TOON. Estável por versão.
#[must_use]
pub fn prime() -> String {
    format!(
        "katu prime v{PRIME_VERSION}\n\
         tools: read/write/edit/move/trash/bash/grep/find/ls/plan/memory\n\
         saida: TOON (chave: valor; listas com '-'; blocos com '|'); JSON com format=json\n\
         cada resultado traz kind/id/hash; so o delta chega ao modelo\n"
    )
}

/// Prime completo (spec TOON) — `--long` (E09-T01). Determinístico e versionado.
#[must_use]
pub fn prime_long() -> String {
    format!(
        "katu prime v{PRIME_VERSION} (long)\n\
         tools: read/write/edit/move/trash/bash/grep/find/ls/plan/memory\n\
         saida: TOON — `chave: valor` por linha; listas com `- item`; blocos de texto com `|`;\n\
         strings entre aspas quando contêm `:`/`#`; inteiros e booleanos sem aspas.\n\
         JSON equivalente com `format=json`/`--json`.\n\
         cada resultado traz kind/id/hash/page; so o delta chega ao modelo; o original fica no log.\n"
    )
}

/// Prime no modo pedido.
#[must_use]
pub fn prime_for(mode: PrimeMode) -> String {
    match mode {
        PrimeMode::Compact => prime(),
        PrimeMode::Long => prime_long(),
    }
}

/// Mantém o **sufixo mais recente** cujo peso cabe em `raw_min`.
fn fit_raw(messages: &[Message], raw_min: usize) -> Vec<Message> {
    let mut used = 0usize;
    let mut start = messages.len();
    for (index, message) in messages.iter().enumerate().rev() {
        let cost = message_weight(message);
        if used.saturating_add(cost) > raw_min {
            break;
        }
        used = used.saturating_add(cost);
        start = index;
    }
    messages.get(start..).unwrap_or_default().to_vec()
}

/// Estimativa determinística de tokens de uma mensagem (sem tokenizer).
fn message_weight(message: &Message) -> usize {
    let bytes = match message {
        Message::User { text } | Message::Assistant { text } => text.len(),
        Message::ToolCall { tool, .. } => tool_weight(tool),
        Message::ToolResult { outcome, .. } => outcome_weight(outcome),
    };
    tokens_from_bytes(bytes)
}

/// Peso estimado de um `ToolUse` (caminhos + `argv` + `cwd`).
fn tool_weight(tool: &ToolUse) -> usize {
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
    match outcome {
        ToolOutcome::Denied { evidence, .. } => {
            evidence.argument.len().saturating_add(evidence.fact.len())
        }
        ToolOutcome::Unavailable { control, .. } => control.as_str().len(),
        _ => 0,
    }
}

/// Estimativa `bytes/4` (arredondada para cima).
fn tokens_from_bytes(bytes: usize) -> usize {
    bytes.div_ceil(4)
}

#[cfg(test)]
mod tests;
