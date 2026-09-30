//! Contabilização de tokens/custo (E12-T03), com base de evidência (DF5).
//!
//! Não inventa preço: um modelo sem entrada na [`PriceTable`] devolve [`EvidenceBasis::Unpriced`]
//! com `micros = None`. A aritmética é inteira (micro-USD) e determinística.

use std::collections::BTreeMap;

use katu_core::evidence::EvidenceBasis;
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

/// Tabela de preços por modelo (vazia por omissão → `unpriced`).
#[derive(Debug, Default, Clone)]
pub struct PriceTable {
    prices: BTreeMap<String, Price>,
}

impl PriceTable {
    /// Tabela vazia.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Regista/atualiza o preço de um modelo.
    pub fn set(&mut self, model: impl Into<String>, price: Price) {
        self.prices.insert(model.into(), price);
    }

    /// Calcula o custo; `unpriced` se o modelo não tiver preço.
    #[must_use]
    pub fn cost(&self, model: &str, usage: &TokenUsage) -> Cost {
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
    use super::{Price, PriceTable};
    use katu_core::evidence::EvidenceBasis;
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
}
