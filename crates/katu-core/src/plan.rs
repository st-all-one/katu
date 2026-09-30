//! Artefacto de **plano** tipado (E06-T06): contrato de escopo + lista de features.
//!
//! O plano é validado por schema ([`Plan::validate`]): exige `forbidden_files` **e** `rollback_plan`,
//! uma lista de features não vazia e no máximo **uma** feature `in_progress`. O kernel exige um
//! plano registado para transitar para [`Phase::Planned`](katu_policy::Phase) (E04).

use serde::{Deserialize, Serialize};

mod merge;

/// Correspondência glob determinística, partilhada com o motor de política (vocabulário v2).
pub use katu_policy::matches_glob;
pub use merge::MergeError;

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
        let _span = crate::trace_fn!("plan::new");

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
    /// Orçamento de tempo em minutos (`None` = sem teto). Merge = **mínimo** (E09-T04).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_budget_minutes: Option<u64>,
    /// Egress de rede permitido (`false` por omissão). Merge = `AND` (E09-T04).
    #[serde(default)]
    pub network_egress: bool,
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
        let _span = crate::trace_fn!("plan::new");

        Self {
            allowed_files,
            forbidden_files,
            acceptance_criteria,
            rollback_plan: rollback_plan.into(),
            time_budget_minutes: None,
            network_egress: false,
        }
    }

    /// Define o orçamento de tempo em minutos (merge por **mínimo**, E09-T04).
    #[must_use]
    pub fn with_time_budget(mut self, minutes: u64) -> Self {
        let _span = crate::trace_fn!("plan::with_time_budget");

        self.time_budget_minutes = Some(minutes);
        self
    }

    /// `true` se o caminho é permitido pelo contrato (proibido vence; `allowed` vazio = tudo).
    #[must_use]
    pub fn allows(&self, path: &str) -> bool {
        let _span = crate::trace_fn!("plan::allows");

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
        let _span = crate::trace_fn!("plan::new");

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
        let _span = crate::trace_fn!("plan::validate");

        if self.feature_list.is_empty() {
            return Err(PlanError::EmptyFeatureList);
        }
        if self.scope_contract.forbidden_files.is_empty() {
            return Err(PlanError::MissingForbiddenFiles);
        }
        if self.scope_contract.rollback_plan.trim().is_empty() {
            return Err(PlanError::MissingRollbackPlan);
        }
        for pattern in self
            .scope_contract
            .allowed_files
            .iter()
            .chain(&self.scope_contract.forbidden_files)
        {
            if !is_relative_glob(pattern) {
                return Err(PlanError::NonRelativeGlob {
                    pattern: pattern.clone(),
                });
            }
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
    /// Glob absoluto ou com `..` (o escopo são globs relativos, não paths).
    NonRelativeGlob {
        /// O padrão rejeitado.
        pattern: String,
    },
}

/// `true` se o padrão é um glob relativo (sem raiz absoluta nem `..`).
fn is_relative_glob(pattern: &str) -> bool {
    let _span = crate::trace_fn!("plan::is_relative_glob");

    !pattern.starts_with('/')
        && !pattern.starts_with('\\')
        && !pattern.split(['/', '\\']).any(|segment| segment == "..")
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _span = crate::trace_fn!("plan::fmt");

        match self {
            Self::EmptyFeatureList => f.write_str("plano sem features"),
            Self::MissingForbiddenFiles => f.write_str("plano sem forbidden_files"),
            Self::MissingRollbackPlan => f.write_str("plano sem rollback_plan"),
            Self::MultipleInProgress => f.write_str("mais de uma feature in_progress"),
            Self::NonRelativeGlob { pattern } => write!(f, "glob não relativo: {pattern}"),
        }
    }
}

impl std::error::Error for PlanError {}

#[cfg(test)]
mod tests;
