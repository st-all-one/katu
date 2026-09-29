//! Helpers partilhados pelas tools: linguagem e conversões inteiras saturantes.

/// Linguagem a partir da extensão do caminho.
#[must_use]
pub fn language(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("rs") => "rust",
        Some("py") => "python",
        Some("js") => "javascript",
        Some("ts") => "typescript",
        Some("go") => "go",
        Some("toml") => "toml",
        Some("json") => "json",
        Some("md") => "markdown",
        _ => "text",
    }
}

/// `usize` → `u64`, saturante.
#[must_use]
pub fn len_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

/// `u64` → `i64`, saturante.
#[must_use]
pub fn to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}
