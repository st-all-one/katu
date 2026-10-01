//! Estatística dos gates (E18-T10/W7): resumo determinístico com IC 95 %.
//!
//! A **fonte única** do cálculo é [`katu_core::stats`] — vive no kernel para o `diag` e o
//! `measure_mvk` a poderem usar sem a borda do `xtask`. Aqui fica só a forma que os artefactos dos
//! gates consomem (`"ci95"`) e o erro legível que aborta uma medição com amostras insuficientes.

use katu_core::stats::{Summary, percentile};
use serde_json::{Value, json};

/// Objeto `"ci95": { "low": …, "high": … }` dos artefactos (ns ou a unidade da métrica).
pub(crate) fn ci95_json(summary: &Summary) -> Value {
    json!({ "low": summary.ci95_low, "high": summary.ci95_high })
}

/// Resumo a partir de amostras; erro legível quando não há amostras bastantes para o gate.
pub(crate) fn summary(samples: &[u64]) -> Result<Summary, String> {
    Summary::try_from_samples(samples).map_err(|error| error.to_string())
}

/// Resumo + p99 + máximo (os gates publicam os três).
pub(crate) fn aggregate(samples: &[u64]) -> Result<(Summary, u64, u64), String> {
    let summary = summary(samples)?;
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let p99 = percentile(&sorted, 9_900);
    let max = sorted.last().copied().unwrap_or(0);
    Ok((summary, p99, max))
}

#[cfg(test)]
mod tests {
    use super::{ci95_json, summary};

    #[test]
    fn too_few_samples_is_an_error() {
        let error = summary(&[1, 2, 3, 4]).err().unwrap_or_default();
        assert!(error.contains("insuficientes"), "{error}");
    }

    #[test]
    fn ci95_json_carries_both_bounds() {
        let Ok(summary) = summary(&[1, 2, 3, 4, 5]) else {
            return;
        };
        let value = ci95_json(&summary);
        assert_eq!(value.get("low"), Some(&serde_json::json!(summary.ci95_low)));
        assert_eq!(
            value.get("high"),
            Some(&serde_json::json!(summary.ci95_high))
        );
    }

    #[test]
    fn the_gate_uses_the_upper_bound() {
        let Ok(summary) = summary(&[100, 100, 100, 100, 100]) else {
            return;
        };
        assert!(summary.within_budget(summary.ci95_high));
        assert!(!summary.within_budget(summary.ci95_high.saturating_sub(1)));
    }
}
