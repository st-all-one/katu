//! Contabilização de tokens/custo (E12-T03), com base de evidência (DF5).
//!
//! Não inventa preço: um modelo sem entrada na [`PriceTable`] devolve [`EvidenceBasis::Unpriced`]
//! com `micros = None`. A aritmética é inteira (micro-USD) e determinística.

use std::collections::BTreeMap;

use katu_core::evidence::{ArtifactRef, EvidenceBasis, EvidenceError, Metric, Unit, to_f64};
use katu_core::provider::TokenUsage;

/// Preço de um modelo em **micro-USD por 1M tokens**.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Price {
    /// Tokens de entrada (não cacheados).
    pub input: u64,
    /// Tokens de saída.
    pub output: u64,
    /// Tokens de entrada servidos por prefix-cache.
    pub cached_input: u64,
}

/// Custo calculado de uma chamada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cost {
    /// Custo em micro-USD (`None` quando não há preço público).
    pub micros: Option<u64>,
    /// Base de evidência.
    pub basis: EvidenceBasis,
}

impl Cost {
    /// Converte o custo numa [`Metric`] (DF5/E12-T03), com a base do `usage`.
    ///
    /// O `artifact` é obrigatório quando a base o exige (`provider_reported`); a borda de
    /// *benchmark* fornece-o. Um custo `unpriced` produz métrica zero com base `unpriced`.
    ///
    /// # Errors
    /// [`EvidenceError`] se a base exigir artefacto e nenhum for dado.
    pub fn metric(
        &self,
        model: &str,
        artifact: Option<ArtifactRef>,
    ) -> Result<Metric, EvidenceError> {
        let _span = katu_core::trace_fn!("usage::metric");

        let value = self.micros.map_or(0.0, to_f64);
        Metric::new(
            format!("provider.cost_micros.{model}"),
            value,
            Unit::Micros,
            self.basis,
            artifact,
        )
    }
}

/// Métricas de tokens de uma chamada (DF5/E12-T03), com a base do `usage`.
///
/// Só emite campos reportados; a base viaja com cada número.
///
/// # Errors
/// [`EvidenceError`] se a base exigir artefacto e nenhum for dado.
pub fn usage_metrics(
    model: &str,
    usage: &TokenUsage,
    artifact: Option<&ArtifactRef>,
) -> Result<Vec<Metric>, EvidenceError> {
    let _span = katu_core::trace_fn!("usage::usage_metrics");

    let mut metrics = Vec::new();
    for (field, tokens) in [
        ("input", usage.input),
        ("output", usage.output),
        ("cached_input", usage.cached_input),
        ("reasoning", usage.reasoning),
    ] {
        if let Some(count) = tokens {
            metrics.push(Metric::new(
                format!("provider.tokens.{field}.{model}"),
                to_f64(count),
                Unit::Tokens,
                usage.basis,
                artifact.cloned(),
            )?);
        }
    }
    Ok(metrics)
}

/// Tabela de preços por modelo (vazia por omissão → `unpriced`).
#[derive(Debug, Default, Clone)]
pub struct PriceTable {
    prices: BTreeMap<String, Price>,
}

impl PriceTable {
    /// Tabela vazia.
    #[must_use]
    pub fn new() -> Self {
        let _span = katu_core::trace_fn!("usage::new");

        Self::default()
    }

    /// Regista/atualiza o preço de um modelo.
    pub fn set(&mut self, model: impl Into<String>, price: Price) {
        let _span = katu_core::trace_fn!("usage::set");

        self.prices.insert(model.into(), price);
    }

    /// Calcula o custo; `unpriced` se o modelo não tiver preço.
    #[must_use]
    pub fn cost(&self, model: &str, usage: &TokenUsage) -> Cost {
        let _span = katu_core::trace_fn!("usage::cost");

        self.prices.get(model).map_or(
            Cost {
                micros: None,
                basis: EvidenceBasis::Unpriced,
            },
            |price| Cost {
                micros: Some(compute(price, usage)),
                basis: usage.basis,
            },
        )
    }
}

/// Custo em micro-USD: entrada não-cacheada + cache + saída, por 1M tokens.
fn compute(price: &Price, usage: &TokenUsage) -> u64 {
    let _span = katu_core::trace_fn!("usage::compute");

    let input = usage.input.unwrap_or(0);
    let cached = usage.cached_input.unwrap_or(0).min(input);
    let fresh = input.saturating_sub(cached);
    let output = usage.output.unwrap_or(0);
    let micros = fresh
        .saturating_mul(price.input)
        .saturating_add(cached.saturating_mul(price.cached_input))
        .saturating_add(output.saturating_mul(price.output));
    micros.checked_div(1_000_000).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{Price, PriceTable, usage_metrics};
    use katu_core::evidence::{ArtifactRef, EvidenceBasis, Unit};
    use katu_core::provider::TokenUsage;

    #[test]
    fn unpriced_without_a_table_entry() {
        let table = PriceTable::new();
        let usage = TokenUsage::new(EvidenceBasis::ProviderReported);
        let cost = table.cost("desconhecido", &usage);
        assert_eq!(cost.micros, None);
        assert_eq!(cost.basis, EvidenceBasis::Unpriced);
    }

    #[test]
    fn computes_integer_micro_usd() {
        let mut table = PriceTable::new();
        table.set(
            "m",
            Price {
                input: 1_000_000,  // 1 USD / 1M
                output: 3_000_000, // 3 USD / 1M
                cached_input: 100_000,
            },
        );
        let mut usage = TokenUsage::new(EvidenceBasis::ProviderReported);
        usage.input = Some(1_000_000);
        usage.cached_input = Some(400_000);
        usage.output = Some(1_000_000);
        // fresh 600k*1 + cached 400k*0.1 + out 1000k*3 = 0.6 + 0.04 + 3 = 3.64 USD
        assert_eq!(table.cost("m", &usage).micros, Some(3_640_000));
    }

    #[test]
    fn cost_and_usage_metrics_carry_the_basis() -> Result<(), Box<dyn std::error::Error>> {
        let mut table = PriceTable::new();
        table.set(
            "m",
            Price {
                input: 1_000_000,
                output: 1_000_000,
                cached_input: 0,
            },
        );
        let mut usage = TokenUsage::new(EvidenceBasis::ProviderReported);
        usage.input = Some(1_000_000);
        usage.output = Some(2_000_000);
        let artifact = ArtifactRef::new("bench/providers/latency.json");
        let metric = table
            .cost("m", &usage)
            .metric("m", Some(artifact.clone()))?;
        assert_eq!(metric.unit, Unit::Micros);
        assert_eq!(metric.basis, EvidenceBasis::ProviderReported);
        assert!(metric.is_publishable());
        let tokens = usage_metrics("m", &usage, Some(&artifact))?;
        assert_eq!(tokens.len(), 2);
        assert!(tokens.iter().all(|metric| metric.unit == Unit::Tokens));
        Ok(())
    }

    #[test]
    fn unpriced_cost_is_a_zero_metric() -> Result<(), Box<dyn std::error::Error>> {
        let table = PriceTable::new();
        let usage = TokenUsage::new(EvidenceBasis::Inferred);
        let metric = table.cost("x", &usage).metric("x", None)?;
        assert_eq!(metric.value.to_bits(), 0.0_f64.to_bits());
        assert_eq!(metric.basis, EvidenceBasis::Unpriced);
        Ok(())
    }
}
