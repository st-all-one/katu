//! `config` (E20-T09): configuração global única e override local.
//!
//! `get`/`list` leem a config **efetiva** (projeto > global); `--global` restringe à global.
//! `set`/`unset` escrevem no projeto por omissão, ou na global com `--global`. O conjunto de
//! chaves é **fechado** ([`crate::config::KEYS`]).

use std::path::PathBuf;

use clap::{Args, Subcommand};
use katu_core::error::Error;
use katu_core::kernel::discover_root;
use serde_json::json;

use crate::config;
use crate::ports::StdFs;
use crate::report::Report;

use super::prime::{self, Group};

/// Argumentos de `katu config`.
#[derive(Debug, Clone, Args)]
#[command(after_help = "Ciclo de uso:\n\
                  katu config list              configuração efetiva (projeto > global)\n\
                  katu config get <chave>       lê uma chave\n\
                  katu config set <chave> <v>   escreve no projeto (ou --global)\n\
                  katu config prime             contexto do grupo (igual a `katu prime --group config`)")]
pub(crate) struct ConfigArgs {
    /// Subcomando de `config`.
    #[command(subcommand)]
    pub(crate) command: Option<ConfigCommand>,
    /// Emite envelope JSON em `stdout`.
    #[arg(long, global = true)]
    pub(crate) json: bool,
}

/// Subcomandos de `config`.
#[derive(Debug, Clone, Subcommand)]
pub(crate) enum ConfigCommand {
    /// Lê uma chave (efetiva, ou global com `--global`).
    Get {
        /// Chave a ler.
        key: String,
        /// Lê a config global em vez da efetiva.
        #[arg(long)]
        global: bool,
    },
    /// Escreve uma chave (no projeto, ou global com `--global`).
    Set {
        /// Chave a escrever.
        key: String,
        /// Valor a atribuir.
        value: String,
        /// Escreve na config global em vez da do projeto.
        #[arg(long)]
        global: bool,
    },
    /// Remove uma chave.
    Unset {
        /// Chave a remover.
        key: String,
        /// Remove da config global em vez da do projeto.
        #[arg(long)]
        global: bool,
    },
    /// Lista as chaves presentes.
    List {
        /// Lista a config global em vez da efetiva.
        #[arg(long)]
        global: bool,
    },
    /// Contexto de arranque do grupo `config`.
    Prime {
        /// Prime longo.
        #[arg(long)]
        long: bool,
    },
}

/// Escopo da configuração.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// Config global (`~/.config/local/katu/katu.toml`).
    Global,
    /// Config do projeto (`<root>/.katu/katu.toml`).
    Project,
}

impl Scope {
    /// Do flag `--global` para o escopo.
    #[allow(
        clippy::fn_params_excessive_bools,
        reason = "`--global` é um flag booleano do clap"
    )]
    const fn new(global: bool) -> Self {
        if global { Self::Global } else { Self::Project }
    }

    /// Rótulo canônico.
    const fn name(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Project => "project",
        }
    }

    /// Caminho de escrita do escopo.
    fn path(self) -> Result<PathBuf, Error> {
        match self {
            Self::Global => config::global_path(),
            Self::Project => project_path(),
        }
    }

    /// Tabela do escopo (no projeto, funde o global por baixo).
    fn table(self) -> Result<toml::Table, Error> {
        match self {
            Self::Global => config::load(&config::global_path()?),
            Self::Project => {
                let mut table = config::load(&config::global_path()?)?;
                let over = config::load(&project_path()?)?;
                config::merge(&mut table, &over);
                Ok(table)
            }
        }
    }
}

/// Executa `katu config`.
pub(crate) fn execute(args: &ConfigArgs) -> Report {
    match &args.command {
        Some(ConfigCommand::Get { key, global }) => get(key, Scope::new(*global)),
        Some(ConfigCommand::Set { key, value, global }) => set(key, value, Scope::new(*global)),
        Some(ConfigCommand::Unset { key, global }) => unset(key, Scope::new(*global)),
        Some(ConfigCommand::List { global }) => list(Scope::new(*global)),
        Some(ConfigCommand::Prime { long }) => prime::report(Group::Config, *long),
        None => Report::failed(
            "config",
            &Error::invalid_input("subcomando em falta: get/set/unset/list"),
        ),
    }
}

/// Caminho da config do projeto (raiz descoberta a subir do diretório atual).
fn project_path() -> Result<PathBuf, Error> {
    let fs = StdFs;
    let start = std::env::current_dir().map_err(|err| Error::io(".", err))?;
    let root = discover_root(&fs, &start);
    Ok(config::project_path(&root))
}

/// `config get`.
fn get(key: &str, scope: Scope) -> Report {
    if config::find(key).is_none() {
        return Report::failed("config", &config::unknown_key(key));
    }
    let table = match scope.table() {
        Ok(table) => table,
        Err(error) => return Report::failed("config", &error),
    };
    match config::get_key(&table, key) {
        Some(value) => Report::ok(
            "config",
            Some(json!({
                "key": key,
                "value": config::display(&value),
                "scope": scope.name(),
            })),
        ),
        None => Report::failed(
            "config",
            &Error::invalid_input(format!("chave ausente: `{key}`")),
        ),
    }
}

/// `config set`.
fn set(key: &str, raw: &str, scope: Scope) -> Report {
    let Some(spec) = config::find(key) else {
        return Report::failed("config", &config::unknown_key(key));
    };
    let value = match config::parse_value(spec.kind, raw) {
        Ok(value) => value,
        Err(error) => return Report::failed("config", &error),
    };
    let path = match scope.path() {
        Ok(path) => path,
        Err(error) => return Report::failed("config", &error),
    };
    let mut table = match config::load(&path) {
        Ok(table) => table,
        Err(error) => return Report::failed("config", &error),
    };
    config::set_key(&mut table, key, value);
    match config::save(&path, &table) {
        Ok(()) => Report::ok(
            "config",
            Some(json!({ "key": key, "value": raw, "scope": scope.name() })),
        ),
        Err(error) => Report::failed("config", &error),
    }
}

/// `config unset`.
fn unset(key: &str, scope: Scope) -> Report {
    if config::find(key).is_none() {
        return Report::failed("config", &config::unknown_key(key));
    }
    let path = match scope.path() {
        Ok(path) => path,
        Err(error) => return Report::failed("config", &error),
    };
    let mut table = match config::load(&path) {
        Ok(table) => table,
        Err(error) => return Report::failed("config", &error),
    };
    let removed = config::unset_key(&mut table, key);
    if removed && let Err(error) = config::save(&path, &table) {
        return Report::failed("config", &error);
    }
    Report::ok(
        "config",
        Some(json!({ "key": key, "removed": removed, "scope": scope.name() })),
    )
}

/// `config list`.
fn list(scope: Scope) -> Report {
    let table = match scope.table() {
        Ok(table) => table,
        Err(error) => return Report::failed("config", &error),
    };
    let mut entries = serde_json::Map::new();
    for spec in config::KEYS {
        if let Some(value) = config::get_key(&table, spec.key) {
            entries.insert(spec.key.to_owned(), json!(config::display(&value)));
        }
    }
    Report::ok(
        "config",
        Some(json!({ "scope": scope.name(), "entries": entries })),
    )
}
