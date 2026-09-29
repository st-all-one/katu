//! Varredura recursiva determinística, com ignore e limites (E06-T05).

use std::path::{Path, PathBuf};

use katu_core::ports::Fs;

/// Profundidade máxima da recursão.
const MAX_DEPTH: u8 = 16;

/// Nomes ignorados por omissão (além de dotfiles).
const IGNORED: &[&str] = &["target", "node_modules", ".katu", ".venv", "dist", "build"];

/// Recolhe até `max_files` ficheiros sob `root`, em ordem canónica.
pub(super) fn walk(fs: &dyn Fs, root: &Path, max_files: usize) -> Vec<PathBuf> {
    let walker = Walker {
        fs,
        ignores: read_ignores(fs, root),
        max_files,
    };
    let mut files = Vec::new();
    walker.visit(root, 0, &mut files);
    files
}

/// Estado da varredura (evita muitos parâmetros em `visit`).
struct Walker<'a> {
    fs: &'a dyn Fs,
    ignores: Vec<String>,
    max_files: usize,
}

impl Walker<'_> {
    fn visit(&self, dir: &Path, depth: u8, out: &mut Vec<PathBuf>) {
        if depth > MAX_DEPTH || out.len() >= self.max_files {
            return;
        }
        let Ok(entries) = self.fs.list_dir(dir) else {
            return;
        };
        for entry in entries {
            if out.len() >= self.max_files {
                return;
            }
            let name = entry
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            if ignored(name, &self.ignores) {
                continue;
            }
            if self.fs.is_dir(&entry) {
                self.visit(&entry, depth.saturating_add(1), out);
            } else {
                out.push(entry);
            }
        }
    }
}

fn ignored(name: &str, ignores: &[String]) -> bool {
    if name.is_empty() || name.starts_with('.') || IGNORED.contains(&name) {
        return true;
    }
    ignores.iter().any(|pattern| matches_pattern(name, pattern))
}

fn matches_pattern(name: &str, pattern: &str) -> bool {
    pattern
        .strip_prefix('*')
        .map_or(name == pattern, |suffix| name.ends_with(suffix))
}

/// Lê `.gitignore` da raiz (padrões simples; `!`/comentários ignorados).
fn read_ignores(fs: &dyn Fs, root: &Path) -> Vec<String> {
    let Ok(bytes) = fs.read(&root.join(".gitignore")) else {
        return Vec::new();
    };
    String::from_utf8_lossy(&bytes)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('!'))
        .map(|line| {
            line.trim_end_matches('/')
                .trim_start_matches('/')
                .to_string()
        })
        .collect()
}
