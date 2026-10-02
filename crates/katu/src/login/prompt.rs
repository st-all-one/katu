//! Prompts interactivos do login (E21): provider, base, modelo e chave.
//!
//! Só a CLI sem `--provider` chega aqui; a chave é lida **sem eco** (`rpassword`). O `stdout` fica
//! reservado a dados: tudo o que é prompt vai para `stderr`.

use std::io::{self, Write as _};

use katu_core::error::Error;

use crate::agent::{default_base, default_model};

use super::{Provider, Request, non_empty, parse};

/// Pergunta interativamente pelo login (usado quando a CLI não recebe `--provider`).
pub(super) fn interactive() -> Result<Request, Error> {
    let _span = katu_core::trace_fn!("login::interactive");
    let provider = prompt_provider()?;
    let base = prompt_default(
        &format!("URL base [{}]: ", default_base(provider.name())),
        default_base(provider.name()),
    )?;
    let model = prompt_default(
        &format!("Modelo [{}]: ", default_model(provider.name())),
        default_model(provider.name()),
    )?;
    let api_key = if provider.needs_key() {
        Some(prompt_secret()?)
    } else {
        None
    };
    Ok(Request {
        provider,
        api_key,
        model: Some(model),
        base: Some(base),
        logout: false,
    })
}

/// Pergunta pelo provider (número ou nome; vazio → opencode Go).
fn prompt_provider() -> Result<Provider, Error> {
    let _span = katu_core::trace_fn!("login::prompt_provider");
    let choices: Vec<String> = Provider::all()
        .iter()
        .enumerate()
        .map(|(index, provider)| format!("  [{}] {}", index.saturating_add(1), provider.label()))
        .collect();
    let menu = format!("Provider do agente principal:\n{}\n", choices.join("\n"));
    write_stderr(&menu)?;
    let choice = prompt_default("Escolha [1]: ", "1")?;
    if let Some(provider) = parse(&choice) {
        return Ok(provider);
    }
    let index = choice.parse::<usize>().ok().and_then(|n| n.checked_sub(1));
    index
        .and_then(|i| Provider::all().get(i).copied())
        .ok_or_else(|| Error::invalid_input(format!("escolha inválida: `{choice}`")))
}

/// Lê uma linha; vazio devolve o default.
fn prompt_default(prompt: &str, default: &str) -> Result<String, Error> {
    let _span = katu_core::trace_fn!("login::prompt_default");
    write_stderr(prompt)?;
    let mut line = String::new();
    io::stdin()
        .read_line(&mut line)
        .map_err(|err| Error::io("<stdin>", err))?;
    let trimmed = line.trim();
    if trimmed.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(trimmed.to_string())
    }
}

/// Lê a chave sem a ecoar no terminal.
fn prompt_secret() -> Result<String, Error> {
    let _span = katu_core::trace_fn!("login::prompt_secret");
    write_stderr("Chave da API do opencode (não é mostrada): ")?;
    let key = rpassword::read_password().map_err(|err| Error::io("<tty>", err))?;
    non_empty(Some(&key)).ok_or_else(|| Error::invalid_input("chave vazia"))
}

/// Escreve um prompt em `stderr` (stdout fica para dados).
fn write_stderr(text: &str) -> Result<(), Error> {
    let _span = katu_core::trace_fn!("login::write_stderr");
    let mut err = io::stderr();
    err.write_all(text.as_bytes())
        .and_then(|()| err.flush())
        .map_err(|error| Error::io("<stderr>", error))
}
