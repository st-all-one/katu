//! Configuração do katu (E20-T09): global única + override local.
//!
//! Conjunto **fechado** de chaves; precedência **projeto > global**; o `init` copia a global 1:1
//! (snapshot, E20-T18). Nada aqui inventa valores: chave ou valor inválido → `invalid_input`.

use std::path::{Path, PathBuf};

use katu_core::diag::{Level, events};
use katu_core::error::Error;

/// Tipo de valor de uma chave canônica.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Texto.
    Text,
    /// Booleano (`true`/`false`).
    Bool,
    /// Inteiro com sinal.
    Integer,
}

/// Especificação de uma chave canônica.
pub(crate) struct KeySpec {
    /// Nome pontuado (ex.: `embeddings.url`).
    pub(crate) key: &'static str,
    /// Tipo do valor.
    pub(crate) kind: Kind,
    /// Descrição curta (para `help`/`list`).
    pub(crate) doc: &'static str,
}

/// Chaves canônicas (conjunto fechado, ordem estável).
pub(crate) const KEYS: &[KeySpec] = &[
    KeySpec {
        key: "provider",
        kind: Kind::Text,
        doc: "Provider por omissão.",
    },
    KeySpec {
        key: "model",
        kind: Kind::Text,
        doc: "Modelo por omissão (vazio = tier da fase).",
    },
    KeySpec {
        key: "thinking",
        kind: Kind::Text,
        doc: "Grau de pensamento por omissão.",
    },
    KeySpec {
        key: "base",
        kind: Kind::Text,
        doc: "Base URL do provider por omissão.",
    },
    KeySpec {
        key: "log_level",
        kind: Kind::Text,
        doc: "Nível de log por omissão.",
    },
    KeySpec {
        key: "git.versioned",
        kind: Kind::Bool,
        doc: "Versionar o `.katu/` (default `true`).",
    },
    KeySpec {
        key: "memory.persist_in_project",
        kind: Kind::Bool,
        doc: "Persistir a memória dentro do projeto.",
    },
    KeySpec {
        key: "behavior.auto_compact",
        kind: Kind::Bool,
        doc: "Compactar o histórico automaticamente.",
    },
    KeySpec {
        key: "behavior.prompt_state",
        kind: Kind::Bool,
        doc: "Incluir a secção `estado` no prime (Q-04; default `false`).",
    },
    KeySpec {
        key: "behavior.context_selection",
        kind: Kind::Text,
        doc: "Política de seleção do contexto: `suffix` (default) ou `utility` (Q-02b/Q-03).",
    },
    KeySpec {
        key: "recall.default_limit",
        kind: Kind::Integer,
        doc: "Máximo de resultados por omissão no `memo ask`.",
    },
    KeySpec {
        key: "embeddings.url",
        kind: Kind::Text,
        doc: "URL do serviço de embeddings (vazio = `off`).",
    },
    KeySpec {
        key: "embeddings.model",
        kind: Kind::Text,
        doc: "Modelo de embeddings.",
    },
    KeySpec {
        key: "embeddings.command",
        kind: Kind::Text,
        doc: "Comando opcional para lançar o serviço de embeddings.",
    },
];

/// Devolve a especificação de uma chave canônica.
pub(crate) fn find(key: &str) -> Option<&'static KeySpec> {
    let _span = katu_core::trace_fn!("config::find");

    KEYS.iter().find(|spec| spec.key == key)
}

/// Erro de chave desconhecida, listando o conjunto fechado (e uma sugestão por prefixo).
pub(crate) fn unknown_key(key: &str) -> Error {
    let _span = katu_core::trace_fn!("config::unknown_key");

    let prefix = key.split('.').next().unwrap_or(key);
    let hint = KEYS
        .iter()
        .find(|spec| spec.key.starts_with(prefix) && spec.key != key);
    let mut message = format!("chave desconhecida: `{key}`");
    if let Some(spec) = hint {
        message.push_str(" (quiseste dizer `");
        message.push_str(spec.key);
        message.push_str("`?)");
    }
    message.push_str(". Chaves válidas:");
    for spec in KEYS {
        message.push_str("\n  - ");
        message.push_str(spec.key);
        message.push_str(" — ");
        message.push_str(spec.doc);
    }
    Error::invalid_input(message)
}

/// Converte um valor textual para o tipo da chave.
pub(crate) fn parse_value(kind: Kind, raw: &str) -> Result<toml::Value, Error> {
    let _span = katu_core::fn_span!(Level::Trace, events::CONFIG_SET, "config::parse_value");
    match kind {
        Kind::Text => Ok(toml::Value::String(raw.to_owned())),
        Kind::Bool => match raw {
            "true" => Ok(toml::Value::Boolean(true)),
            "false" => Ok(toml::Value::Boolean(false)),
            _ => Err(Error::invalid_input(format!(
                "valor booleano inválido: `{raw}` (use `true` ou `false`)"
            ))),
        },
        Kind::Integer => raw
            .parse::<i64>()
            .map(toml::Value::Integer)
            .map_err(|_| Error::invalid_input(format!("valor inteiro inválido: `{raw}`"))),
    }
}

