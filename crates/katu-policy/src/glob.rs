//! Correspondência glob determinística (vocabulário v3, E07-T05).
//!
//! `*` = qualquer sequência, `?` = um caractere. Sem regex, sem I/O, sem backtracking sobre
//! entradas não confiáveis. É a **única** semântica de glob do projeto: o contrato de escopo do
//! plano (`katu-core::plan`) reexporta-a para não haver duas.

/// Correspondência glob simples: `*` = qualquer sequência, `?` = um caractere.
#[must_use]
pub fn matches_glob(pattern: &str, path: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let path: Vec<char> = path.chars().collect();
    glob(&pattern, &path)
}

fn glob(pattern: &[char], path: &[char]) -> bool {
    let Some(&head) = pattern.first() else {
        return path.is_empty();
    };
    let rest = pattern.get(1..).unwrap_or_default();
    match head {
        '*' => (0..=path.len()).any(|skip| glob(rest, path.get(skip..).unwrap_or_default())),
        '?' => !path.is_empty() && glob(rest, path.get(1..).unwrap_or_default()),
        other => path.first() == Some(&other) && glob(rest, path.get(1..).unwrap_or_default()),
    }
}

#[cfg(test)]
mod tests {
    use super::matches_glob;

    #[test]
    fn matches_prefix_and_suffix() {
        assert!(matches_glob("src/**", "src/a/b.rs"));
        assert!(matches_glob("*.md", "README.md"));
        assert!(matches_glob("**/secrets/**", "a/secrets/x"));
        assert!(!matches_glob("src/**", "docs/a.md"));
    }

    #[test]
    fn matches_sensitive_component_names() {
        assert!(matches_glob(".env", ".env"));
        assert!(matches_glob(".env*", ".env.local"));
        assert!(matches_glob("id_rsa*", "id_rsa.pub"));
        assert!(!matches_glob(".env", "environment"));
    }
}
