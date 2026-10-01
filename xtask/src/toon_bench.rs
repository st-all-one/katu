//! Micro-bench do formato ao modelo (ADR 0005/0006, DF5/E15). **Dev-only**:
//! `cargo run -p xtask --features tokenizer -- bench-toon`.
//!
//! Mede tokens reais (`cl100k_base`) de relatórios representativos (v3 vs JSON), o A/B do digest
//! de compactação (antigo `Debug` vs tabela `m`) e o A/B do catálogo de tools, além do custo de
//! CPU da renderização. Publicar exige base DF5.
#![allow(
    clippy::print_stdout,
    reason = "micro-bench dev-only: imprime a tabela"
)]

mod corpus;

use std::time::Instant;

use katu_core::context::{
    CompactionMode, ContextBudget, compact, message_id, prime, prime_with_catalog,
};
use katu_core::error::ToolOutcome;
use katu_core::kernel::{CallId, Event, Message, derive_messages};
use katu_core::toon::{Aliases, Cell, RowTable, Section, emit};
use katu_policy::{Evidence, ResolvedPath, RuleId, ToolArgs, ToolName, ToolUse};
use katu_tools::schema::catalog as tool_catalog;

use corpus::corpus;

/// Corre o micro-bench (todos os A/B) e imprime as tabelas.
pub(crate) fn run() -> Result<(), String> {
    let bpe = tiktoken_rs::cl100k_base().map_err(|err| format!("tokenizer: {err}"))?;
    density(&bpe)?;
    digest(&bpe)?;
    catalog(&bpe);
    timing();
    Ok(())
}

/// A/B de densidade por relatório: v3 vs aliases vs JSON.
fn density(bpe: &tiktoken_rs::CoreBPE) -> Result<(), String> {
    let mut aliases = Aliases::new();
    let (mut v3_total, mut alias_total, mut json_total) = (0_u32, 0_u32, 0_u32);
    for report in corpus() {
        let v3 = report.to_toon();
        let alias = report.to_toon_with(&mut aliases);
        let json = report.to_json().map_err(|err| format!("json: {err}"))?;
        let (v3_tokens, alias_tokens, json_tokens) =
            (tokens(bpe, &v3), tokens(bpe, &alias), tokens(bpe, &json));
        v3_total = v3_total.saturating_add(v3_tokens);
        alias_total = alias_total.saturating_add(alias_tokens);
        json_total = json_total.saturating_add(json_tokens);
        println!(
            "{:<16} v3={v3_tokens:>5}  alias={alias_tokens:>5}  json={json_tokens:>5}  v3-json={:.0}%  alias-json={:.0}%",
            report.kind,
            saving(v3_tokens, json_tokens),
            saving(alias_tokens, json_tokens)
        );
    }
    println!(
        "{:<16} v3={v3_total:>5}  alias={alias_total:>5}  json={json_total:>5}  v3-json={:.0}%  alias-json={:.0}%",
        "TOTAL",
        saving(v3_total, json_total),
        saving(alias_total, json_total)
    );
    Ok(())
}

/// A/B do digest de compactação: formato antigo (`Debug`, linhas soltas) vs tabela `m`.
fn digest(bpe: &tiktoken_rs::CoreBPE) -> Result<(), String> {
    let events = session()?;
    let budget = ContextBudget {
        raw_min: 4,
        summary_max: 512,
    };
    let compaction = compact(&events, budget, CompactionMode::Enabled)
        .ok_or("compactação devia estar ligada")?;
    let new_summary = compaction.context.summary.unwrap_or_default();
    let old_summary = legacy_digest(&events);
    let old_tokens = tokens(bpe, &old_summary);
    let new_tokens = tokens(bpe, &new_summary);
    println!(
        "{:<16} old={old_tokens:>5}  new={new_tokens:>5}  new-old={:+.0}%",
        "digest",
        -saving(new_tokens, old_tokens)
    );
    Ok(())
}

/// A/B do catálogo de tools: prime com a lista à mão vs tabela `tool` (fonte: `SCHEMAS`).
fn catalog(bpe: &tiktoken_rs::CoreBPE) {
    let plain = prime();
    let with = prime_with_catalog(&tool_catalog());
    let (plain_tokens, with_tokens) = (tokens(bpe, &plain), tokens(bpe, &with));
    let delta = i64::from(with_tokens).saturating_sub(i64::from(plain_tokens));
    println!(
        "{:<16} prime={plain_tokens:>5}  +tool={with_tokens:>5}  delta={delta:>+}",
        "catalog"
    );
}

/// A/B do emissor: escrita direta no buffer (novo) vs uma `String` por célula (antigo).
#[allow(
    clippy::disallowed_methods,
    reason = "bench dev-only: medição de tempo de parede com `Instant`"
)]
fn timing() {
    let rows = bench_rows();
    let mut table = RowTable::new("symbols");
    for row in &rows {
        table.push(row.iter().cloned().map(Cell::text).collect());
    }
    let section = Section::Rows(table);
    let iterations = 20_000_u32;
    let (old_ns, _) = time(iterations, || legacy_emit(&rows).len());
    let (new_ns, bytes) = time(iterations, || emit(std::slice::from_ref(&section)).len());
    let speedup = f64::from(old_ns) / f64::from(new_ns.max(1));
    println!(
        "{:<16} emit={new_ns} ns/op  legacy={old_ns} ns/op  speedup={speedup:.2}x  ({bytes} bytes)",
        "timing"
    );
}

