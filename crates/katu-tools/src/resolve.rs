//! Resolução canónica de caminhos **antes** do veredicto (E07-T02).
//!
//! A política nunca vê um caminho relativo nem um symlink por resolver: o chamador canonicaliza
//! via a porta [`Fs`] e só depois constrói o [`ToolUse`](katu_policy::ToolUse). Este módulo expõe
//! o ponto único dessa resolução (o `..` lexical sozinho não chega — um link dentro do workspace
//! pode apontar para fora).

use std::fmt;
use std::path::Path;

use katu_core::ports::{Fs, FsError};
use katu_policy::{PolicyError, ResolvedPath};

/// Falha ao resolver um caminho para o invariante pós-resolução.
#[derive(Debug)]
#[non_exhaustive]
pub enum ResolveError {
    /// Falha de I/O/canonicalização (inclui ciclos de symlink).
    Fs(FsError),
    /// O resultado não é um caminho absoluto normalizável.
    Policy(PolicyError),
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fs(err) => write!(f, "resolução de caminho: {err}"),
            Self::Policy(err) => write!(f, "caminho resolvido inválido: {err}"),
        }
    }
}

impl std::error::Error for ResolveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Fs(err) => Some(err),
            Self::Policy(err) => Some(err),
        }
    }
}

impl From<FsError> for ResolveError {
    fn from(value: FsError) -> Self {
        Self::Fs(value)
    }
}

impl From<PolicyError> for ResolveError {
    fn from(value: PolicyError) -> Self {
        Self::Policy(value)
    }
}

/// Resolve `path` seguindo **symlinks** (via porta [`Fs`]) e devolve o invariante canónico.
///
/// # Errors
/// [`ResolveError`] se a canonicalização falhar (`NotFound`/ciclo) ou o resultado não for absoluto.
pub fn resolve(fs: &dyn Fs, path: &Path) -> Result<ResolvedPath, ResolveError> {
    let canonical = fs.canonicalize(path)?;
    Ok(ResolvedPath::from_canonical(canonical)?)
}

#[cfg(test)]
mod tests {
    use super::resolve;
    use katu_core::ports::{Fs, MemFs};
    use std::path::Path;

    #[test]
    fn resolves_symlinks_via_the_port() -> Result<(), Box<dyn std::error::Error>> {
        let fs = MemFs::new();
        fs.write_atomic(Path::new("/work/src/main.rs"), b"ok")?;
        fs.write_atomic(Path::new("/etc/passwd"), b"secret")?;
        fs.symlink(Path::new("/work/escape"), Path::new("/etc/passwd"));

        let resolved = resolve(&fs, Path::new("/work/escape"))?;
        assert_eq!(resolved.as_str(), "/etc/passwd");
        let workspace = katu_policy::ResolvedPath::from_canonical("/work")?;
        assert!(!resolved.is_under(&workspace), "o link escapa ao workspace");
        Ok(())
    }

    #[test]
    fn missing_leaf_resolves_its_parent() -> Result<(), Box<dyn std::error::Error>> {
        let fs = MemFs::new();
        fs.write_atomic(Path::new("/work/src/main.rs"), b"ok")?;
        let resolved = resolve(&fs, Path::new("/work/src/new.rs"))?;
        assert_eq!(resolved.as_str(), "/work/src/new.rs");
        Ok(())
    }

    #[test]
    fn symlink_cycle_is_fail_closed() {
        let fs = MemFs::new();
        fs.symlink(Path::new("/a"), Path::new("/b"));
        fs.symlink(Path::new("/b"), Path::new("/a"));
        assert!(resolve(&fs, Path::new("/a")).is_err());
    }
}
