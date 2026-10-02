//! Login do agente principal (E21): opencode **ou** llama.cpp, nunca os dois.
//!
//! O login mexe **só** no provider/modelo/base do agente e guarda a chave do opencode na config
//! **global** (`katu.toml`, chave `opencode_api_key`, modo 0600). O embedding fica reservado ao
//! `katu.toml` (E20-T17): este módulo nunca toca em `embeddings.*`. Como só existe uma chave
//! `provider`, ligar um provider desliga o outro por construção — não há estado “os dois ligados”.

use std::path::Path;
use std::time::Duration;

use katu_core::error::Error;
use katu_core::ports::Env as _;
use katu_core::provider::Flow;
use katu_providers::{HttpRequest, Transport as _, UreqTransport};

use crate::agent::{default_base, default_model};
use crate::config;
use crate::ports::StdEnv;

mod prompt;

#[cfg(test)]
mod tests;

/// Chave da API na config global. Fica **fora** de [`crate::config::KEYS`]: não aparece no `list`
/// nem é aceite por `config get/set`, mas vive no `katu.toml` (modo 0600).
const KEY: &str = "opencode_api_key";

/// Variáveis de ambiente aceites como chave (a primeira não-vazia vence).
const KEY_ENVS: &[&str] = &["KATU_OPENCODE_KEY", "OPENCODE_API_KEY"];

/// Provider do agente principal reconhecido pelo login.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Provider {
    /// opencode Go (`opencode-go`).
    OpenCodeGo,
    /// opencode Zen (`opencode-zen`).
    OpenCodeZen,
    /// llama.cpp local (`llama`).
    Llama,
}

impl Provider {
    /// Nome canônico escrito no `katu.toml`.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::OpenCodeGo => "opencode-go",
            Self::OpenCodeZen => "opencode-zen",
            Self::Llama => "llama",
        }
    }

    /// Rótulo humano (prompt/menu).
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::OpenCodeGo => "opencode (Go)",
            Self::OpenCodeZen => "opencode (Zen)",
            Self::Llama => "llama.cpp (local)",
        }
    }

    /// `true` se exige chave de API.
    pub(crate) const fn needs_key(self) -> bool {
        matches!(self, Self::OpenCodeGo | Self::OpenCodeZen)
    }

    /// Todas as escolhas, pela ordem do prompt/menu.
    pub(crate) const fn all() -> [Self; 3] {
        [Self::OpenCodeGo, Self::OpenCodeZen, Self::Llama]
    }
}

/// Reconhece o nome canônico e os atalhos (`opencode` → Go, `local` → llama).
pub(crate) fn parse(name: &str) -> Option<Provider> {
    let _span = katu_core::trace_fn!("login::parse");
    match name {
        "opencode" | "opencode-go" | "go" => Some(Provider::OpenCodeGo),
        "opencode-zen" | "zen" => Some(Provider::OpenCodeZen),
        "llama" | "llama.cpp" | "llama-cpp" | "local" => Some(Provider::Llama),
        _ => None,
    }
}

/// Intenção do pedido: iniciar sessão ou terminá-la.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Intent {
    /// Inicia sessão: escreve a seleção e guarda a chave.
    Login,
    /// Termina a sessão: apaga a chave guardada.
    Logout,
}

/// Pedido de login já resolvido (da CLI interativa ou das flags).
pub(crate) struct Request {
    /// Provider escolhido.
    pub(crate) provider: Provider,
    /// Chave da API (só opencode); ausente → env/guardada.
    pub(crate) api_key: Option<String>,
    /// Modelo; ausente → default do provider.
    pub(crate) model: Option<String>,
    /// URL base; ausente → default do provider.
    pub(crate) base: Option<String>,
    /// Termina a sessão: apaga a chave guardada e não escreve seleção.
    pub(crate) logout: bool,
}

/// O que ficou configurado.
pub(crate) struct Outcome {
    /// Provider canônico.
    pub(crate) provider: &'static str,
    /// Modelo efetivo.
    pub(crate) model: String,
    /// URL base efetiva.
    pub(crate) base: String,
    /// `true` quando ficou uma chave guardada.
    pub(crate) logged_in: bool,
    /// Aviso não fatal (ex.: `KATU_OPENCODE_KEY` a sobrepor-se à chave guardada).
    pub(crate) warning: Option<String>,
}

