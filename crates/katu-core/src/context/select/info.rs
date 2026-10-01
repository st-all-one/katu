//! Informação do contexto: contagem de termos e divergência JS (Q-03).
//!
//! A distribuição é **empírica** (contagem dos termos presentes), sem corpus externo: a divergência
//! mede o que o digest/sufixo representa do original, não o que um modelo julgaria relevante. É o
//! número que o gatilho `τ_JS` usa e o que o A/B publica.

use crate::diag::{Level, events};
use crate::evidence::from_f64;
use std::collections::{BTreeMap, BTreeSet};

/// Distribuição de termos de um conjunto de candidatos (contagem por termo).
#[must_use]
pub fn term_counts(terms: impl IntoIterator<Item = BTreeSet<String>>) -> BTreeMap<String, u32> {
    let _span = crate::trace_fn!("context::select::term_counts");

    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    for set in terms {
        for term in set {
            let slot = counts.entry(term).or_insert(0);
            *slot = slot.saturating_add(1);
        }
    }
    counts
}

/// Divergência **JS** entre duas distribuições de termos, em milésimos de nat.
///
/// `JS(P‖Q) = ½ KL(P‖M) + ½ KL(Q‖M)` com `M = (P+Q)/2` e suavização de Laplace, para que uma
/// distribuição vazia seja tratada (e não produza `ln 0`). `0` ⇒ o sufixo cobre o prefixo.
#[must_use]
pub fn js_milli(left: &BTreeMap<String, u32>, right: &BTreeMap<String, u32>) -> u64 {
    let _span = crate::fn_span!(
        Level::Trace,
        events::CONTEXT_DIGEST,
        "context::select::js_milli",
        "left" => left.len(),
        "right" => right.len(),
    );

    let mut vocabulary: BTreeSet<&String> = BTreeSet::new();
    vocabulary.extend(left.keys());
    vocabulary.extend(right.keys());
    if vocabulary.is_empty() {
        return 0;
    }
    let left_total = f64::from(left.values().copied().sum::<u32>()) + 1.0;
    let right_total = f64::from(right.values().copied().sum::<u32>()) + 1.0;
    let size = f64::from(u32::try_from(vocabulary.len()).unwrap_or(u32::MAX));
    let mut total = 0.0f64;
    for term in vocabulary {
        let p = (f64::from(left.get(term).copied().unwrap_or(0)) + 1.0 / size) / left_total;
        let q = (f64::from(right.get(term).copied().unwrap_or(0)) + 1.0 / size) / right_total;
        let m = f64::midpoint(p, q);
        total += (0.5 * q).mul_add((q / m).ln(), 0.5 * p * (p / m).ln());
    }
    if total.is_finite() && total > 0.0 {
        from_f64(total * 1_000.0)
    } else {
        0
    }
}
