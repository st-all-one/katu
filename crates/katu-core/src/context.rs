//! Montagem de contexto com **orçamento** (E09-T01): prime determinístico + mensagens do log.
//!
//! Invariante: nenhuma mensagem entra sem **origem no log** (`Model-visible ⟺ logged`). A montagem
//! é pura (sem I/O): recebe os eventos, projeta-os com [`derive_messages`] e mantém o **sufixo mais
//! recente** que cabe no orçamento. A contagem de tokens usa o rácio **medido**
//! [`BYTES_PER_TOKEN_MILLI`] (Q-01; tokenizer do modelo local, `bench/e18/tokens/`), publicado com
//! base `measured` (DF5).

use serde::{Deserialize, Serialize};

use crate::diag::{Level, events};
use crate::error::ToolOutcome;
use crate::kernel::{Event, Message, derive_messages};
use crate::toon::schema::{self, Mode};
use katu_policy::ToolUse;

mod compact;

pub use compact::{
    COMPACTION_SCHEMA_VERSION, Compaction, CompactionMode, Replacement, compact, message_id,
    recover,
};

/// Versão do prime (DF12). Mudar o texto do prime exige incrementar isto.
pub const PRIME_VERSION: u32 = 3;

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
    let _span = crate::trace_fn!("context::assemble");

    assemble_with_prime(events, budget, PrimeMode::Compact)
}

/// Monta o contexto no modo de prime pedido (`--long` usa a spec completa — E09-T01).
#[must_use]
pub fn assemble_with_prime(events: &[Event], budget: ContextBudget, mode: PrimeMode) -> Context {
    let _span = crate::fn_span!(
        Level::Debug,
        events::CONTEXT_BUILD,
        "context::assemble_with_prime",
        "raw_min" => budget.raw_min,
        "summary_max" => budget.summary_max,
    );
    let all = derive_messages(events);
    let messages = fit_raw(&all, budget.raw_min);
    let raw_tokens = messages.iter().map(message_weight).sum();
    let prime = prime_for(mode);
    let tokens = tokens_from_bytes(prime.len()).saturating_add(raw_tokens);
    crate::event!(
        Level::Debug,
        events::CONTEXT_BUILD,
        "messages" => messages.len(),
        "raw_tokens" => raw_tokens,
        "tokens" => tokens,
    );
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
    let _span = crate::trace_fn!("context::needs_compaction");

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

/// Gramática do formato, partilhada pelo prime compacto e pelo prime com catálogo.
const PRIME_GRAMMAR: &str = "saida: TOON colunar v3 (D39) SEM headers; o esquema vive aqui:\n\
     - tabela: `\\x1eNOME` e depois linhas com celulas separadas por `\\x1f`, na ordem indicada;\n\
     - literal: `\\x1dNOME` e depois linhas cruas ate a proxima secao;\n\
     - vazio = celula ausente; `{a,b}` = dominio fechado; booleano 0/1; sem floats.\n";

/// Prime compacto (DF12): ensina a gramática do TOON colunar v3 **e o registo de esquema**.
#[must_use]
pub fn prime() -> String {
    let _span = crate::trace_fn!("context::prime");

    format!(
        "katu prime v{PRIME_VERSION}\n\
         tools: read/write/edit/move/trash/bash/grep/find/ls/plan/memory\n\
         {PRIME_GRAMMAR}{}\n\
         JSON com format=json; so o delta chega ao modelo.\n",
        registry_text()
    )
}

/// Prime compacto com o **catálogo de tools** injetado (fonte: `katu_tools::schema::catalog`).
///
/// Substitui a linha `tools:` pela tabela `tool` do registo — o esquema e as tools partilham a
/// mesma projeção colunar, sem a lista escrita à mão.
#[must_use]
pub fn prime_with_catalog(catalog: &str) -> String {
    let _span = crate::trace_fn!("context::prime_with_catalog");

    format!(
        "katu prime v{PRIME_VERSION}\n\
         tools (tabela `tool`; `?` opcional, `{{a,b}}` dominio fechado):\n\
         {catalog}\
         {PRIME_GRAMMAR}{}\n\
         JSON com format=json; so o delta chega ao modelo.\n",
        registry_text()
    )
}

/// Prime completo (spec TOON colunar v3) — `--long` (E09-T01). Determinístico e versionado.
#[must_use]
pub fn prime_long() -> String {
    let _span = crate::trace_fn!("context::prime_long");

    format!(
        "katu prime v{PRIME_VERSION} (long)\n\
         tools: read/write/edit/move/trash/bash/grep/find/ls/plan/memory\n\
         saida: TOON colunar v3 (D39, ADR 0005). Sem headers no stream; o esquema segue.\n\
         - tabela: linha `\\x1eNOME`; linhas seguintes com celulas `\\x1f` na ordem das colunas;\n\
         - literal: linha `\\x1dNOME`; linhas cruas ate a proxima secao;\n\
         - envelope `r` = kind,id,hash,cur,tot,trunc,bytes,ms,tok; escadares da tool em `k` (k,v);\n\
         - ids/paths podem vir como `#N`/`@N` (aliases de sessao), mapeados na secao `sym`;\n\
         - vazio = celula ausente; dominio `{{a,b}}` fechado; booleano 0/1; sem floats.\n\
         {}\n\
         JSON equivalente com `format=json`/`--json`; so o delta chega ao modelo.\n",
        registry_text()
    )
}

/// Uma linha por secção do registo: `nome R col...` ou `nome L`.
fn registry_text() -> String {
    let _span = crate::trace_fn!("context::registry_text");

    let mut out = String::from("esquema:\n");
    for spec in schema::registry() {
        out.push_str(spec.name);
        match spec.mode {
            Mode::Literal => {
                out.push_str(" L\n");
            }
            Mode::Rows => {
                out.push_str(" R");
                for col in spec.cols {
                    out.push(' ');
                    out.push_str(col.name);
                    if !col.domain.is_empty() {
                        out.push('{');
                        out.push_str(&col.domain.join(","));
                        out.push('}');
                    }
                }
                out.push('\n');
            }
        }
    }
    out
}

/// Prime no modo pedido.
#[must_use]
pub fn prime_for(mode: PrimeMode) -> String {
    let _span = crate::trace_fn!("context::prime_for");

    match mode {
        PrimeMode::Compact => prime(),
        PrimeMode::Long => prime_long(),
    }
}

/// Mantém o **sufixo mais recente** cujo peso cabe em `raw_min`.
fn fit_raw(messages: &[Message], raw_min: usize) -> Vec<Message> {
    let _span = crate::fn_span!(Level::Trace, events::CONTEXT_TRIM, "context::fit_raw");
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
    let _span = crate::trace_fn!("context::message_weight");

    let bytes = match message {
        Message::User { text } | Message::Assistant { text } => text.len(),
        Message::ToolCall { tool, .. } => tool_weight(tool),
        Message::ToolResult { outcome, .. } => outcome_weight(outcome),
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
fn tokens_from_bytes(bytes: usize) -> usize {
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