/// Mede `iterations` execuções de `run`; devolve ns/op e bytes acumulados.
#[allow(
    clippy::disallowed_methods,
    reason = "bench dev-only: medição de tempo de parede com `Instant`"
)]
fn time(iterations: u32, mut run: impl FnMut() -> usize) -> (u32, usize) {
    let start = Instant::now();
    let mut bytes = 0usize;
    for _ in 0..iterations {
        bytes = bytes.saturating_add(run());
    }
    let elapsed = start.elapsed();
    let per_op = elapsed
        .as_nanos()
        .checked_div(u128::from(iterations))
        .unwrap_or(0);
    (u32::try_from(per_op).unwrap_or(u32::MAX), bytes)
}

/// Linhas realistas (24×4) para o A/B do emissor.
fn bench_rows() -> Vec<Vec<String>> {
    (0..24_i64)
        .map(|index| {
            vec![
                format!("crates/katu-tools/src/tool_{index}.rs"),
                12_i64.saturating_add(index).to_string(),
                "code".to_string(),
                "let allowed = policy.authorize(&use_)?;".to_string(),
            ]
        })
        .collect()
}

/// Emissor no formato **antigo**: uma `String` por célula (réplica do `render()` removido).
fn legacy_emit(rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    out.push('\u{1e}');
    out.push_str("symbols");
    out.push('\n');
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            if index > 0 {
                out.push('\u{1f}');
            }
            out.push_str(&legacy_render(cell));
        }
        out.push('\n');
    }
    out
}

fn legacy_render(text: &str) -> String {
    if !text
        .chars()
        .any(|ch| matches!(ch, '\u{1e}' | '\u{1d}' | '\u{1f}' | '\n' | '\r'))
    {
        return text.to_string();
    }
    text.chars()
        .map(|ch| match ch {
            '\u{1e}' | '\u{1d}' | '\u{1f}' | '\n' | '\r' => ' ',
            other => other,
        })
        .collect()
}

fn tokens(bpe: &tiktoken_rs::CoreBPE, text: &str) -> u32 {
    u32::try_from(bpe.encode_with_special_tokens(text).len()).unwrap_or(u32::MAX)
}

fn saving(toon: u32, json: u32) -> f64 {
    100.0 * (1.0 - f64::from(toon) / f64::from(json.max(1)))
}

/// Sessão representativa com um resultado de tool negado (o pior caso do `Debug` antigo).
fn session() -> Result<Vec<Event>, String> {
    let mut events = Vec::new();
    for turn in 0..3_u32 {
        events.push(Event::UserMessage {
            text: format!("pedido {turn}: analisa o dispatch e o custo por chamada"),
        });
        events.push(Event::AssistantMessage {
            text: format!("resposta {turn}: o custo é advisory e vive na borda do dispatch"),
        });
    }
    let call = CallId::new("c_0001");
    events.push(Event::ToolCall {
        call: call.clone(),
        tool: tool_use()?,
    });
    events.push(Event::ToolResult {
        call,
        outcome: denied(),
        delta: None,
    });
    Ok(events)
}

fn tool_use() -> Result<ToolUse, String> {
    let cwd = ResolvedPath::from_canonical("/work").map_err(|err| err.to_string())?;
    Ok(ToolUse {
        name: ToolName::MemoryRecall,
        args: ToolArgs::Other,
        resolved_paths: Vec::new(),
        argv: None,
        cwd,
    })
}

fn denied() -> ToolOutcome {
    let rule = RuleId::from("mem-recall-before-write");
    ToolOutcome::Denied {
        rule_id: rule.clone(),
        evidence: Evidence::new("sem recall", "/work/src/lib.rs", rule),
    }
}

/// Digesto no formato **antigo** (`kind id excerpt` por linha; `Debug` para tools).
fn legacy_digest(events: &[Event]) -> String {
    let mut out = String::new();
    for message in derive_messages(events) {
        let id = message_id(&message);
        out.push_str(message_kind(&message));
        out.push(' ');
        out.push_str(&id);
        out.push(' ');
        out.push_str(&legacy_excerpt(&message));
        out.push('\n');
    }
    out
}

fn message_kind(message: &Message) -> &'static str {
    match message {
        Message::User { .. } => "user",
        Message::Assistant { .. } => "assistant",
        Message::ToolCall { .. } => "tool_call",
        Message::ToolResult { .. } => "tool_result",
        _ => "other",
    }
}

fn legacy_excerpt(message: &Message) -> String {
    let text = match message {
        Message::User { text } | Message::Assistant { text } => text.clone(),
        Message::ToolCall { tool, .. } => format!("{:?}", tool.name),
        Message::ToolResult { outcome, .. } => format!("{outcome:?}"),
        _ => String::new(),
    };
    if text.len() <= 48 {
        return text;
    }
    let mut end = 48;
    while end > 0 && !text.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    format!("{}…", text.get(..end).unwrap_or_default())
}
