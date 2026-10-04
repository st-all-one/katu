//! A/B do índice de auditoria (ADR 0009). **Dev-only**: `cargo run -p xtask -- bench-audit`.
//!
//! Mede densidade (tabela `t` colunar vs binário delta+varint+Bloom) e o custo de CPU do codec
//! (build/encode/decode). É o que decide a adoção do formato binário (base DF5).
#![allow(
    clippy::print_stdout,
    reason = "micro-bench dev-only: imprime a tabela"
)]

use katu_core::kernel::Visibility;
use std::time::Instant;

use katu_core::audit::{AuditRecord, Index, decode_index, encode_index};
use katu_core::kernel::Event;
use katu_core::toon::{Cell, RowTable, Section, emit};

/// Tamanhos de corpus medidos.
const SIZES: [usize; 3] = [1_000, 10_000, 100_000];

/// Corre o A/B e imprime as tabelas.
pub(crate) fn run() {
    println!(
        "{:<8} {:>10} {:>10} {:>8} {:>10} {:>10}",
        "events", "text(B)", "bin(B)", "minus", "build(us)", "decode(us)"
    );
    for size in SIZES {
        let records = corpus(size);
        let index = Index::build(&records);
        let text = legacy_index(&index);
        let binary = encode_index(&index);
        let minus = saving(binary.len(), text.len());
        let build_us = micros(3, || {
            let built = Index::build(&records);
            std::hint::black_box(built.postings().len())
        });
        let decode_us = micros(20, || {
            decode_index(&binary).map_or(0, |decoded| decoded.postings().len())
        });
        println!(
            "{size:<8} {:>10} {:>10} {minus:>7}% {build_us:>10} {decode_us:>10}",
            text.len(),
            binary.len(),
        );
    }
}

/// Corpus sintético representativo (mensagens, chamadas e resultados de tool).
fn corpus(size: usize) -> Vec<AuditRecord> {
    let tools = ["read", "edit", "grep", "exec"];
    (0..size)
        .map(|i| {
            let event = if i.checked_rem(4).unwrap_or(0) == 0 {
                Event::UserMessage {
                    text: format!("pedido {i}: ajustar o parser de toon e a densidade"),
                    visibility: Visibility::User,
                }
            } else if i.checked_rem(4).unwrap_or(0) == 1 {
                Event::AssistantMessage {
                    text: format!("resposta {i}: o parser trata whitespace"),
                }
            } else {
                Event::PhaseTransition {
                    to: katu_policy::Phase::KnowledgeConsulted,
                    outcome: None,
                }
            };
            let tool = tools
                .get(i.checked_rem(tools.len()).unwrap_or(0))
                .copied()
                .unwrap_or("read");
            let mut record = AuditRecord::from_event(u64::try_from(i).unwrap_or(0), &event);
            record.tool = tool.to_string();
            record.path = format!(
                "crates/katu-core/src/audit/part_{}.rs",
                i.checked_rem(97).unwrap_or(0)
            );
            record
        })
        .collect()
}

/// Tamanho do índice no formato **antigo**: tabela `t` colunar.
fn legacy_index(index: &Index) -> String {
    let mut table = RowTable::new("t");
    for (term, postings) in index.postings() {
        for posting in postings {
            table.push(vec![
                Cell::text(term.clone()),
                Cell::int(i64::from(posting.field)),
                Cell::int(i64::from(posting.ln)),
                Cell::int(i64::from(posting.pos)),
            ]);
        }
    }
    emit(&[Section::Rows(table)])
}

/// Mede `iterations` execuções e devolve microssegundos por operação.
#[allow(
    clippy::disallowed_methods,
    reason = "bench dev-only: medição de tempo de parede com `Instant`"
)]
fn micros(iterations: u32, mut run: impl FnMut() -> usize) -> u64 {
    let start = Instant::now();
    for _ in 0..iterations {
        std::hint::black_box(run());
    }
    let elapsed = start.elapsed().as_micros();
    u64::try_from(elapsed)
        .unwrap_or(u64::MAX)
        .checked_div(u64::from(iterations))
        .unwrap_or(0)
}

/// Poupança percentual de `binary` face a `text` (inteira, sem vírgula flutuante).
fn saving(binary: usize, text: usize) -> u64 {
    let base = text.max(1);
    let saved = base.saturating_sub(binary);
    u64::try_from(saved.saturating_mul(100).checked_div(base).unwrap_or(0)).unwrap_or(0)
}
