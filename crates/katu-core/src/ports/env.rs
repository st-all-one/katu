//! Porta de ambiente (`Env`).

use std::collections::BTreeMap;

/// Porta de ambiente e argumentos.
///
/// O núcleo **não** lê `std::env` diretamente (reforçado por `clippy.toml`).
pub trait Env: Send + Sync {
    /// Valor de uma variável de ambiente.
    fn var(&self, key: &str) -> Option<String>;

    /// Argumentos do processo, sem o nome do programa.
    fn args(&self) -> Vec<String>;

    /// Todas as variáveis de ambiente (para filtragem antes de executar).
    fn vars(&self) -> Vec<(String, String)>;
}

/// Ambiente falso e determinístico.
#[derive(Debug, Clone, Default)]
pub struct FakeEnv {
    vars: BTreeMap<String, String>,
    args: Vec<String>,
}

impl FakeEnv {
    /// Cria um ambiente vazio.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Define uma variável de ambiente.
    #[must_use]
    pub fn with_var(mut self, key: &str, value: &str) -> Self {
        self.vars.insert(key.to_string(), value.to_string());
        self
    }

    /// Define os argumentos do processo.
    #[must_use]
    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }
}

impl Env for FakeEnv {
    fn var(&self, key: &str) -> Option<String> {
        self.vars.get(key).cloned()
    }

    fn args(&self) -> Vec<String> {
        self.args.clone()
    }

    fn vars(&self) -> Vec<(String, String)> {
        self.vars
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{Env, FakeEnv};

    #[test]
    fn fake_env_reads_configured_values() {
        let env = FakeEnv::new()
            .with_var("KATU_TEST", "on")
            .with_args(vec!["--json".to_string()]);
        assert_eq!(env.var("KATU_TEST"), Some("on".to_string()));
        assert_eq!(env.var("MISSING"), None);
        assert_eq!(env.args(), vec!["--json".to_string()]);
    }
}
