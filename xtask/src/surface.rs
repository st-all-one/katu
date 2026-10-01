#![allow(
    clippy::print_stdout,
    reason = "xtask dev-only: avisos do teto de superfície"
)]
//! `check-surface` (E14-T05) — teto de superfície versionado.
//!
//! Conta a superfície real (crates, tools, regras, ADRs, eventos de diag, subcomandos do `xtask` e
//! artefactos de *benchmark*) e compara com `surface.toml`: **avisa** quando uma dimensão atinge o
//! teto e **falha** quando o excede. Subir um teto é uma decisão em PR — a superfície cresce de
//! forma deliberada, nunca por acidente (a lição do `arag` §20 / `maxima` §51.13).

use std::fs;
use std::path::{Path, PathBuf};

use katu_core::diag::events;
use katu_policy::RuleSet;
use katu_tools::schema::SCHEMAS;
use serde::Deserialize;

use crate::docs::ADR_DIR;
use crate::walk::{collect_by_extension, collect_rule_files, is_foreign_root};

/// Ficheiro do teto de superfície (versionado).
const SURFACE_FILE: &str = "surface.toml";

/// Teto de superfície.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Surface {
    crates: usize,
    tools: usize,
    rules: usize,
    adrs: usize,
    diag_events: usize,
    xtask_commands: usize,
    bench_artifacts: usize,
}

impl Surface {
    /// Dimensões com o respetivo teto.
    fn dimensions(&self) -> [(&'static str, usize); 7] {
        [
            ("crates", self.crates),
            ("tools", self.tools),
            ("rules", self.rules),
            ("adrs", self.adrs),
            ("diag_events", self.diag_events),
            ("xtask_commands", self.xtask_commands),
            ("bench_artifacts", self.bench_artifacts),
        ]
    }
}

/// Superfície real contada.
#[derive(Debug, Clone, Copy)]
struct Counted {
    crates: usize,
    tools: usize,
    rules: usize,
    adrs: usize,
    diag_events: usize,
    xtask_commands: usize,
    bench_artifacts: usize,
}

impl Counted {
    /// Valor de uma dimensão (0 se desconhecida).
    fn value(self, dimension: &str) -> usize {
        match dimension {
            "crates" => self.crates,
            "tools" => self.tools,
            "rules" => self.rules,
            "adrs" => self.adrs,
            "diag_events" => self.diag_events,
            "xtask_commands" => self.xtask_commands,
            "bench_artifacts" => self.bench_artifacts,
            _ => 0,
        }
    }
}

/// Ponto de entrada de `check-surface`.
pub(crate) fn check_surface(args: &[String]) -> Result<(), String> {
    let path = args
        .first()
        .map_or_else(|| PathBuf::from(SURFACE_FILE), PathBuf::from);
    let text =
        fs::read_to_string(&path).map_err(|err| format!("lendo {}: {err}", path.display()))?;
    let surface: Surface =
        toml::from_str(&text).map_err(|err| format!("{}: toml inválido: {err}", path.display()))?;
    let counted = count_all()?;
    let (failures, warnings) = compare(&surface, &counted);
    for warning in &warnings {
        println!("{warning}");
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "check-surface falhou:\n  {}",
            failures.join("\n  ")
        ))
    }
}

/// Conta a superfície real.
fn count_all() -> Result<Counted, String> {
    Ok(Counted {
        crates: count_crates()?,
        tools: SCHEMAS.len(),
        rules: count_rules()?,
        adrs: count_adrs()?,
        diag_events: events::ALL.len(),
        xtask_commands: count_xtask_commands()?,
        bench_artifacts: count_bench_artifacts()?,
    })
}

/// Compara o real com o teto: `(falhas, avisos)`.
fn compare(surface: &Surface, counted: &Counted) -> (Vec<String>, Vec<String>) {
    let mut failures = Vec::new();
    let mut warnings = Vec::new();
    for (dimension, ceiling) in surface.dimensions() {
        let actual = counted.value(dimension);
        if actual > ceiling {
            failures.push(format!(
                "{dimension}: {actual} > teto {ceiling} (suba o teto em `{SURFACE_FILE}` no PR)"
            ));
        } else if ceiling > 0 && actual == ceiling {
            warnings.push(format!(
                "AVISO {dimension}: no teto {ceiling} (suba o teto em PR para crescer)"
            ));
        }
    }
    (failures, warnings)
}

