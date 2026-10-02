//! `katu config login` (E21): resolve (flags ou interativo) e aplica o login do agente principal.
//!
//! A chave vive na config **global** (`katu.toml`, `opencode_api_key`, modo 0600). Com um terminal
//! interactivo, a chave é **verificada** com um `chat/completions` mínimo (o `/models` do opencode
//! não autentica) e o resultado entra no envelope como `check`.

use std::io::IsTerminal as _;

use katu_core::diag::{Level, events};
use serde_json::json;

use crate::config;
use crate::login;
use crate::report::Report;

/// Resolve e aplica o login; devolve o relatório pronto a emitir.
pub(super) fn run(
    provider: Option<&str>,
    api_key: Option<&str>,
    model: Option<&str>,
    base: Option<&str>,
    intent: login::Intent,
) -> Report {
    let _span = katu_core::fn_span!(Level::Debug, events::CLI_CONFIG, "config_cmd::login");
    let request = match login::resolve(provider, api_key, model, base, intent) {
        Ok(request) => request,
        Err(error) => return Report::failed("config", &error),
    };
    let outcome = match login::apply(&request) {
        Ok(outcome) => outcome,
        Err(error) => return Report::failed("config", &error),
    };
    let mut data = serde_json::Map::new();
    data.insert("provider".to_string(), json!(outcome.provider));
    data.insert("model".to_string(), json!(outcome.model));
    data.insert("base".to_string(), json!(outcome.base));
    data.insert("logged_in".to_string(), json!(outcome.logged_in));
    data.insert(
        "config".to_string(),
        json!(
            config::global_path()
                .map(|path| path.display().to_string())
                .ok()
        ),
    );
    if let Some(warning) = &outcome.warning {
        data.insert("warning".to_string(), json!(warning));
    }
    // Verificação com rede só quando há um humano no terminal (scripts/testes ficam offline).
    if std::io::stdin().is_terminal()
        && let Some(message) = login::verify(&outcome)
    {
        data.insert("check".to_string(), json!(message));
    }
    Report::ok("config", Some(serde_json::Value::Object(data)))
}