/// Constrói o pedido a partir das flags; sem `provider`, pergunta interativamente.
pub(crate) fn resolve(
    provider: Option<&str>,
    api_key: Option<&str>,
    model: Option<&str>,
    base: Option<&str>,
    intent: Intent,
) -> Result<Request, Error> {
    let _span = katu_core::trace_fn!("login::resolve");
    if intent == Intent::Logout {
        let provider = match provider {
            Some(name) => parse(name).ok_or_else(|| unknown(name))?,
            None => Provider::OpenCodeGo,
        };
        return Ok(Request {
            provider,
            api_key: None,
            model: None,
            base: None,
            logout: true,
        });
    }
    let Some(name) = provider else {
        return prompt::interactive();
    };
    let provider = parse(name).ok_or_else(|| unknown(name))?;
    Ok(Request {
        provider,
        api_key: api_key.map(str::to_string),
        model: model.map(str::to_string),
        base: base.map(str::to_string),
        logout: false,
    })
}

/// Erro que ensina: nome inválido + lista de válidos.
fn unknown(name: &str) -> Error {
    let _span = katu_core::trace_fn!("login::unknown");
    Error::invalid_input(format!(
        "provider desconhecido: `{name}` (use `opencode`, `opencode-zen` ou `llama`)"
    ))
}

/// Aplica o pedido: escreve a seleção (e a chave) na config **global**.
pub(crate) fn apply(request: &Request) -> Result<Outcome, Error> {
    let _span = katu_core::trace_fn!("login::apply");
    let name = request.provider.name();
    if request.logout {
        clear_key()?;
        return Ok(Outcome {
            provider: name,
            model: default_model(name).to_string(),
            base: default_base(name).to_string(),
            logged_in: false,
            warning: None,
        });
    }
    let model =
        non_empty(request.model.as_deref()).unwrap_or_else(|| default_model(name).to_string());
    let base = non_empty(request.base.as_deref()).unwrap_or_else(|| default_base(name).to_string());
    let key = if request.provider.needs_key() {
        Some(resolve_key(request.api_key.as_deref())?)
    } else {
        None
    };
    let warning = key.as_deref().and_then(shadowed_by_env);
    write_global(name, &model, &base, key.as_deref())?;
    Ok(Outcome {
        provider: name,
        model,
        base,
        logged_in: key.is_some(),
        warning,
    })
}

/// Aviso quando uma chave antiga no ambiente se sobrepõe à chave guardada.
fn shadowed_by_env(key: &str) -> Option<String> {
    let _span = katu_core::trace_fn!("login::shadowed_by_env");
    let (name, value) = env_key()?;
    if value == key {
        return None;
    }
    Some(format!(
        "{name} está definido no ambiente e **sobrepõe-se** à chave guardada; \
         remova-o (`unset {name}`) para usar a que acabou de guardar"
    ))
}

/// Chave vinda do ambiente: `(nome, valor)` da primeira variável não-vazia.
fn env_key() -> Option<(&'static str, String)> {
    let _span = katu_core::trace_fn!("login::env_key");
    let env = StdEnv;
    KEY_ENVS
        .iter()
        .find_map(|name| non_empty(env.var(name).as_deref()).map(|value| (*name, value)))
}

/// Chave do opencode guardada na config global, se existir.
pub(crate) fn stored_key() -> Option<String> {
    let _span = katu_core::trace_fn!("login::stored_key");

    let path = config::global_path().ok()?;
    let table = config::load(&path).ok()?;
    match config::get_key(&table, KEY) {
        Some(toml::Value::String(key)) => non_empty(Some(&key)),
        _ => None,
    }
}

