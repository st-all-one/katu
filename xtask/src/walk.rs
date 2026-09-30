//! Percurso de ficheiros partilhado pelos checks do `xtask`.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

/// `true` se `dir` é a raiz de **outro** projeto (submódulo/workspace aninhado).
///
/// Esses diretórios têm o seu próprio `.git` e resolvem-se sozinhos (ex.: `crates/knudge`, que
/// está sob `crates/` apenas por organização). Os checks do katu **não** descem para lá.
pub(crate) fn is_foreign_root(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// Recolhe recursivamente ficheiros com a extensão dada sob `dir`.
///
/// Não desce em raízes de outros projetos ([`is_foreign_root`]).
pub(crate) fn collect_by_extension(
    dir: &Path,
    extension: &str,
    out: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|err| format!("lendo {}: {err}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("lendo entrada: {err}"))?;
        let path = entry.path();
        if path.is_dir() {
            if is_foreign_root(&path) {
                continue;
            }
            collect_by_extension(&path, extension, out)?;
        } else if path.extension().and_then(OsStr::to_str) == Some(extension) {
            out.push(path);
        }
    }
    Ok(())
}

/// Recolhe `*.toml` que declaram regras (`[[rules]]`), ignorando dados de política (ex.:
/// `policy/tiers.toml`, E12-T03).
pub(crate) fn collect_rule_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut files: Vec<PathBuf> = Vec::new();
    collect_by_extension(dir, "toml", &mut files)?;
    for file in files {
        let text =
            fs::read_to_string(&file).map_err(|err| format!("lendo {}: {err}", file.display()))?;
        if text.contains("[[rules]]") {
            out.push(file);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{collect_by_extension, is_foreign_root};
    use std::fs;
    use std::path::PathBuf;

    /// Diretório temporário único; devolve o caminho.
    fn scratch(key: &str) -> Result<PathBuf, std::io::Error> {
        let path = std::env::temp_dir().join(format!("katu-walk-{key}"));
        drop(fs::remove_dir_all(&path));
        fs::create_dir_all(&path)?;
        Ok(path)
    }

    #[test]
    fn detects_nested_project_roots() -> Result<(), std::io::Error> {
        let root = scratch("foreign")?;
        let nested = root.join("nested");
        fs::create_dir_all(&nested)?;
        assert!(!is_foreign_root(&root));
        fs::write(nested.join(".git"), "gitdir: ../../.git/modules/x")?;
        assert!(is_foreign_root(&nested));
        fs::remove_dir_all(&root)?;
        Ok(())
    }

    #[test]
    fn collection_skips_nested_projects() -> Result<(), std::io::Error> {
        let root = scratch("collect")?;
        let nested = root.join("nested");
        fs::create_dir_all(nested.join("src"))?;
        fs::write(root.join("own.rs"), "")?;
        fs::write(nested.join(".git"), "gitdir: ../../.git/modules/x")?;
        fs::write(nested.join("src/other.rs"), "")?;
        let mut found = Vec::new();
        collect_by_extension(&root, "rs", &mut found).map_err(std::io::Error::other)?;
        assert_eq!(found, vec![root.join("own.rs")]);
        fs::remove_dir_all(&root)?;
        Ok(())
    }
}