/// Crates do workspace (ignora raízes de outros projetos, ex.: o submódulo `knudge`).
fn count_crates() -> Result<usize, String> {
    let entries = fs::read_dir("crates").map_err(|err| format!("lendo crates: {err}"))?;
    let mut count: usize = 0;
    for entry in entries {
        let entry = entry.map_err(|err| format!("lendo entrada: {err}"))?;
        let path = entry.path();
        if path.is_dir() && !is_foreign_root(&path) && path.join("Cargo.toml").is_file() {
            count = count.saturating_add(1);
        }
    }
    Ok(count)
}

/// Regras declaradas em `policy/*.toml`.
fn count_rules() -> Result<usize, String> {
    let mut files: Vec<PathBuf> = Vec::new();
    collect_rule_files(Path::new("policy"), &mut files)?;
    let mut count: usize = 0;
    for file in &files {
        let text =
            fs::read_to_string(file).map_err(|err| format!("lendo {}: {err}", file.display()))?;
        let rules =
            RuleSet::from_toml(&text).map_err(|err| format!("{}: {err}", file.display()))?;
        count = count.saturating_add(rules.rules.len());
    }
    Ok(count)
}

/// ADRs versionadas em `docs/adr/NNNN-*.md`.
fn count_adrs() -> Result<usize, String> {
    let entries = fs::read_dir(ADR_DIR).map_err(|err| format!("lendo {ADR_DIR}: {err}"))?;
    let mut count: usize = 0;
    for entry in entries {
        let entry = entry.map_err(|err| format!("lendo entrada: {err}"))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let numbered = name
            .get(0..4)
            .is_some_and(|prefix| prefix.bytes().all(|byte| byte.is_ascii_digit()));
        let is_md = Path::new(name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("md"));
        if numbered && is_md {
            count = count.saturating_add(1);
        }
    }
    Ok(count)
}

/// Subcomandos do `xtask` (proxy do número de gates).
fn count_xtask_commands() -> Result<usize, String> {
    let text = fs::read_to_string("xtask/src/main.rs")
        .map_err(|err| format!("lendo xtask/src/main.rs: {err}"))?;
    Ok(text.matches("Some(\"").count())
}

/// Artefactos crus de *benchmark* (`bench/**/*.json`).
fn count_bench_artifacts() -> Result<usize, String> {
    let mut files: Vec<PathBuf> = Vec::new();
    collect_by_extension(Path::new("bench"), "json", &mut files)?;
    Ok(files.len())
}

#[cfg(test)]
mod tests {
    use super::{Counted, Surface, compare};

    /// Teto com todas as dimensões a `ceiling`.
    fn surface(ceiling: usize) -> Surface {
        Surface {
            crates: ceiling,
            tools: ceiling,
            rules: ceiling,
            adrs: ceiling,
            diag_events: ceiling,
            xtask_commands: ceiling,
            bench_artifacts: ceiling,
        }
    }

    /// Contagem com todas as dimensões a `actual`.
    fn counted(actual: usize) -> Counted {
        Counted {
            crates: actual,
            tools: actual,
            rules: actual,
            adrs: actual,
            diag_events: actual,
            xtask_commands: actual,
            bench_artifacts: actual,
        }
    }

    #[test]
    fn exceeding_a_ceiling_fails() {
        let (failures, warnings) = compare(&surface(1), &counted(2));
        assert_eq!(failures.len(), 7);
        assert!(warnings.is_empty());
    }

    #[test]
    fn at_the_ceiling_warns_but_passes() {
        let (failures, warnings) = compare(&surface(3), &counted(3));
        assert!(failures.is_empty());
        assert_eq!(warnings.len(), 7);
    }

    #[test]
    fn below_the_ceiling_is_silent() {
        let (failures, warnings) = compare(&surface(100), &counted(1));
        assert!(failures.is_empty());
        assert!(warnings.is_empty());
    }
}
