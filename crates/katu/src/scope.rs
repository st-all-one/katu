//! Contrato de escopo e `feature_list` carregados no arranque (E09-T04).
//!
//! O artefacto de plano (E06-T06) é **dado do projeto**, não do modelo: o modelo só fornece
//! `goal`/`next_action`. Carregar `scope_contract.json` + `feature_list.json` da raiz e validá-los
//! **antes de qualquer turno** mantém a tool `plan` fail-closed (sem artefacto, indisponível) e o
//! invariante "≤ 1 `in_progress`" verificado no startup. Sem os dois ficheiros, nada muda; com um
//! só, recusa (artefacto incompleto); com ambos inválidos, recusa (DF4).

use std::path::Path;

use katu_core::diag::{Level, events};
use katu_core::plan::{Feature, Plan, PlanError, ScopeContract};
use katu_core::ports::{Fs, FsError};
use serde::Deserialize;
use serde::de::DeserializeOwned;

/// Nome do ficheiro do contrato de escopo (raiz do projeto).
const SCOPE_CONTRACT_FILE: &str = "scope_contract.json";
/// Nome do ficheiro da lista de features (raiz do projeto).
const FEATURE_LIST_FILE: &str = "feature_list.json";

/// Falha ao carregar o artefacto de plano (fail-closed).
#[derive(Debug, thiserror::Error)]
pub(crate) enum ScopeError {
    /// Leitura falhou.
    #[error("lendo {path}: {source}")]
    Io {
        /// Caminho do ficheiro.
        path: String,
        /// Causa.
        source: FsError,
    },
    /// JSON malformado.
    #[error("{path}: JSON inválido: {message}")]
    Json {
        /// Caminho do ficheiro.
        path: String,
        /// Mensagem do parser.
        message: String,
    },
    /// Um dos ficheiros existe sem o outro.
    #[error("artefacto de plano incompleto: falta {missing}")]
    Incomplete {
        /// Ficheiro em falta.
        missing: &'static str,
    },
    /// O plano não passa a validação de schema.
    #[error("plano inválido: {0}")]
    Plan(#[from] PlanError),
}

/// Carrega e valida o plano do projeto, se o artefacto existir.
///
/// Devolve `None` quando **nenhum** dos ficheiros existe (a tool `plan` fica indisponível). Um
/// artefacto pela metade ou inválido é erro (fail-closed).
///
/// # Errors
/// [`ScopeError`] em falha de leitura, JSON malformado, artefacto incompleto ou plano inválido.
pub(crate) fn load(fs: &dyn Fs, root: &Path) -> Result<Option<Plan>, ScopeError> {
    let _span = katu_core::span!(Level::Info, events::SCOPE_LOAD, "root" => root.to_str().unwrap_or_default());
    let scope_path = root.join(SCOPE_CONTRACT_FILE);
    let features_path = root.join(FEATURE_LIST_FILE);
    match (fs.exists(&scope_path), fs.exists(&features_path)) {
        (false, false) => Ok(None),
        (true, false) => Err(ScopeError::Incomplete {
            missing: FEATURE_LIST_FILE,
        }),
        (false, true) => Err(ScopeError::Incomplete {
            missing: SCOPE_CONTRACT_FILE,
        }),
        (true, true) => {
            let scope: ScopeContract = read_json(fs, &scope_path)?;
            let features = read_features(fs, &features_path)?;
            let plan = Plan::new(scope, features);
            plan.validate()?;
            Ok(Some(plan))
        }
    }
}

/// Lê e desserializa um ficheiro JSON através da porta `Fs`.
fn read_json<T: DeserializeOwned>(fs: &dyn Fs, path: &Path) -> Result<T, ScopeError> {
    let bytes = fs.read(path).map_err(|source| ScopeError::Io {
        path: path.display().to_string(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|err| ScopeError::Json {
        path: path.display().to_string(),
        message: err.to_string(),
    })
}

/// Formas aceitas de `feature_list.json`: array direto ou objeto `{ "features": [...] }`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum FeaturesFile {
    /// `[ { "id": "F1", … } ]`
    Bare(Vec<Feature>),
    /// `{ "features": [ … ] }`
    Wrapped {
        /// Lista de features.
        features: Vec<Feature>,
    },
}

/// Lê a lista de features nas formas aceitas.
fn read_features(fs: &dyn Fs, path: &Path) -> Result<Vec<Feature>, ScopeError> {
    match read_json::<FeaturesFile>(fs, path)? {
        FeaturesFile::Bare(features) | FeaturesFile::Wrapped { features } => Ok(features),
    }
}

#[cfg(test)]
mod tests {
    use katu_core::plan::{FeatureStatus, Plan};
    use katu_core::ports::{Fs, FsError, MemFs};

    use super::{FEATURE_LIST_FILE, SCOPE_CONTRACT_FILE, ScopeError, load};

    /// Contrato válido com os dois campos obrigatórios.
    const CONTRACT: &str = r#"{
        "allowed_files": ["src/**"],
        "forbidden_files": ["**/secrets/**"],
        "acceptance_criteria": ["testes passam"],
        "rollback_plan": "reverter o commit"
    }"#;

    /// Lista de features (array direto) com uma `in_progress`.
    const FEATURES: &str = r#"[{"id":"F1","description":"fazer","status":"in_progress"}]"#;

    fn root() -> std::path::PathBuf {
        std::path::PathBuf::from("/work")
    }

    fn write(fs: &MemFs, name: &str, body: &str) -> Result<(), FsError> {
        fs.write_atomic(&root().join(name), body.as_bytes())
    }

    #[test]
    fn absent_artifact_leaves_plan_unavailable() -> Result<(), Box<dyn std::error::Error>> {
        let fs = MemFs::new();
        assert!(load(&fs, &root())?.is_none());
        Ok(())
    }

    #[test]
    fn valid_artifact_loads_and_validates() -> Result<(), Box<dyn std::error::Error>> {
        let fs = MemFs::new();
        write(&fs, SCOPE_CONTRACT_FILE, CONTRACT)?;
        write(&fs, FEATURE_LIST_FILE, FEATURES)?;
        let plan = load(&fs, &root())?.ok_or("esperava um plano")?;
        plan.validate()?;
        assert_eq!(plan.feature_list.len(), 1);
        assert_eq!(
            plan.feature_list.first().map(|f| f.status),
            Some(FeatureStatus::InProgress)
        );
        Ok(())
    }

    #[test]
    fn wrapped_feature_list_is_accepted() -> Result<(), Box<dyn std::error::Error>> {
        let fs = MemFs::new();
        write(&fs, SCOPE_CONTRACT_FILE, CONTRACT)?;
        write(
            &fs,
            FEATURE_LIST_FILE,
            r#"{"features":[{"id":"F1","description":"fazer","status":"pending"}]}"#,
        )?;
        let plan: Plan = load(&fs, &root())?.ok_or("esperava um plano")?;
        assert_eq!(plan.feature_list.len(), 1);
        Ok(())
    }

    #[test]
    fn half_an_artifact_is_refused() -> Result<(), Box<dyn std::error::Error>> {
        let fs = MemFs::new();
        write(&fs, SCOPE_CONTRACT_FILE, CONTRACT)?;
        assert!(matches!(
            load(&fs, &root()),
            Err(ScopeError::Incomplete { .. })
        ));
        Ok(())
    }

    #[test]
    fn invalid_plan_is_refused() -> Result<(), Box<dyn std::error::Error>> {
        let fs = MemFs::new();
        write(
            &fs,
            SCOPE_CONTRACT_FILE,
            r#"{"allowed_files":[],"forbidden_files":[],"acceptance_criteria":[],"rollback_plan":"r"}"#,
        )?;
        write(&fs, FEATURE_LIST_FILE, FEATURES)?;
        assert!(matches!(load(&fs, &root()), Err(ScopeError::Plan(_))));
        Ok(())
    }

    #[test]
    fn malformed_json_is_refused() -> Result<(), Box<dyn std::error::Error>> {
        let fs = MemFs::new();
        write(&fs, SCOPE_CONTRACT_FILE, "{não é json")?;
        write(&fs, FEATURE_LIST_FILE, FEATURES)?;
        assert!(matches!(load(&fs, &root()), Err(ScopeError::Json { .. })));
        Ok(())
    }
}