/// Resolve a chave: explícita > ambiente > guardada.
fn resolve_key(explicit: Option<&str>) -> Result<String, Error> {
    let _span = katu_core::trace_fn!("login::resolve_key");
    if let Some(key) = non_empty(explicit) {
        return Ok(key);
    }
    if let Some((_, key)) = env_key() {
        return Ok(key);
    }
    if let Some(key) = stored_key() {
        return Ok(key);
    }
    Err(Error::invalid_input(
        "chave do opencode ausente: use `--api-key`, exporte `KATU_OPENCODE_KEY` ou repita o login",
    ))
}

/// Devolve o texto sem espaços, ou `None` se ficar vazio.
fn non_empty(value: Option<&str>) -> Option<String> {
    let _span = katu_core::trace_fn!("login::non_empty");
    value
        .map(str::trim)
        .filter(|trimmed| !trimmed.is_empty())
        .map(str::to_string)
}

/// Escreve a seleção do agente (e a chave, se houver) na config **global**; modo 0600.
fn write_global(provider: &str, model: &str, base: &str, key: Option<&str>) -> Result<(), Error> {
    let _span = katu_core::trace_fn!("login::write_global");
    let path = config::global_path()?;
    let mut table = config::load(&path)?;
    for (name, value) in [("provider", provider), ("model", model), ("base", base)] {
        config::set_key(&mut table, name, toml::Value::String(value.to_string()));
    }
    if let Some(key) = key {
        config::set_key(&mut table, KEY, toml::Value::String(key.to_string()));
    }
    config::save(&path, &table)?;
    restrict(&path)
}

/// Apaga a chave guardada (logout) da config global.
fn clear_key() -> Result<(), Error> {
    let _span = katu_core::trace_fn!("login::clear_key");
    let path = config::global_path()?;
    let mut table = config::load(&path)?;
    if config::unset_key(&mut table, KEY) {
        config::save(&path, &table)?;
        restrict(&path)?;
    }
    Ok(())
}

/// Verifica a chave com um pedido mínimo ao gateway (só opencode).
///
/// O `/models` do opencode **não** autentica (devolve 200 sem chave), pelo que a única verificação
/// fiel é um `chat/completions` mínimo: um 401 só aparece aqui. Nunca falha o login.
pub(crate) fn verify(outcome: &Outcome) -> Option<String> {
    let _span = katu_core::trace_fn!("login::verify");
    let provider = parse(outcome.provider)?;
    if !provider.needs_key() {
        return None;
    }
    let key = stored_key()?;
    let transport = UreqTransport::new(Duration::from_secs(5), Duration::from_secs(20));
    let url = format!("{}/chat/completions", outcome.base.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": outcome.model,
        "messages": [{"role": "user", "content": "ping"}],
        "max_tokens": 1,
        "stream": false,
    });
    let payload = serde_json::to_vec(&body).ok()?;
    let request = HttpRequest::post(
        url,
        payload,
        vec![
            ("content-type".to_string(), "application/json".to_string()),
            ("authorization".to_string(), format!("Bearer {key}")),
        ],
    );
    let mut sink = |_chunk: &[u8]| Flow::Break;
    match transport.send(&request, &mut sink) {
        Ok(meta) if (200..300).contains(&meta.status) => {
            Some(format!("chave aceite pelo {} (HTTP 200)", outcome.provider))
        }
        Ok(meta) if meta.status == 401 => Some(
            "chave recusada (HTTP 401): confirme que é uma chave do opencode e que escolheu \
             Go/Zen corretamente; um `KATU_OPENCODE_KEY` antigo no ambiente sobrepõe-se"
                .to_string(),
        ),
        Ok(meta) => Some(format!("verificação da chave: HTTP {}", meta.status)),
        Err(error) => Some(format!("não verifiquei a chave: {error}")),
    }
}

/// Restringe a config global a `0600` (só o dono lê; contém a chave da API).
#[cfg(unix)]
fn restrict(path: &Path) -> Result<(), Error> {
    use std::os::unix::fs::PermissionsExt as _;
    let _span = katu_core::trace_fn!("login::restrict");

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|error| Error::io(path.display().to_string(), error))
}

/// Sem semântica de permissões POSIX: nada a restringir.
#[cfg(not(unix))]
fn restrict(_path: &Path) -> Result<(), Error> {
    let _span = katu_core::trace_fn!("login::restrict");
    Ok(())
}
