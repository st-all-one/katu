//! Gestão dos ficheiros de git do `.katu/` (E20-T19): exclusões e blocos geridos.
//!
//! `audit/`, `trash/` e `log/` ficam **sempre** fora; o resto segue `git.versioned`. Os blocos são
//! delimitados por marcadores, para não pisar conteúdo manual.

use std::path::{Path, PathBuf};

use katu_core::error::Error;

use crate::config;

use super::GitMode;

/// Marcadores do bloco gerido.
const START: &str = "# katu — gerido (início)";
/// Fim do bloco gerido.
const END: &str = "# katu — gerido (fim)";
/// Subárvore sempre fora do git.
const ALWAYS_EXCLUDED: &str = ".katu/audit/\n.katu/trash/\n.katu/log/\n";
/// Atributos git para o conteúdo gerado.
const ATTRIBUTES: &str = ".katu/audit/** -diff\n.katu/log/** -diff\n";

/// Gere os ficheiros de git (só quando o projeto está num repositório).
pub(super) fn ensure(root: &Path, mode: GitMode) -> Result<bool, Error> {
    let Some(git) = find_git(root) else {
        return Ok(false);
    };
    let versioned = match mode {
        GitMode::Excluded => false,
        GitMode::Tracked => true,
        GitMode::Default => versioned(root),
    };
    let mut changed = upsert_block(&root.join(".gitignore"), ALWAYS_EXCLUDED)?;
    changed |= upsert_block(&root.join(".gitattributes"), ATTRIBUTES)?;
    let exclude = git.join("info").join("exclude");
    let tree = if versioned { "" } else { ".katu/\n" };
    changed |= upsert_block(&exclude, tree)?;
    Ok(changed)
}

/// Se o `.katu/` (fora de audit/trash/log) deve ser versionado.
fn versioned(root: &Path) -> bool {
    let mut table = match config::global_path() {
        Ok(path) => config::load(&path).unwrap_or_default(),
        Err(_) => toml::Table::new(),
    };
    if let Ok(project) = config::load(&config::project_path(root)) {
        config::merge(&mut table, &project);
    }
    match config::get_key(&table, "git.versioned") {
        Some(toml::Value::Boolean(value)) => value,
        _ => true,
    }
}

/// Sobe a partir de `root` à procura do diretório `.git`.
fn find_git(root: &Path) -> Option<PathBuf> {
    let mut current = root.to_path_buf();
    loop {
        let git = current.join(".git");
        if git.is_dir() {
            return Some(git);
        }
        match current.parent() {
            Some(parent) if parent != current => current = parent.to_path_buf(),
            _ => return None,
        }
    }
}

/// Substitui (ou remove) o bloco gerido de um ficheiro; devolve `true` se mudou.
fn upsert_block(path: &Path, body: &str) -> Result<bool, Error> {
    let existing = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(Error::io(path.display().to_string(), err)),
    };
    let mut new = strip_block(&existing);
    if !body.is_empty() {
        if !new.is_empty() && !new.ends_with('\n') {
            new.push('\n');
        }
        new.push_str(START);
        new.push('\n');
        new.push_str(body);
        if !body.ends_with('\n') {
            new.push('\n');
        }
        new.push_str(END);
        new.push('\n');
    }
    if new == existing {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| Error::io(parent.display().to_string(), err))?;
    }
    std::fs::write(path, new).map_err(|err| Error::io(path.display().to_string(), err))?;
    Ok(true)
}

/// Remove o bloco gerido (marcadores incluídos) do texto.
fn strip_block(text: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for line in text.lines() {
        if line == START {
            inside = true;
            continue;
        }
        if line == END {
            inside = false;
            continue;
        }
        if !inside {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::strip_block;

    #[test]
    fn strip_removes_only_the_managed_block() {
        let text = "manual\n# katu — gerido (início)\n.katu/\n# katu — gerido (fim)\nfim\n";
        assert_eq!(strip_block(text), "manual\nfim\n");
    }

    #[test]
    fn strip_is_identity_without_block() {
        assert_eq!(strip_block("a\nb\n"), "a\nb\n");
    }
}
