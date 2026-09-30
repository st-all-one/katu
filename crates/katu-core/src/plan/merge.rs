//! Merge de contratos de escopo por **menor privilégio** (E09-T04).
//!
//! Regras: `allowed` = interseção (cobertura glob-aware); `forbidden` = união;
//! `time_budget_minutes` = mínimo (`None` = sem teto); `network_egress` = `AND`;
//! `acceptance_criteria` = união; `rollback_plan` = o primeiro não vazio.
//!
//! Se ambos os lados têm `allowed` não vazio e a interseção é vazia, o merge **falha**
//! (fail-closed) em vez de conceder "tudo" — a representação de `allowed` vazio significa
//! "tudo o que não for proibido".

use super::{ScopeContract, matches_glob};
use crate::diag::{Level, events};

/// Erro de merge (fail-closed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MergeError {
    /// `allowed` não vazio em ambos os lados e a interseção é vazia.
    EmptyAllowedScope,
}

impl std::fmt::Display for MergeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _span = crate::trace_fn!("plan::merge::fmt");

        match self {
            Self::EmptyAllowedScope => {
                f.write_str("merge de escopos sem `allowed` em comum (fail-closed)")
            }
        }
    }
}

impl std::error::Error for MergeError {}

impl ScopeContract {
    /// Merge por menor privilégio (E09-T04).
    ///
    /// # Errors
    /// [`MergeError::EmptyAllowedScope`] se ambos os `allowed_files` são não vazios e disjuntos.
    pub fn merge(&self, other: &Self) -> Result<Self, MergeError> {
        let _span = crate::span!(
            Level::Debug,
            events::SCOPE_MERGE,
            "allowed_a" => self.allowed_files.len(),
            "allowed_b" => other.allowed_files.len(),
        );
        let allowed = intersect_allowed(&self.allowed_files, &other.allowed_files);
        if allowed.is_empty() && !self.allowed_files.is_empty() && !other.allowed_files.is_empty() {
            return Err(MergeError::EmptyAllowedScope);
        }
        Ok(Self {
            allowed_files: allowed,
            forbidden_files: union(&self.forbidden_files, &other.forbidden_files),
            acceptance_criteria: union(&self.acceptance_criteria, &other.acceptance_criteria),
            rollback_plan: if self.rollback_plan.trim().is_empty() {
                other.rollback_plan.clone()
            } else {
                self.rollback_plan.clone()
            },
            time_budget_minutes: min_budget(self.time_budget_minutes, other.time_budget_minutes),
            network_egress: self.network_egress && other.network_egress,
        })
    }
}

/// Interseção de listas de globs: quando um lado é vazio ("tudo"), vence o outro; senão mantém
/// os padrões cobertos pelo outro lado (ou iguais). Conservador: nunca alarga o escopo.
fn intersect_allowed(a: &[String], b: &[String]) -> Vec<String> {
    let _span = crate::trace_fn!("plan::merge::intersect_allowed");

    if a.is_empty() {
        return b.to_vec();
    }
    if b.is_empty() {
        return a.to_vec();
    }
    let mut out: Vec<String> = Vec::new();
    for pattern in a {
        if b.iter().any(|other| glob_covers(other, pattern)) {
            push_unique(&mut out, pattern);
        }
    }
    for pattern in b {
        if a.iter().any(|other| glob_covers(other, pattern)) {
            push_unique(&mut out, pattern);
        }
    }
    out
}

/// União determinística (ordem: `a` depois `b`, sem duplicados).
fn union(a: &[String], b: &[String]) -> Vec<String> {
    let _span = crate::trace_fn!("plan::merge::union");

    let mut out: Vec<String> = Vec::new();
    for item in a.iter().chain(b) {
        push_unique(&mut out, item);
    }
    out
}

/// Acrescenta `item` se ainda não existir.
fn push_unique(out: &mut Vec<String>, item: &str) {
    let _span = crate::trace_fn!("plan::merge::push_unique");

    if !out.iter().any(|existing| existing == item) {
        out.push(item.to_string());
    }
}

/// Mínimo de dois orçamentos (`None` = sem teto).
fn min_budget(a: Option<u64>, b: Option<u64>) -> Option<u64> {
    let _span = crate::trace_fn!("plan::merge::min_budget");

    match (a, b) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    }
}

