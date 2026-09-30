//! Gate de substituibilidade da memória (E03-T06): o knudge só acopla em `katu/src/memory/`.
//!
//! Responde à pergunta do §13 — "se amanhã voltarmos ao MCP, quantos ficheiros mudam?" — de forma
//! estática e determinística: o vocabulário do knudge não pode aparecer fora do adaptador, a
//! dependência tem de ser opcional e nenhum crate de núcleo a pode declarar.

use std::fs;
use std::path::Path;

use crate::walk::collect_by_extension;

/// Crates do núcleo que **jamais** podem conhecer o knudge (DF6 + firewall de camadas).
const FIREWALLED: &[&str] = &[
    "katu-core",
    "katu-policy",
    "katu-tools",
    "katu-providers",
    "katu-tui",
];

/// Vocabulários que denunciam acoplamento ao knudge.
const EN_ACOPLAMENTO: &[&str] = &["knudge_core", "knudge-core"];

/// Verifica que trocar o adaptador de memória muda **só** `katu/src/memory/`.
pub(crate) fn check_memory_swap() -> Result<(), String> {
    let mut violations: Vec<String> = Vec::new();
    check_optional_dep(&mut violations)?;
    check_firewall(&mut violations)?;
    check_confined(&mut violations)?;
    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "check-memory-swap falhou:\n  {}",
            violations.join("\n  ")
        ))
    }
}

/// O binário declara `knudge-core` como dependência opcional atrás de `memory-in-process`.
fn check_optional_dep(violations: &mut Vec<String>) -> Result<(), String> {
    let path = Path::new("crates/katu/Cargo.toml");
    let manifest =
        fs::read_to_string(path).map_err(|err| format!("lendo {}: {err}", path.display()))?;
    if !manifest.contains("memory-in-process = [\"dep:knudge-core\"]") {
        violations.push("feature `memory-in-process` deve ligar `dep:knudge-core`".to_string());
    }
    let optional = manifest.lines().any(|line| {
        line.trim_start().starts_with("knudge-core") && line.contains("optional = true")
    });
    if !optional {
        violations.push("`knudge-core` deve ser dependência opcional (feature-gated)".to_string());
    }
    Ok(())
}

/// Nenhum crate de núcleo pode depender do knudge.
fn check_firewall(violations: &mut Vec<String>) -> Result<(), String> {
    for krate in FIREWALLED.iter().copied() {
        let path = Path::new("crates").join(krate).join("Cargo.toml");
        let manifest =
            fs::read_to_string(&path).map_err(|err| format!("lendo {}: {err}", path.display()))?;
        if manifest
            .lines()
            .any(|line| line.trim_start().starts_with("knudge-core"))
        {
            violations.push(format!("{krate} não pode depender de `knudge-core`"));
        }
    }
    Ok(())
}

/// O vocabulário do knudge só aparece sob `katu/src/memory/` e o módulo é *feature-gated*.
fn check_confined(violations: &mut Vec<String>) -> Result<(), String> {
    let main = Path::new("crates/katu/src/main.rs");
    let source =
        fs::read_to_string(main).map_err(|err| format!("lendo {}: {err}", main.display()))?;
    if !source
        .lines()
        .any(|line| line.trim_start() == "#[cfg(feature = \"memory-in-process\")]")
    {
        violations.push(
            "`mod memory` deve estar sob `#[cfg(feature = \"memory-in-process\")]`".to_string(),
        );
    }
    let root = Path::new("crates/katu/src");
    let mut files = Vec::new();
    collect_by_extension(root, "rs", &mut files)?;
    for file in files {
        let relative = file.strip_prefix(root).map_err(|err| err.to_string())?;
        if relative.starts_with("memory") {
            continue;
        }
        let text =
            fs::read_to_string(&file).map_err(|err| format!("lendo {}: {err}", file.display()))?;
        if EN_ACOPLAMENTO
            .iter()
            .copied()
            .any(|needle| text.contains(needle))
        {
            violations.push(format!(
                "{}: acopla ao knudge fora do adaptador",
                file.display()
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::EN_ACOPLAMENTO;

    #[test]
    fn detects_both_spellings() {
        let has = |source: &str| {
            EN_ACOPLAMENTO
                .iter()
                .copied()
                .any(|needle| source.contains(needle))
        };
        assert!(has("use knudge_core::Knudge;"));
        assert!(has("knudge-core = { path }"));
        assert!(!has("use katu_core::memory;"));
    }
}
