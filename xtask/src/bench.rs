//! `gate:bench` (E15-T02, DF5): **nenhum número publicado sem base e artefacto**.
//!
//! Lê `bench/published.toml` — a lista de números que o projeto publica — e falha se algum
//! valor não puder fundamentar uma decisão: base ausente/imprópria, artefacto em falta, artefacto
//! inexistente no repositório, `unpriced` com valor diferente de zero, ou manifesto vazio.
//!
//! O manifesto pode conter linhas `unpriced`/`inferred` (negativos, preços por medir), mas tem de
//! conter **pelo menos um** número publicável: um projeto que só publica o que não mediu não passa.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use katu_core::evidence::{EvidenceBasis, Metric};
use serde::Deserialize;

/// Manifesto de números publicados.
#[derive(Debug, Deserialize)]
struct Published {
    /// Métricas publicadas.
    #[serde(default)]
    metrics: Vec<Metric>,
}

/// Verifica o manifesto de números publicados.
///
/// # Erros
/// Devolve uma mensagem agregada com **todas** as violações encontradas.
pub(crate) fn gate_bench(args: &[String]) -> Result<(), String> {
    let path = args.first().map_or("bench/published.toml", String::as_str);
    let text = fs::read_to_string(path).map_err(|err| format!("lendo {path}: {err}"))?;
    let published: Published =
        toml::from_str(&text).map_err(|err| format!("{path} inválido: {err}"))?;

    let mut violations = Vec::new();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut publishable = 0_usize;

    if published.metrics.is_empty() {
        violations.push("manifesto sem métricas".to_string());
    }

    for metric in &published.metrics {
        let name = label(metric);
        if !seen.insert(metric.name.as_str()) {
            violations.push(format!("{name}: nome duplicado"));
        }
        if metric.basis == EvidenceBasis::Unpriced && metric.value != 0.0 {
            violations.push(format!("{name}: `unpriced` com valor diferente de zero"));
        }
        if metric.basis.is_publishable() && metric.artifact.is_none() {
            violations.push(format!(
                "{name}: base `{}` exige artefacto",
                metric.basis.as_str()
            ));
        }
        if let Some(artifact) = &metric.artifact
            && !Path::new(&artifact.path).exists()
        {
            violations.push(format!("{name}: artefacto inexistente `{}`", artifact.path));
        }
        if metric.is_publishable() {
            publishable = publishable.saturating_add(1);
        }
    }

    if publishable == 0 {
        violations.push("nenhum número publicável (só `inferred`/`unpriced`)".to_string());
    }

    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!("gate:bench falhou:\n  {}", violations.join("\n  ")))
    }
}

/// Rótulo legível da métrica (nome ou marcador), sem alocar.
fn label(metric: &Metric) -> &str {
    if metric.name.is_empty() {
        "<sem nome>"
    } else {
        metric.name.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::gate_bench;

    /// Escreve um ficheiro temporário único e devolve o seu caminho.
    fn write_temp(key: &str, body: &str) -> Result<String, std::io::Error> {
        let path = std::env::temp_dir().join(format!("katu-gate-bench-{key}.tmp"));
        std::fs::write(&path, body)?;
        Ok(path.to_string_lossy().into_owned())
    }

    #[test]
    fn accepts_measured_with_existing_artifact() -> Result<(), Box<dyn std::error::Error>> {
        let artifact = write_temp("artifact", "raw")?;
        let body = format!(
            "[[metrics]]\nname = \"m\"\nvalue = 1.0\nunit = \"count\"\nbasis = \"measured\"\nartifact = {{ path = \"{artifact}\" }}\n"
        );
        let path = write_temp("ok", &body)?;
        assert!(gate_bench(&[path]).is_ok());
        Ok(())
    }

    #[test]
    fn rejects_publishable_without_artifact() -> Result<(), Box<dyn std::error::Error>> {
        let body =
            "[[metrics]]\nname = \"m\"\nvalue = 1.0\nunit = \"count\"\nbasis = \"measured\"\n";
        let path = write_temp("no-artifact", body)?;
        assert!(gate_bench(&[path]).is_err());
        Ok(())
    }

    #[test]
    fn rejects_only_unpriced() -> Result<(), Box<dyn std::error::Error>> {
        let body =
            "[[metrics]]\nname = \"m\"\nvalue = 0.0\nunit = \"count\"\nbasis = \"unpriced\"\n";
        let path = write_temp("only-unpriced", body)?;
        let error = match gate_bench(&[path]) {
            Ok(()) => String::new(),
            Err(err) => err,
        };
        assert!(error.contains("publicável"), "{error}");
        Ok(())
    }

    #[test]
    fn rejects_unpriced_non_zero() -> Result<(), Box<dyn std::error::Error>> {
        let body =
            "[[metrics]]\nname = \"m\"\nvalue = 2.0\nunit = \"count\"\nbasis = \"unpriced\"\n";
        let path = write_temp("unpriced-nonzero", body)?;
        assert!(gate_bench(&[path]).is_err());
        Ok(())
    }
}
