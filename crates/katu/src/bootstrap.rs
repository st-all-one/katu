//! Bootstrap do `.katu/` (E20-T19/T18): layout central, snapshot da config e versionamento.
//!
//! Idempotente: correr duas vezes não altera nada. A config do projeto é uma **cópia 1:1** da
//! global no arranque (ou o default, sem global). `audit/`, `trash/` e `log/` ficam **sempre**
//! fora do git; o resto segue `git.versioned` (default `true`).

use std::path::Path;

use katu_core::diag::{Level, events};
use katu_core::error::Error;
use katu_core::kernel::discover_root;

use crate::config;
use crate::ports::StdFs;

mod git;

/// Modo de versionamento pedido na linha de comandos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GitMode {
    /// Segue a config `git.versioned` (default `true`).
    Default,
    /// Exclui a árvore `.katu/` do git.
    Excluded,
    /// Força o versionamento do resto do `.katu/`.
    Tracked,
}

/// Resumo do bootstrap (o que foi criado/escrito).
#[allow(
    clippy::struct_excessive_bools,
    reason = "relatório do bootstrap: flags config/git"
)]
#[derive(Debug, Default)]
pub(crate) struct BootstrapReport {
    /// Subdiretórios criados (caminhos relativos ao `.katu/`).
    pub(crate) created: Vec<String>,
    /// Se a config do projeto foi escrita (snapshot).
    pub(crate) config: bool,
    /// Se os ficheiros de git foram atualizados.
    pub(crate) git: bool,
}

/// Subdiretórios do layout `.katu/`.
const LAYOUT: &[&str] = &[
    "audit",
    "knowledge",
    "knowledge/.idx",
    "guardrails",
    "trash",
    "log",
    "plan",
];

/// Conhecimento personalizado **preservado** por `--init --force`.
const PRESERVED: &[&str] = &["knowledge", "guardrails", "audit"];

/// Garante o layout do projeto a partir do diretório atual.
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "`force` é o flag `--init --force` do clap"
)]
pub(crate) fn ensure_current(mode: GitMode, force: bool) -> Result<BootstrapReport, Error> {
    let _span = katu_core::fn_span!(
        Level::Debug,
        events::BOOTSTRAP_INIT,
        "bootstrap::ensure_current"
    );
    let fs = StdFs;
    let start = std::env::current_dir().map_err(|err| Error::io(".", err))?;
    let root = discover_root(&fs, &start);
    ensure(&root, mode, force)
}

/// Garante o layout, o snapshot da config e o versionamento (idempotente).
///
/// Com `force`, **remove** tudo em `.katu/` exceto o conhecimento personalizado
/// ([`PRESERVED`]) e refaz o resto (config, trash, log, plan, sessions).
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "`force` é o flag `--init --force` do clap"
)]
pub(crate) fn ensure(root: &Path, mode: GitMode, force: bool) -> Result<BootstrapReport, Error> {
    let _span = katu_core::fn_span!(Level::Debug, events::BOOTSTRAP_INIT, "bootstrap::ensure");
    let base = root.join(".katu");
    let mut report = BootstrapReport::default();
    if force {
        remove_unpreserved(&base)?;
    }
    for dir in LAYOUT {
        let path = base.join(dir);
        if !path.is_dir() {
            std::fs::create_dir_all(&path)
                .map_err(|err| Error::io(path.display().to_string(), err))?;
            report.created.push((*dir).to_owned());
        }
    }
    report.config = ensure_config(root)?;
    copy_guardrails(root)?;
    report.git = git::ensure(root, mode)?;
    Ok(report)
}

/// Remove tudo em `.katu/` exceto o conhecimento personalizado ([`PRESERVED`]).
fn remove_unpreserved(base: &Path) -> Result<(), Error> {
    let _span = katu_core::fn_span!(
        Level::Trace,
        events::BOOTSTRAP_INIT,
        "bootstrap::remove_unpreserved"
    );
    if !base.is_dir() {
        return Ok(());
    }
    let entries =
        std::fs::read_dir(base).map_err(|err| Error::io(base.display().to_string(), err))?;
    for entry in entries {
        let entry = entry.map_err(|err| Error::io(base.display().to_string(), err))?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if PRESERVED.contains(&name.as_str()) {
            continue;
        }
        let path = entry.path();
        let result = if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        result.map_err(|err| Error::io(path.display().to_string(), err))?;
    }
    Ok(())
}

