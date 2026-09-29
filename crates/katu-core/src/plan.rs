//! Artefacto de **plano** tipado (E06-T06): contrato de escopo + lista de features.
//!
//! O plano é validado por schema ([`Plan::validate`]): exige `forbidden_files` **e** `rollback_plan`,
//! uma lista de features não vazia e no máximo **uma** feature `in_progress`. O kernel exige um
//! plano registado para transitar para [`Phase::Planned`](katu_policy::Phase) (E04).

use serde::{Deserialize, Serialize};

/// Estado de uma feature do plano.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FeatureStatus {
    /// Ainda não começou.
    Pending,
    /// Em curso (no máximo uma).
    InProgress,
    /// Concluída.
    Done,
}

impl FeatureStatus {
    /// Nome estável.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Done => "done",
        }
    }
}

/// Uma feature do plano.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Feature {
    /// Identificador curto (ex.: `F1`).
    pub id: String,
    /// Descrição.
    pub description: String,
    /// Estado.
    pub status: FeatureStatus,
}

impl Feature {
    /// Constrói uma feature.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        description: impl Into<String>,
        status: FeatureStatus,
    ) -> Self {
        Self {
            id: id.into(),
            description: description.into(),
            status,
        }
    }
}

/// Contrato de escopo do plano.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopeContract {
    /// Globs permitidos (vazio = tudo o que não for proibido).
    pub allowed_files: Vec<String>,
    /// Globs proibidos (obrigatório não vazio).
    pub forbidden_files: Vec<String>,
    /// Critérios de aceitação.
    pub acceptance_criteria: Vec<String>,
    /// Plano de reversão (obrigatório não vazio).
    pub rollback_plan: String,
}

impl ScopeContract {
    /// Constrói um contrato de escopo.
    #[must_use]
    pub fn new(
        allowed_files: Vec<String>,
        forbidden_files: Vec<String>,
        acceptance_criteria: Vec<String>,
        rollback_plan: impl Into<String>,
    ) -> Self {
        Self {
            allowed_files,
            forbidden_files,
            acceptance_criteria,
            rollback_plan: rollback_plan.into(),
        }
    }

    /// `true` se o caminho é permitido pelo contrato (proibido vence; `allowed` vazio = tudo).
    #[must_use]
    pub fn allows(&self, path: &str) -> bool {
        if self
            .forbidden_files
            .iter()
            .any(|pattern| matches_glob(pattern, path))
        {
            return false;
        }
        self.allowed_files.is_empty()
            || self
                .allowed_files
                .iter()
                .any(|pattern| matches_glob(pattern, path))
    }
}

/// Plano tipado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    /// Contrato de escopo.
    pub scope_contract: ScopeContract,
    /// Lista de features.
    pub feature_list: Vec<Feature>,
}

impl Plan {
    /// Constrói um plano.
    #[must_use]
    pub fn new(scope_contract: ScopeContract, feature_list: Vec<Feature>) -> Self {
        Self {
            scope_contract,
            feature_list,
        }
    }

    /// Valida o plano (schema + invariante "≤ 1 `in_progress`").
    ///
    /// # Errors
    /// Devolve [`PlanError`] na primeira violação encontrada (fail-closed).
    pub fn validate(&self) -> Result<(), PlanError> {
        if self.feature_list.is_empty() {
            return Err(PlanError::EmptyFeatureList);
        }
        if self.scope_contract.forbidden_files.is_empty() {
            return Err(PlanError::MissingForbiddenFiles);
        }
        if self.scope_contract.rollback_plan.trim().is_empty() {
            return Err(PlanError::MissingRollbackPlan);
        }
        let in_progress = self
            .feature_list
            .iter()
            .filter(|feature| feature.status == FeatureStatus::InProgress)
            .count();
        if in_progress > 1 {
            return Err(PlanError::MultipleInProgress);
        }
        Ok(())
    }
}

/// Erro de validação do plano (fail-closed).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PlanError {
    /// Sem features.
    EmptyFeatureList,
    /// Sem ficheiros proibidos.
    MissingForbiddenFiles,
    /// Sem plano de reversão.
    MissingRollbackPlan,
    /// Mais do que uma feature em curso.
    MultipleInProgress,
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyFeatureList => f.write_str("plano sem features"),
            Self::MissingForbiddenFiles => f.write_str("plano sem forbidden_files"),
            Self::MissingRollbackPlan => f.write_str("plano sem rollback_plan"),
            Self::MultipleInProgress => f.write_str("mais de uma feature in_progress"),
        }
    }
}

impl std::error::Error for PlanError {}

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
    use super::{Feature, FeatureStatus, Plan, PlanError, ScopeContract, matches_glob};

    fn contract() -> ScopeContract {
        ScopeContract::new(
            vec!["src/**".to_string()],
            vec!["**/secrets/**".to_string()],
            vec!["testes passam".to_string()],
            "reverter o commit",
        )
    }

    fn plan(features: Vec<Feature>) -> Plan {
        Plan::new(contract(), features)
    }

    fn feature(id: &str, status: FeatureStatus) -> Feature {
        Feature::new(id, "fazer", status)
    }

    #[test]
    fn valid_plan_passes() -> Result<(), PlanError> {
        plan(vec![feature("F1", FeatureStatus::InProgress)]).validate()
    }

    #[test]
    fn empty_feature_list_is_rejected() {
        assert_eq!(
            plan(Vec::new()).validate(),
            Err(PlanError::EmptyFeatureList)
        );
    }

    #[test]
    fn missing_forbidden_files_is_rejected() {
        let mut plan = plan(vec![feature("F1", FeatureStatus::Pending)]);
        plan.scope_contract.forbidden_files.clear();
        assert_eq!(plan.validate(), Err(PlanError::MissingForbiddenFiles));
    }

    #[test]
    fn missing_rollback_is_rejected() {
        let mut plan = plan(vec![feature("F1", FeatureStatus::Pending)]);
        plan.scope_contract.rollback_plan = "   ".to_string();
        assert_eq!(plan.validate(), Err(PlanError::MissingRollbackPlan));
    }

    #[test]
    fn multiple_in_progress_is_rejected() {
        let plan = plan(vec![
            feature("F1", FeatureStatus::InProgress),
            feature("F2", FeatureStatus::InProgress),
        ]);
        assert_eq!(plan.validate(), Err(PlanError::MultipleInProgress));
    }

    #[test]
    fn glob_matches_prefix_and_suffix() {
        assert!(matches_glob("src/**", "src/a/b.rs"));
        assert!(matches_glob("*.md", "README.md"));
        assert!(matches_glob("**/secrets/**", "a/secrets/x"));
        assert!(!matches_glob("src/**", "docs/a.md"));
    }

    #[test]
    fn allows_respects_forbidden_first() {
        let contract = contract();
        assert!(contract.allows("src/main.rs"));
        assert!(!contract.allows("src/secrets/token"));
        assert!(!contract.allows("docs/readme.md"));
    }
}
