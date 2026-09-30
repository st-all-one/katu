//! Erros de validação que **ensinam** (OA19): `Issue { path, message }` **agregado**, nunca uma
//! `String` solta.
//!
//! O tipo é partilhado: o validador de checkpoint ([`crate::kernel::checkpoint`], E09-T02) e o
//! linter de schema de tools (E06-T02) devolvem a mesma forma, para o consumidor apontar o **campo
//! exato** que falhou e mostrar todos os problemas de uma vez.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Um problema de validação: caminho do campo + mensagem que ensina a corrigir.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    /// Caminho do campo que falhou (ex.: `feature_list[0].status`, `$` para a raiz).
    pub path: String,
    /// Mensagem que ensina o que fazer.
    pub message: String,
}

impl Issue {
    /// Constrói um problema com caminho e mensagem.
    #[must_use]
    pub fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        let _span = crate::trace_fn!("validate::new");

        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for Issue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _span = crate::trace_fn!("validate::fmt");

        write!(f, "{}: {}", self.path, self.message)
    }
}

/// Lista **agregada** de problemas (ordem determinística de produção).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Issues(Vec<Issue>);

impl Issues {
    /// Agrega os problemas (o chamador garante que não está vazio).
    #[must_use]
    pub fn new(issues: Vec<Issue>) -> Self {
        let _span = crate::trace_fn!("validate::new");

        Self(issues)
    }

    /// Problemas, na ordem em que foram encontrados.
    #[must_use]
    pub fn as_slice(&self) -> &[Issue] {
        let _span = crate::trace_fn!("validate::as_slice");

        &self.0
    }

    /// Número de problemas.
    #[must_use]
    pub fn len(&self) -> usize {
        let _span = crate::trace_fn!("validate::len");

        self.0.len()
    }

    /// `true` se não há problemas.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        let _span = crate::trace_fn!("validate::is_empty");

        self.0.is_empty()
    }
}

impl fmt::Display for Issues {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _span = crate::trace_fn!("validate::fmt");

        let mut first = true;
        for issue in &self.0 {
            if !first {
                f.write_str("; ")?;
            }
            write!(f, "{issue}")?;
            first = false;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Issue, Issues};

    #[test]
    fn issue_display_names_the_path() {
        let issue = Issue::new("goal", "campo obrigatório em falta");
        assert_eq!(issue.to_string(), "goal: campo obrigatório em falta");
    }

    #[test]
    fn issues_join_deterministically() {
        let issues = Issues::new(vec![
            Issue::new("a", "primeiro"),
            Issue::new("b", "segundo"),
        ]);
        assert_eq!(issues.len(), 2);
        assert!(!issues.is_empty());
        assert_eq!(issues.to_string(), "a: primeiro; b: segundo");
        assert_eq!(issues.as_slice().len(), 2);
    }

    #[test]
    fn empty_issues_display_nothing() {
        assert!(Issues::new(Vec::new()).is_empty());
        assert_eq!(Issues::new(Vec::new()).to_string(), "");
    }
}