/// Escreve o snapshot da config do projeto se ainda não existir (1:1 da global, ou default).
fn ensure_config(root: &Path) -> Result<bool, Error> {
    let _span = katu_core::fn_span!(
        Level::Trace,
        events::BOOTSTRAP_INIT,
        "bootstrap::ensure_config"
    );
    let project = config::project_path(root);
    if project.exists() {
        return Ok(false);
    }
    match config::global_path() {
        Ok(global) if global.is_file() => {
            let bytes = std::fs::read(&global)
                .map_err(|err| Error::io(global.display().to_string(), err))?;
            if let Some(parent) = project.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|err| Error::io(parent.display().to_string(), err))?;
            }
            std::fs::write(&project, bytes)
                .map_err(|err| Error::io(project.display().to_string(), err))?;
        }
        _ => config::save(&project, &default_config())?,
    }
    Ok(true)
}

/// Copia as travas padrão globais para o projeto (só as que faltarem).
fn copy_guardrails(root: &Path) -> Result<(), Error> {
    let _span = katu_core::fn_span!(
        Level::Trace,
        events::BOOTSTRAP_INIT,
        "bootstrap::copy_guardrails"
    );
    let Ok(global) = config::guardrails_dir() else {
        return Ok(());
    };
    if !global.is_dir() {
        return Ok(());
    }
    let local = root.join(".katu").join("guardrails");
    let entries =
        std::fs::read_dir(&global).map_err(|err| Error::io(global.display().to_string(), err))?;
    for entry in entries {
        let entry = entry.map_err(|err| Error::io(global.display().to_string(), err))?;
        if !entry.file_type().is_ok_and(|kind| kind.is_file()) {
            continue;
        }
        let target = local.join(entry.file_name());
        if !target.exists() {
            std::fs::copy(entry.path(), &target)
                .map_err(|err| Error::io(target.display().to_string(), err))?;
        }
    }
    Ok(())
}

/// Config default (global ou snapshot do projeto).
pub(crate) fn default_config() -> toml::Table {
    let _span = katu_core::trace_fn!("bootstrap::default_config");

    let mut table = toml::Table::new();
    config::set_key(
        &mut table,
        "provider",
        toml::Value::String("llama".to_owned()),
    );
    // B1/W8-1: saída estruturada desligada por omissão (opt-in explícito).
    config::set_key(&mut table, "structured_output", toml::Value::Boolean(false));
    config::set_key(&mut table, "git.versioned", toml::Value::Boolean(true));
    config::set_key(
        &mut table,
        "memory.persist_in_project",
        toml::Value::Boolean(true),
    );
    config::set_key(
        &mut table,
        "behavior.auto_compact",
        toml::Value::Boolean(false),
    );
    config::set_key(
        &mut table,
        "behavior.prompt_state",
        toml::Value::Boolean(true),
    );
    config::set_key(
        &mut table,
        "behavior.context_selection",
        toml::Value::String("suffix".to_owned()),
    );
    // A3/W8-4: o gate de VOI fica **desligado** por omissão (exige A/B com o modelo); quando
    // ligado, só atua com a seleção `suffix` (com `utility` podia descartar a unidade lida).
    config::set_key(&mut table, "behavior.tool_voi", toml::Value::Boolean(false));
    // ADR 0024 (P-01): *group commit* por omissão (`turn`); `event` volta a sincronizar por evento.
    config::set_key(
        &mut table,
        "behavior.durability",
        toml::Value::String("turn".to_owned()),
    );
    config::set_key(&mut table, "recall.default_limit", toml::Value::Integer(5));
    table
}
