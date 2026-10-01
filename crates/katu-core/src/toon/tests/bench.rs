//! A/B do emissor TOON (P-02): **onde** o tempo se gasta e quanto se ganha na escrita direta.
//!
//! A pergunta: `report::to_toon` faz três passagens (projeção `Value`→secções, emissão das secções e
//! junção final). A separação de fases diz qual delas domina; a réplica `legacy_*` **congela** o
//! emissor anterior e é comparada com a produção no mesmo processo — o A/B não depende de reverter o
//! repositório, e o teste afirma que as duas escritas são **byte-idênticas**.
//!
//! Corre-se com `KATU_TOON_OUT=bench/e18/toon/raw.json cargo test -p katu-core --lib -- --ignored
//! --nocapture ab_toon_emitter_by_phase`; o artefacto é publicado e `bench/published.toml` cita-o
//! (DF5). `check-diag` proíbe `println!` em `crates/`: o número sai para **ficheiro**.
//!
//! Base `measured` (relógio) — ao contrário dos A/B de informação (base `inferred`), aqui não há
//! proxy: mede-se o custo de CPU da renderização. O que **não** mede: densidade em tokens (essa vive
//! em `xtask bench-toon`, com o tokenizer real) nem o custo do resto do turno.

use std::hint::black_box;
use std::time::Instant;

use crate::evidence::to_f64;
use crate::report::ToolReport;
use crate::toon::{Cell, RowTable, Section, Value, emit, project};

/// Iterações de cada fase (o número é publicado no artefacto).
pub(super) const ITERATIONS: u32 = 2_000;

/// Linhas do payload de referência (perto do pior caso real: `grep`/`find` com muitas linhas).
const ROWS: i64 = 60;

/// Caminho de uma linha (texto longo, com delimitadores de diretório).
fn path(index: i64) -> String {
    format!("crates/katu-tools/src/tool_{index}/mod.rs")
}

/// Payload representativo: escalares, lista de linhas (colunas do registo), lista aninhada e bloco.
pub(super) fn payload() -> Value {
    let rows: Vec<Value> = (0..ROWS)
        .map(|index| {
            Value::map(vec![
                ("path".to_string(), Value::str(path(index))),
                ("ln".to_string(), Value::int(index.saturating_add(3))),
                ("ty".to_string(), Value::str("code")),
                (
                    "preview".to_string(),
                    Value::str(format!("let allowed_{index} = policy.authorize(&use_)?;")),
                ),
                (
                    "hits".to_string(),
                    Value::list(vec![Value::map(vec![
                        ("ln".to_string(), Value::int(index)),
                        ("ty".to_string(), Value::str("code")),
                        (
                            "preview".to_string(),
                            Value::str("use katu_core::policy; // acentuação: ção"),
                        ),
                    ])]),
                ),
            ])
        })
        .collect();
    Value::map(vec![
        ("path".to_string(), Value::str("crates/katu-tools/src")),
        ("total".to_string(), Value::int(ROWS)),
        ("truncated".to_string(), Value::bool(false)),
        ("matches".to_string(), Value::list(rows)),
        (
            "text".to_string(),
            Value::block("fn main() {\n    let x = 1;\n    assert_eq!(x, 1);\n}"),
        ),
    ])
}

/// Relatório de referência (envelope + payload).
fn report() -> ToolReport {
    ToolReport::new("grep.matches", payload()).with_id("f_0123456789abcdef")
}

/// Fases medidas, em nanossegundos por operação.
#[derive(Debug, Clone, Copy)]
struct Phases {
    /// `project(&payload)` — projeção para secções.
    project: u32,
    /// `emit(&sections)` — escrita das secções (produção).
    emit: u32,
    /// `legacy_emit(&sections)` — escrita congelada (o antes).
    legacy_emit: u32,
    /// `to_toon()` — produção, ponta a ponta.
    to_toon: u32,
    /// `legacy_to_toon()` — projeção + escrita congelada, ponta a ponta.
    legacy_to_toon: u32,
    /// `to_json()` — alternativa de máquina (referência de ordem de grandeza).
    json: u32,
}

