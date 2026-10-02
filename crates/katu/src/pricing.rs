//! Preços por modelo (E12-T03) para a transparência de **custo** na borda.
//!
//! Lê `policy/prices.toml` (versionado, **vazio por omissão**): um modelo sem entrada fica
//! `unpriced` e o custo não é mostrado. O katu nunca inventa preço (DF5); preencher o ficheiro é
//! uma decisão explícita do dono.

use std::collections::BTreeMap;

use katu_core::diag::{Level, events};
use katu_providers::{Price, PriceTable};
use serde::Deserialize;

/// Preços versionados (dado, não código).
const PRICES: &str = include_str!("../../../policy/prices.toml");

/// Ficheiro de preços.
#[derive(Debug, Deserialize)]
struct PricesFile {
    #[serde(default)]
    models: BTreeMap<String, PriceSpec>,
}

/// Preço de um modelo no ficheiro.
#[derive(Debug, Deserialize)]
struct PriceSpec {
    input: u64,
    output: u64,
    #[serde(default)]
    cached_input: u64,
}

/// Carrega a tabela de preços do ficheiro versionado.
///
/// # Errors
/// Mensagem se o ficheiro não for um TOML válido.
pub(crate) fn price_table() -> Result<PriceTable, String> {
    let _span = katu_core::fn_span!(Level::Trace, events::POLICY_LOAD, "pricing::price_table");
    let file: PricesFile =
        toml::from_str(PRICES).map_err(|err| format!("policy/prices.toml inválido: {err}"))?;
    let mut table = PriceTable::new();
    for (model, spec) in file.models {
        table.set(
            model,
            Price {
                input: spec.input,
                output: spec.output,
                cached_input: spec.cached_input,
            },
        );
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::price_table;
    use katu_core::evidence::EvidenceBasis;
    use katu_core::provider::TokenUsage;

    /// Uso fixo para os testes de preço.
    fn usage() -> TokenUsage {
        TokenUsage {
            input: Some(1000),
            output: Some(500),
            cached_input: Some(0),
            reasoning: None,
            basis: EvidenceBasis::Measured,
        }
    }

    #[test]
    fn embedded_prices_load() -> Result<(), Box<dyn std::error::Error>> {
        price_table()?;
        Ok(())
    }

    #[test]
    fn local_models_have_zero_cost() -> Result<(), Box<dyn std::error::Error>> {
        let table = price_table()?;

        // Modelos locais devem ter custo zero
        let cost = table.cost("qwen2.5-coder-1.5b", &usage());
        assert_eq!(cost.micros, Some(0), "modelo local deve ter custo zero");
        assert_eq!(cost.basis, EvidenceBasis::Measured);
        Ok(())
    }

    #[test]
    fn remote_models_have_nonzero_cost() -> Result<(), Box<dyn std::error::Error>> {
        let table = price_table()?;

        // GPT-4o deve ter custo não-zero
        let cost = table.cost("gpt-4o", &usage());
        let micros = cost.micros.ok_or("GPT-4o tem de ter preço")?;
        assert!(micros > 0, "GPT-4o deve ter custo não-zero");
        Ok(())
    }

    #[test]
    fn unknown_model_is_unpriced() -> Result<(), Box<dyn std::error::Error>> {
        let table = price_table()?;

        // Modelo desconhecido deve ser unpriced
        let cost = table.cost("modelo-desconhecido-xyz", &usage());
        assert_eq!(cost.micros, None, "modelo desconhecido deve ser unpriced");
        assert_eq!(cost.basis, EvidenceBasis::Unpriced);
        Ok(())
    }
}
