//! Caminhos e `argv` resolvidos (E02-T01). A resolução de symlinks é do kernel (via a porta `Fs`).

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::PolicyError;

/// Caminho **absoluto** e canonicamente normalizado (sem `.`/`..`).
///
/// A resolução de symlinks é feita pelo kernel (via a porta `Fs`) **antes** do veredicto; este tipo
/// é o invariante pós-resolução: absoluto e lexicalmente normal. O construtor é a única porta.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ResolvedPath(String);

impl ResolvedPath {
    /// Constrói a partir de um caminho já resolvido (absoluto).
    pub fn from_canonical(path: impl AsRef<Path>) -> Result<Self, PolicyError> {
        let path = path.as_ref();
        if !path.is_absolute() {
            return Err(PolicyError::NonAbsolutePath(path.display().to_string()));
        }
        let normalized = normalize(path);
        Ok(Self(normalized.to_string_lossy().into_owned()))
    }

    /// Representação textual normalizada.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// `true` se `self` é `root` ou está sob `root`.
    #[must_use]
    pub fn is_under(&self, root: &Self) -> bool {
        if root.0 == "/" {
            return self.0.starts_with('/');
        }
        self.0 == root.0 || self.0.starts_with(&format!("{}/", root.0))
    }
}

impl TryFrom<String> for ResolvedPath {
    type Error = PolicyError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_canonical(value)
    }
}

impl From<ResolvedPath> for String {
    fn from(value: ResolvedPath) -> Self {
        value.0
    }
}

/// `argv` resolvido e não vazio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<String>", into = "Vec<String>")]
pub struct ResolvedArgv(Vec<String>);

impl ResolvedArgv {
    /// Constrói a partir de uma lista de argumentos (não vazia).
    pub fn new(argv: Vec<String>) -> Result<Self, PolicyError> {
        if argv.is_empty() {
            return Err(PolicyError::InvalidArgv("lista vazia".to_string()));
        }
        Ok(Self(argv))
    }

    /// Programa (primeiro argumento).
    #[must_use]
    pub fn program(&self) -> &str {
        self.0.first().map_or("", String::as_str)
    }

    /// Argumentos completos.
    #[must_use]
    pub fn as_slice(&self) -> &[String] {
        &self.0
    }
}

impl TryFrom<Vec<String>> for ResolvedArgv {
    type Error = PolicyError;

    fn try_from(value: Vec<String>) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ResolvedArgv> for Vec<String> {
    fn from(value: ResolvedArgv) -> Self {
        value.0
    }
}

/// Normaliza lexicalmente um caminho absoluto (resolve `.`/`..`, sem tocar no SO).
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::from("/");
    for component in path.components() {
        match component {
            Component::RootDir | Component::CurDir | Component::Prefix(_) => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(part) => out.push(part),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{ResolvedArgv, ResolvedPath};
    use crate::error::PolicyError;
    use proptest::collection::vec;
    use proptest::prelude::{prop_assert, proptest};
    use std::path::{Component, Path};

    #[test]
    fn relative_paths_are_rejected() {
        assert!(ResolvedPath::from_canonical("relativo/dir").is_err());
    }

    #[test]
    fn parent_dir_is_resolved() -> Result<(), PolicyError> {
        let resolved = ResolvedPath::from_canonical("/a/b/../c")?;
        assert_eq!(resolved.as_str(), "/a/c");
        Ok(())
    }

    #[test]
    fn under_relation_respects_boundaries() -> Result<(), PolicyError> {
        let root = ResolvedPath::from_canonical("/work")?;
        let inside = ResolvedPath::from_canonical("/work/sub/x")?;
        let sibling = ResolvedPath::from_canonical("/workshop")?;
        assert!(inside.is_under(&root));
        assert!(root.is_under(&root));
        assert!(!sibling.is_under(&root));
        Ok(())
    }

    #[test]
    fn empty_argv_is_rejected() {
        assert!(ResolvedArgv::new(Vec::new()).is_err());
        assert!(ResolvedArgv::new(vec!["ls".to_string()]).is_ok());
    }

    proptest! {
        #[test]
        fn resolved_path_never_keeps_parent_dir(segments in vec("[a-z]{1,8}", 1..6)) {
            let mut raw = String::from("/");
            for segment in &segments {
                raw.push_str(segment);
                raw.push_str("/../");
            }
            let ok = ResolvedPath::from_canonical(&raw).is_ok_and(|resolved| {
                !Path::new(resolved.as_str())
                    .components()
                    .any(|component| matches!(component, Component::ParentDir))
            });
            prop_assert!(ok, "caminho manteve `..`: {raw}");
        }
    }
}