/// Mede `iterations` execuções de `run` e devolve ns/op.
#[allow(
    clippy::disallowed_methods,
    reason = "bench opt-in: medição de tempo de parede com `Instant` (só aqui o clippy o permite)"
)]
fn ns_per_op(iterations: u32, mut run: impl FnMut()) -> u32 {
    let start = Instant::now();
    for _ in 0..iterations {
        run();
    }
    let per_op = start
        .elapsed()
        .as_nanos()
        .checked_div(u128::from(iterations))
        .unwrap_or(0);
    u32::try_from(per_op).unwrap_or(u32::MAX)
}

/// Mede todas as fases sobre o mesmo payload (as secções são projetadas uma vez).
fn phases() -> Phases {
    let report = report();
    let sections = project(&report.data);
    let project_ns = ns_per_op(ITERATIONS, || {
        black_box(project(&report.data));
    });
    let emit_ns = ns_per_op(ITERATIONS, || {
        black_box(emit(&sections));
    });
    let legacy_emit_ns = ns_per_op(ITERATIONS, || {
        black_box(legacy_emit(&sections));
    });
    let to_toon_ns = ns_per_op(ITERATIONS, || {
        black_box(report.to_toon());
    });
    let legacy_to_toon_ns = ns_per_op(ITERATIONS, || {
        black_box(legacy_to_toon(&report));
    });
    let json_ns = ns_per_op(ITERATIONS, || {
        black_box(report.to_json().unwrap_or_default());
    });
    Phases {
        project: project_ns,
        emit: emit_ns,
        legacy_emit: legacy_emit_ns,
        to_toon: to_toon_ns,
        legacy_to_toon: legacy_to_toon_ns,
        json: json_ns,
    }
}

/// `antes`/`depois` em milésimos (a razão publicada é `antes/depois`).
fn speedup_milli(before: u32, after: u32) -> u64 {
    u64::from(before)
        .saturating_mul(1_000)
        .checked_div(u64::from(after.max(1)))
        .unwrap_or(0)
}

/// Redução percentual de `before` para `after` (0 quando não há base).
fn gain_pct(before: u32, after: u32) -> f64 {
    if before == 0 {
        return 0.0;
    }
    let retained = to_f64(u64::from(after)) / to_f64(u64::from(before));
    (1.0 - retained) * 100.0
}

/// Mede e serializa o artefacto do A/B.
pub(super) fn measure() -> Result<String, Box<dyn std::error::Error>> {
    let phases = phases();
    let report = report();
    let text = report.to_toon();
    let legacy = legacy_to_toon(&report);
    let sections = project(&report.data);
    let emit_gain = gain_pct(phases.legacy_emit, phases.emit);
    let to_toon_gain = gain_pct(phases.legacy_to_toon, phases.to_toon);
    let value = serde_json::json!({
        "schema": "katu.bench.toon.v1",
        "question": "toon_emitter_single_pass",
        "profile": if cfg!(debug_assertions) { "dev" } else { "release" },
        "iterations": ITERATIONS,
        "payload": {
            "rows": ROWS,
            "bytes": text.len(),
            "sections": sections.len(),
            "identical_to_legacy": text == legacy && emit(&sections) == legacy_emit(&sections),
        },
        "phases_ns": {
            "project": phases.project,
            "emit": phases.emit,
            "emit_legacy": phases.legacy_emit,
            "to_toon": phases.to_toon,
            "to_toon_legacy": phases.legacy_to_toon,
            "json": phases.json,
        },
        "derived": {
            "project_share_pct": 100.0 * to_f64(u64::from(phases.project))
                / to_f64(u64::from(phases.to_toon.max(1))),
            "emit_share_pct": 100.0 * to_f64(u64::from(phases.emit))
                / to_f64(u64::from(phases.to_toon.max(1))),
            "emit_speedup_milli": speedup_milli(phases.legacy_emit, phases.emit),
            "emit_gain_pct": emit_gain,
            "to_toon_speedup_milli": speedup_milli(phases.legacy_to_toon, phases.to_toon),
            "to_toon_gain_pct": to_toon_gain,
            "criterion_pct": 20.0,
            "criterion_met": emit_gain >= 20.0,
        },
        "caveat": "custo de CPU (relógio); a densidade em tokens mede-se com o tokenizer real (xtask bench-toon); a projeção antes de P-02 foi medida em 137,8 µs dev, logo o ganho ponta a ponta real é maior que o `to_toon_gain_pct` (que usa a projeção já otimizada com o emissor antigo)",
    });
    Ok(serde_json::to_string_pretty(&value)?)
}

