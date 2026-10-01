//! `check-diag` (S-04/W7): nenhum id **prometido** pelo catálogo fica sem emissão.
//!
//! O catálogo [`katu_core::diag::events`] é a fonte única dos identificadores. Q-10 fechou os
//! órfãos de então; este gate impede que voltem: extrai os nomes do módulo `events.rs` e verifica
//! que cada um aparece referido como `events::NOME` em código de produção (o próprio catálogo não
//! conta). Um id sem emissão é um id que mente no *dashboard*.

use std::fs;
use std::path::Path;

use crate::walk::collect_by_extension;

/// Caminho do catálogo de eventos.
const EVENTS: &str = "crates/katu-core/src/diag/events.rs";

/// Verifica que todos os ids do catálogo são emitidos em código de produção.
///
/// # Errors
/// Devolve a lista de ids (nomes de constante) sem qualquer referência `events::NOME`.
pub(crate) fn check_event_orphans() -> Result<(), String> {
    let source = fs::read_to_string(EVENTS).map_err(|err| format!("lendo {EVENTS}: {err}"))?;
    let names = event_names(&source);
    let haystack = production_source()?;
    let orphans: Vec<&str> = names
        .iter()
        .map(String::as_str)
        .filter(|name| !haystack.contains(&format!("events::{name}")))
        .collect();
    if orphans.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "check-diag falhou (ids órfãos; emita-os ou remova-os do catálogo, subindo o teto em PR):\n  {}",
            orphans.join("\n  ")
        ))
    }
}

/// Extrai os nomes de constante dos pares `(NOME, "id", …)` do catálogo.
///
/// Normaliza os espaços (as entradas podem ocupar várias linhas) e só aceita o que segue o formato
/// `( IDENT , "…"`; qualquer outro parêntese (texto de doc) é ignorado.
fn event_names(source: &str) -> Vec<String> {
    let normalized: String = source.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut names = Vec::new();
    for chunk in normalized.split('(').skip(1) {
        let chunk = chunk.trim_start();
        let end = chunk
            .find(|c: char| !(c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'))
            .unwrap_or(0);
        let name = chunk.get(..end).unwrap_or("");
        if name.is_empty() || !name.starts_with(|c: char| c.is_ascii_uppercase()) {
            continue;
        }
        let rest = chunk.get(end..).unwrap_or("").trim_start();
        let Some(rest) = rest.strip_prefix(',') else {
            continue;
        };
        if rest.trim_start().starts_with('"') {
            names.push(name.to_string());
        }
    }
    names
}

/// Concatena o código de produção (`crates/*/src`), sem o próprio catálogo.
fn production_source() -> Result<String, String> {
    let mut files = Vec::new();
    collect_by_extension(Path::new("crates"), "rs", &mut files)?;
    let mut haystack = String::new();
    for file in files {
        let rel = file.to_string_lossy().replace('\\', "/");
        if rel.ends_with("diag/events.rs") {
            continue;
        }
        let source =
            fs::read_to_string(&file).map_err(|err| format!("lendo {}: {err}", file.display()))?;
        haystack.push_str(&source);
        haystack.push('\n');
    }
    Ok(haystack)
}

#[cfg(test)]
mod tests {
    use super::event_names;

    #[test]
    fn extracts_names_from_catalog_entries() {
        let source = r#"
catalog! {
    (KATU_RUN, "katu.run", "Execução de topo."),
    (
        KATU_FN,
        "katu.fn",
        "Span genérico (E19-T03)."
    ),
}
"#;
        let names = event_names(source);
        assert_eq!(names, vec!["KATU_RUN".to_string(), "KATU_FN".to_string()]);
    }

    #[test]
    fn ignores_doc_parentheses() {
        let source = "(POLICY_DENY, \"policy.deny\", \"Negada (E02).\")";
        assert_eq!(event_names(source), vec!["POLICY_DENY".to_string()]);
    }
}