/// `true` se todo o caminho que casa com `b` também casa com `a` (**conservador**).
///
/// Prova o narrowing comum (`src/**` cobre `src/parser/**`; `src/*.rs` cobre `src/main.rs`) e
/// devolve `false` quando não consegue provar inclusão — a consequência é um escopo mais
/// restrito, nunca mais permissivo.
fn glob_covers(a: &str, b: &str) -> bool {
    let _span = crate::trace_fn!("plan::merge::glob_covers");

    if a == b {
        return true;
    }
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let mut index = 0_usize;
    while index < a_chars.len() && index < b_chars.len() {
        let left = a_chars.get(index).copied().unwrap_or('\0');
        let right = b_chars.get(index).copied().unwrap_or('\0');
        if is_wildcard(left) || is_wildcard(right) {
            break;
        }
        if left != right {
            return false;
        }
        index = index.saturating_add(1);
    }
    let a_rest: String = a_chars.get(index..).unwrap_or_default().iter().collect();
    let b_rest: String = b_chars.get(index..).unwrap_or_default().iter().collect();
    if a_rest.is_empty() {
        return b_rest.is_empty();
    }
    if a_rest.chars().all(|c| c == '*') {
        return true;
    }
    if !b_rest.chars().any(is_wildcard) {
        return matches_glob(&a_rest, &b_rest);
    }
    false
}

/// `true` para `*` ou `?`.
fn is_wildcard(c: char) -> bool {
    let _span = crate::trace_fn!("plan::merge::is_wildcard");

    c == '*' || c == '?'
}

#[cfg(test)]
mod tests {
    use super::{MergeError, ScopeContract, glob_covers};
    use crate::plan::{Feature, FeatureStatus, Plan, PlanError};

    fn contract(allowed: &[&str], forbidden: &[&str]) -> ScopeContract {
        ScopeContract::new(
            allowed.iter().map(|s| (*s).to_string()).collect(),
            forbidden.iter().map(|s| (*s).to_string()).collect(),
            vec!["testes passam".to_string()],
            "reverter",
        )
    }

    #[test]
    fn covers_proves_the_common_narrowing() {
        assert!(glob_covers("src/**", "src/parser/**"));
        assert!(glob_covers("src/*.rs", "src/main.rs"));
        assert!(glob_covers("**", "src/anything/**"));
        assert!(glob_covers("**/secrets/**", "**/secrets/**"));
        assert!(!glob_covers("src/parser/**", "src/**"));
        assert!(!glob_covers("src/", "src/parser/**"));
    }

    #[test]
    fn merge_narrows_allowed_and_unions_forbidden() -> Result<(), MergeError> {
        let parent = contract(&["src/**"], &["**/secrets/**"]).with_time_budget(30);
        let child = contract(&["src/parser/**"], &["**/generated/**"]).with_time_budget(10);
        let merged = parent.merge(&child)?;
        assert_eq!(merged.allowed_files, vec!["src/parser/**".to_string()]);
        assert_eq!(
            merged.forbidden_files,
            vec!["**/secrets/**".to_string(), "**/generated/**".to_string()]
        );
        assert_eq!(merged.time_budget_minutes, Some(10));
        assert!(!merged.network_egress);
        Ok(())
    }

    #[test]
    fn merge_takes_the_minimum_budget_and_network_and() -> Result<(), MergeError> {
        let mut a = contract(&[], &["a/**"]).with_time_budget(60);
        a.network_egress = true;
        let b = contract(&[], &["b/**"]);
        let merged = a.merge(&b)?;
        assert_eq!(merged.time_budget_minutes, Some(60));
        assert!(!merged.network_egress);
        assert_eq!(
            merged.forbidden_files,
            vec!["a/**".to_string(), "b/**".to_string()]
        );
        Ok(())
    }

    #[test]
    fn disjoint_allowed_scopes_fail_closed() {
        let a = contract(&["src/**"], &["x/**"]);
        let b = contract(&["docs/**"], &["y/**"]);
        assert_eq!(a.merge(&b), Err(MergeError::EmptyAllowedScope));
    }

    #[test]
    fn merged_contract_is_valid() -> Result<(), Box<dyn std::error::Error>> {
        let parent = contract(&["src/**"], &["**/secrets/**"]);
        let child = contract(&["src/parser/**"], &["**/generated/**"]);
        let merged = parent.merge(&child)?;
        let plan = Plan::new(
            merged,
            vec![Feature::new("F1", "fazer", FeatureStatus::Pending)],
        );
        assert_eq!(plan.validate(), Ok(()));
        Ok(())
    }

    #[test]
    fn non_relative_globs_are_rejected() {
        let plan = Plan::new(
            contract(&["/etc/**"], &["x/**"]),
            vec![Feature::new("F1", "fazer", FeatureStatus::Pending)],
        );
        assert!(matches!(
            plan.validate(),
            Err(PlanError::NonRelativeGlob { .. })
        ));
    }
}
