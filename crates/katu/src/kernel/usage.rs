//! Linha de uso/custo de um turno (E12-T03), com a base de evidência.
//!
//! Vive num módulo próprio para manter `kernel.rs` sob o teto de linhas; a linha alimenta o painel
//! da TUI e **nunca** inventa custo (DF5): só aparece quando o modelo tem preço em `prices.toml`.

use katu_providers::PriceTable;

use crate::agent::TurnReport;

/// Linha de uso/custo do turno (E12-T03), com a base de evidência. `None` sem contabilização.
///
/// O custo só aparece quando o modelo tem preço em `policy/prices.toml` (nunca inventado, DF5).
pub(super) fn usage_line(model: &str, turn: &TurnReport, prices: &PriceTable) -> Option<String> {
    let _span = katu_core::trace_fn!("kernel::usage::usage_line");

    let usage = turn.usage.as_ref()?;
    let mut parts: Vec<String> = Vec::new();
    for (label, tokens) in [
        ("in", usage.input),
        ("out", usage.output),
        ("cache", usage.cached_input),
        ("think", usage.reasoning),
    ] {
        if let Some(count) = tokens {
            parts.push(format!("{label} {count}"));
        }
    }
    if let Some(micros) = prices.cost(model, usage).micros {
        parts.push(format!("custo {micros} µUS$"));
    }
    if parts.is_empty() {
        return Some(format!("tokens n/d ({})", usage.basis.as_str()));
    }
    Some(format!("tokens {}", parts.join(" ")))
}
