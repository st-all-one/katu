//! Preços por modelo (E12-T03) para a transparência de **custo** na borda.
//!
//! Lê `policy/prices.toml` (versionado, **vazio por omissão**): um modelo sem entrada fica
//! `unpriced` e o custo não é mostrado. O katu nunca inventa preço (DF5); preencher o ficheiro é
//! uma decisão explícita do dono.

use std::collections::BTreeMap;

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

    #[test]
    fn embedded_prices_load() {
        let table = price_table();
        assert!(table.is_ok(), "policy/prices.toml tem de ser TOML válido");
    }
}
