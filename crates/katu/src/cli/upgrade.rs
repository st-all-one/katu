//! `upgrade` (E20-T02): sincronização de versão contra GitHub Releases.
//!
//! Consulta a versão mais recente publicada no GitHub Releases e compara com a versão atual do
//! binário. Se houver uma versão mais recente, mostra o comando de atualização (o `install.sh`
//! resolve e verifica o artefacto); nunca descarrega nem executa nada sozinho (fail-closed).

use std::time::Duration;

use clap::Args;
use katu_core::diag::{Level, events};
use katu_core::error::Error;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::json;

use crate::report::Report;

/// Argumentos de `katu upgrade`.
#[derive(Debug, Clone, Args)]
pub(crate) struct UpgradeArgs {
    /// Emite envelope JSON em `stdout`.
    #[arg(long)]
    pub(crate) json: bool,
}

/// Resposta da API do GitHub Releases.
#[derive(Debug, Deserialize)]
struct Release {
    /// Tag da release (ex: `v0.1.0`).
    tag_name: String,
}

/// Versão atual do katu.
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Repositório do GitHub (owner/repo).
const REPO: &str = "st-all-one/katu";

/// URL base da API do GitHub.
const GITHUB_API: &str = "https://api.github.com";

/// Executa `katu upgrade` (verifica se há versão mais recente).
pub(crate) fn execute(_args: &UpgradeArgs) -> Report {
    let _span = katu_core::fn_span!(Level::Debug, events::CLI_UPGRADE, "upgrade::execute");

    match check_for_updates() {
        Ok(Some(latest)) => {
            let message = format!(
                "Nova versão disponível: {latest} (atual: {CURRENT_VERSION})\n\
                 Execute: curl -fsSL https://raw.githubusercontent.com/{REPO}/main/install.sh | bash"
            );
            Report::ok(
                "upgrade",
                Some(json!({
                    "update_available": true,
                    "latest": latest,
                    "current": CURRENT_VERSION,
                    "message": message,
                })),
            )
        }
        Ok(None) => Report::ok(
            "upgrade",
            Some(json!({
                "update_available": false,
                "current": CURRENT_VERSION,
                "message": "katu está atualizado",
            })),
        ),
        Err(error) => Report::failed(
            "upgrade",
            &Error::unavailable(format!("falha ao verificar atualizações: {error}")),
        ),
    }
}

/// Verifica se há versão mais recente no GitHub Releases.
fn check_for_updates() -> Result<Option<String>, Box<dyn std::error::Error>> {
    let _span = katu_core::trace_fn!("upgrade::check_for_updates");

    let url = format!("{GITHUB_API}/repos/{REPO}/releases/latest");
    let client = Client::builder()
        .user_agent(format!("katu/{CURRENT_VERSION}"))
        .timeout(Duration::from_secs(10))
        .build()?;

    let response = client.get(&url).send()?;
    if !response.status().is_success() {
        return Err(format!("GitHub API retornou status {}", response.status()).into());
    }

    let release: Release = response.json()?;
    if is_newer(&release.tag_name, CURRENT_VERSION) {
        Ok(Some(release.tag_name))
    } else {
        Ok(None)
    }
}

/// Compara versões `vX.Y.Z` **numericamente** (o texto sozinho ordenaria `0.10` antes de `0.9`).
///
/// Componentes em falta contam como `0`; sufixos de pré-lançamento (`-rc.1`) e metadados (`+build`)
/// são ignorados nesta comparação simples (o canal publicado é sempre uma tag estável).
fn is_newer(candidate: &str, current: &str) -> bool {
    let _span = katu_core::trace_fn!("upgrade::is_newer");

    parse_version(candidate) > parse_version(current)
}

/// Extrai os três componentes numéricos de uma versão.
fn parse_version(version: &str) -> (u64, u64, u64) {
    let _span = katu_core::trace_fn!("upgrade::parse_version");

    let core = version.trim_start_matches('v');
    let core = core.split(['-', '+']).next().unwrap_or(core);
    let mut parts = core.split('.').map(|part| part.parse::<u64>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_version_is_set() {
        assert!(!CURRENT_VERSION.is_empty());
    }

    #[test]
    fn repo_is_set() {
        assert_eq!(REPO, "st-all-one/katu");
    }

    #[test]
    fn version_comparison_is_numeric() {
        // O bug que o texto puro escondia: "0.10.0" é **mais novo** que "0.9.0".
        assert!(is_newer("v0.10.0", "0.9.0"));
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("v1.0.0", "0.99.99"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("v0.0.9", "0.1.0"));
        assert!(is_newer("v0.1.1", "0.1.0"));
    }

    #[test]
    fn version_suffixes_are_ignored() {
        assert_eq!(parse_version("v1.2.3-rc.1"), (1, 2, 3));
        assert_eq!(parse_version("1.2+build.7"), (1, 2, 0));
        assert_eq!(parse_version("2"), (2, 0, 0));
    }
}