/// A/B publicado: escreve o artefacto em `KATU_TOON_OUT` (opt-in) e afirma a **igualdade de bytes**.
#[test]
#[ignore = "A/B de P-02: escreve o artefacto em KATU_TOON_OUT (não é asserção de CI)"]
fn ab_toon_emitter_by_phase() -> Result<(), Box<dyn std::error::Error>> {
    let json = measure()?;
    if let Some(path) = std::env::var_os("KATU_TOON_OUT") {
        std::fs::write(path, format!("{json}\n"))?;
    }
    let value: serde_json::Value = serde_json::from_str(&json)?;
    let identical = value
        .get("payload")
        .and_then(|payload| payload.get("identical_to_legacy"))
        .and_then(serde_json::Value::as_bool);
    assert_eq!(
        identical,
        Some(true),
        "o emissor novo tem de ser byte-idêntico ao antigo"
    );
    Ok(())
}

/// O caminho **antigo**, ponta a ponta: projeção + emissor congelado.
fn legacy_to_toon(report: &ToolReport) -> String {
    let mut sections = vec![legacy_envelope(report)];
    sections.extend(project(&report.data));
    legacy_emit(&sections)
}

/// Envelope `r` no formato antigo (mesmas colunas; ausente = vazio).
fn legacy_envelope(report: &ToolReport) -> Section<'_> {
    let row = vec![
        Cell::text(report.kind),
        Cell::text(report.id.clone().unwrap_or_default()),
        Cell::text(""),
        Cell::text(""),
        Cell::int(0),
        Cell::bool(false),
        Cell::int(0),
        Cell::int(0),
        Cell::int(0),
    ];
    let mut table = RowTable::new("r");
    table.push(row);
    Section::Rows(table)
}

/// **Réplica congelada** do emissor anterior a P-02 (uma passagem, mas com `char` a `char` e
/// `write!` por inteiro). Serve de base ao A/B: se a produção divergir, o teste falha.
fn legacy_emit(sections: &[Section<'_>]) -> String {
    let mut out = String::new();
    for section in sections {
        match section {
            Section::Rows(table) if !table.rows.is_empty() => legacy_emit_rows(&mut out, table),
            Section::Literal { name, lines } if !lines.is_empty() => {
                legacy_emit_literal(&mut out, name, lines);
            }
            Section::Rows(_) | Section::Literal { .. } => {}
        }
    }
    out
}

fn legacy_emit_rows(out: &mut String, table: &RowTable<'_>) {
    out.reserve(table.name.len().saturating_add(1));
    out.push('\u{1e}');
    out.push_str(&table.name);
    out.push('\n');
    for row in &table.rows {
        for (index, cell) in row.iter().enumerate() {
            if index > 0 {
                out.push('\u{1f}');
            }
            legacy_emit_cell(out, cell);
        }
        out.push('\n');
    }
}

fn legacy_emit_cell(out: &mut String, cell: &Cell<'_>) {
    use std::fmt::Write as _;
    match cell {
        Cell::Text(text) => legacy_push_sanitized(out, text),
        Cell::Int(number) => {
            write!(out, "{number}").unwrap_or_default();
        }
        Cell::Bool(flag) => out.push(if *flag { '1' } else { '0' }),
    }
}

fn legacy_emit_literal(out: &mut String, name: &str, lines: &[std::borrow::Cow<'_, str>]) {
    out.push('\u{1d}');
    out.push_str(name);
    out.push('\n');
    for line in lines {
        for ch in line.chars() {
            out.push(match ch {
                '\u{1e}' | '\u{1d}' => ' ',
                other => other,
            });
        }
        out.push('\n');
    }
}

fn legacy_push_sanitized(out: &mut String, text: &str) {
    if !text
        .chars()
        .any(|ch| matches!(ch, '\u{1e}' | '\u{1d}' | '\u{1f}' | '\n' | '\r'))
    {
        out.push_str(text);
        return;
    }
    for ch in text.chars() {
        out.push(match ch {
            '\u{1e}' | '\u{1d}' | '\u{1f}' | '\n' | '\r' => ' ',
            other => other,
        });
    }
}