/// Renderiza um valor para `get`/`list` (texto simples, sem TOML).
pub(crate) fn display(value: &toml::Value) -> String {
    let _span = katu_core::trace_fn!("config::display");

    match value {
        toml::Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// Lê a chave pontuada de uma tabela (navegando sub-tabelas).
pub(crate) fn get_key(table: &toml::Table, key: &str) -> Option<toml::Value> {
    let _span = katu_core::fn_span!(Level::Trace, events::CONFIG_LOAD, "config::get_key");
    let mut parts = key.split('.');
    let first = parts.next()?;
    let mut value = table.get(first)?;
    for part in parts {
        value = value.as_table()?.get(part)?;
    }
    Some(value.clone())
}

/// Escreve a chave pontuada numa tabela, criando sub-tabelas em falta.
pub(crate) fn set_key(table: &mut toml::Table, key: &str, value: toml::Value) {
    let _span = katu_core::fn_span!(Level::Trace, events::CONFIG_SET, "config::set_key");
    let mut parts: Vec<&str> = key.split('.').collect();
    let Some(leaf) = parts.pop() else {
        return;
    };
    let mut current = table;
    for part in parts {
        if !current.contains_key(part) {
            current.insert(part.to_owned(), toml::Value::Table(toml::Table::new()));
        }
        let Some(next) = current.get_mut(part).and_then(toml::Value::as_table_mut) else {
            return;
        };
        current = next;
    }
    current.insert(leaf.to_owned(), value);
}

/// Remove a chave pontuada; devolve `true` se existia.
pub(crate) fn unset_key(table: &mut toml::Table, key: &str) -> bool {
    let _span = katu_core::fn_span!(Level::Trace, events::CONFIG_SET, "config::unset_key");
    let mut parts: Vec<&str> = key.split('.').collect();
    let Some(leaf) = parts.pop() else {
        return false;
    };
    let mut current = table;
    for part in parts {
        let Some(next) = current.get_mut(part).and_then(toml::Value::as_table_mut) else {
            return false;
        };
        current = next;
    }
    current.remove(leaf).is_some()
}

/// Funde `over` sobre `base` (o projeto vence o global, chave a chave).
pub(crate) fn merge(base: &mut toml::Table, over: &toml::Table) {
    let _span = katu_core::fn_span!(Level::Trace, events::CONFIG_LOAD, "config::merge");
    for (key, value) in over {
        match (base.get_mut(key), value) {
            (Some(toml::Value::Table(base_table)), toml::Value::Table(over_table)) => {
                merge(base_table, over_table);
            }
            _ => {
                base.insert(key.clone(), value.clone());
            }
        }
    }
}

/// Caminho da config do projeto (`<root>/.katu/katu.toml`).
pub(crate) fn project_path(root: &Path) -> PathBuf {
    let _span = katu_core::trace_fn!("config::project_path");

    root.join(".katu").join("katu.toml")
}

/// Lê uma tabela TOML; ficheiro ausente é uma tabela vazia (fail-soft na leitura).
pub(crate) fn load(path: &Path) -> Result<toml::Table, Error> {
    let _span = katu_core::fn_span!(Level::Trace, events::CONFIG_LOAD, "config::load");
    match std::fs::read_to_string(path) {
        Ok(text) => text.parse::<toml::Table>().map_err(|err| {
            Error::invalid_input(format!("config inválida em {}: {err}", path.display()))
        }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(toml::Table::new()),
        Err(err) => Err(Error::io(path.display().to_string(), err)),
    }
}

/// Escreve uma tabela TOML, criando a cadeia de pastas em falta.
pub(crate) fn save(path: &Path, table: &toml::Table) -> Result<(), Error> {
    let _span = katu_core::fn_span!(Level::Trace, events::CONFIG_SET, "config::save");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| Error::io(parent.display().to_string(), err))?;
    }
    let text = toml::to_string(table)
        .map_err(|err| Error::internal(format!("serializando config: {err}")))?;
    std::fs::write(path, text).map_err(|err| Error::io(path.display().to_string(), err))
}

/// Caminho da config global (por sistema operativo).
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) fn global_path() -> Result<PathBuf, Error> {
    let _span = katu_core::trace_fn!("config::global_path");

    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(xdg).join("local/katu/katu.toml"));
    }
    Ok(home()?.join(".config/local/katu/katu.toml"))
}

/// Caminho da config global no macOS.
#[cfg(target_os = "macos")]
pub(crate) fn global_path() -> Result<PathBuf, Error> {
    let _span = katu_core::trace_fn!("config::global_path");

    Ok(home()?.join("Library/Application Support/katu/katu.toml"))
}

/// Caminho da config global no Windows.
#[cfg(target_os = "windows")]
pub(crate) fn global_path() -> Result<PathBuf, Error> {
    let _span = katu_core::trace_fn!("config::global_path");

    let appdata = std::env::var_os("APPDATA").ok_or_else(|| {
        Error::invalid_input("APPDATA ausente: não sei onde fica a config global")
    })?;
    Ok(PathBuf::from(appdata).join("katu/katu.toml"))
}

/// Diretório pessoal (`HOME`, com queda para `USERPROFILE`).
fn home() -> Result<PathBuf, Error> {
    let _span = katu_core::trace_fn!("config::home");

    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .ok_or_else(|| Error::invalid_input("HOME ausente: não sei onde fica a config global"))
}

/// Diretório das travas padrão globais (`<config>/katu/guardrails`).
pub(crate) fn guardrails_dir() -> Result<PathBuf, Error> {
    let _span = katu_core::trace_fn!("config::guardrails_dir");

    let global = global_path()?;
    let parent = global
        .parent()
        .ok_or_else(|| Error::internal("caminho de config sem diretório"))?;
    Ok(parent.join("guardrails"))
}

#[cfg(test)]
mod tests;
