//! `check-slices` (E14-T07) — `_REF/` é pesquisa externa: fora do build, fora do git e sem links.
//!
//! `_REF/*` é consulta e citação, **nunca** integração nem dependência (§44). Este check recusa que
//! `_REF/` seja versionado, que um crate dele dependa, ou que o `plan/` o ligue por caminho.

use std::fs;
use std::path::{Path, PathBuf};

use crate::check_policy::git_ignored;
use crate::markdown_links;
use crate::walk::{collect_by_extension, is_foreign_root};

/// Diretório de pesquisa externa (não versionado).
const REF_DIR: &str = "_REF";

/// Ponto de entrada de `check-slices`.
pub(crate) fn check_slices() -> Result<(), String> {
    let mut problems: Vec<String> = Vec::new();
    if !git_ignored(&format!("{REF_DIR}/x")) {
        problems.push(format!("`{REF_DIR}/` deve estar no `.gitignore`"));
    }
    for manifest in manifests()? {
        let text = fs::read_to_string(&manifest)
            .map_err(|err| format!("lendo {}: {err}", manifest.display()))?;
        if text.contains(REF_DIR) {
            problems.push(format!("{}: depende de `{REF_DIR}/`", manifest.display()));
        }
    }
    let mut plans: Vec<PathBuf> = Vec::new();
    collect_by_extension(Path::new("plan"), "md", &mut plans)?;
    for plan in &plans {
        let text =
            fs::read_to_string(plan).map_err(|err| format!("lendo {}: {err}", plan.display()))?;
        for link in markdown_links(&text) {
            if link.contains(REF_DIR) {
                problems.push(format!(
                    "{}: liga a `{REF_DIR}/` por caminho",
                    plan.display()
                ));
            }
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "check-slices falhou (E14-T07):\n  {}",
            problems.join("\n  ")
        ))
    }
}

/// Manifestos do workspace (crates puros + `xtask`).
fn manifests() -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir("crates").map_err(|err| format!("lendo crates: {err}"))?;
    let mut manifests: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|err| format!("lendo entrada: {err}"))?;
        let path = entry.path();
        if path.is_dir() && !is_foreign_root(&path) {
            let manifest = path.join("Cargo.toml");
            if manifest.is_file() {
                manifests.push(manifest);
            }
        }
    }
    let xtask = PathBuf::from("xtask/Cargo.toml");
    if xtask.is_file() {
        manifests.push(xtask);
    }
    manifests.sort();
    Ok(manifests)
}
