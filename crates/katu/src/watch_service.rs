//! Worker de auto-drain (E20-T20): instala/remove o agendador que corre
//! `katu memo drain --digest` nos projetos subscritos.
//!
//! O agendamento usa um **timer systemd `--user`** (Linux). A materialização (script + unidades) é
//! feita com `std::fs` e a ativação via a porta `Process` (injetável nos testes). **Fail-closed**:
//! se o `systemctl` faltar ou falhar, o comando recusa com `unavailable` (exit 10).

use std::iter::once;
use std::path::{Path, PathBuf};

use katu_core::diag::{Level, events};
use katu_core::error::Error;
use katu_core::ports::{Env, ExecRequest, ExecResult, Process};
use serde_json::{Value, json};

use crate::ports::{StdEnv, StdProcess};
use crate::report::Report;

use store::{
    SCRIPT_NAME, SERVICE_NAME, TIMER_NAME, TIMER_UNIT, WATCHED_NAME, read_watched, remove_file,
    service_unit, write_file, write_script, write_watched,
};

mod store;

#[cfg(test)]
mod tests;

/// Timeout de cada chamada ao `systemctl`.
const SYSTEMCTL_TIMEOUT_MS: u64 = 15_000;

/// Ação pedida a `--watch-service` (default: `status`, read-only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    /// Reporta instalação + projetos subscritos.
    Status,
    /// Materializa e ativa o timer.
    Install,
    /// Subscreve o projeto atual.
    Subscribe,
    /// Remove a subscrição do projeto atual.
    Unsubscribe,
    /// Desativa e remove o timer.
    Uninstall,
}

/// Caminhos base do worker (derivados do ambiente).
struct Paths {
    base: PathBuf,
    systemd: PathBuf,
}

impl Paths {
    /// Deriva os caminhos de `XDG_DATA_HOME`/`XDG_CONFIG_HOME` (com fallback para `HOME`).
    fn from_env(env: &dyn Env) -> Self {
        let _span = katu_core::trace_fn!("watch_service::from_env");

        let base = env
            .var("XDG_DATA_HOME")
            .map_or_else(|| home(env).join(".local/share"), PathBuf::from)
            .join("katu");
        let systemd = env
            .var("XDG_CONFIG_HOME")
            .map_or_else(|| home(env).join(".config"), PathBuf::from)
            .join("systemd/user");
        Self { base, systemd }
    }

    fn script(&self) -> PathBuf {
        let _span = katu_core::trace_fn!("watch_service::script");

        self.base.join(SCRIPT_NAME)
    }

    fn watched(&self) -> PathBuf {
        let _span = katu_core::trace_fn!("watch_service::watched");

        self.base.join(WATCHED_NAME)
    }

    fn service(&self) -> PathBuf {
        let _span = katu_core::trace_fn!("watch_service::service");

        self.systemd.join(SERVICE_NAME)
    }

    fn timer(&self) -> PathBuf {
        let _span = katu_core::trace_fn!("watch_service::timer");

        self.systemd.join(TIMER_NAME)
    }
}

/// `HOME` com fallback determinístico (nunca vazio).
fn home(env: &dyn Env) -> PathBuf {
    let _span = katu_core::trace_fn!("watch_service::home");

    env.var("HOME")
        .map_or_else(|| PathBuf::from("."), PathBuf::from)
}

/// Contexto de uma ação (portas + caminhos + projeto).
struct Context<'a> {
    paths: &'a Paths,
    process: &'a dyn Process,
    env: &'a dyn Env,
    project: &'a Path,
}

/// Executa a ação pedida (borda real: `StdProcess`/`StdEnv`).
pub(crate) fn run(action: Action) -> Report {
    let _span = katu_core::fn_span!(Level::Debug, events::WATCH_TICK, "watch_service::run");
    let process = StdProcess;
    let env = StdEnv;
    let paths = Paths::from_env(&env);
    let project = std::env::current_dir().unwrap_or_default();
    let context = Context {
        paths: &paths,
        process: &process,
        env: &env,
        project: &project,
    };
    match dispatch(action, &context) {
        Ok(data) => Report::ok("memo.drain", Some(data)),
        Err(error) => Report::failed("memo.drain", &error),
    }
}

/// Despacha a ação para o handler correspondente.
fn dispatch(action: Action, context: &Context<'_>) -> Result<Value, Error> {
    let _span = katu_core::trace_fn!("watch_service::dispatch");

    match action {
        Action::Status => status(context),
        Action::Install => install(context),
        Action::Subscribe => subscribe(context),
        Action::Unsubscribe => unsubscribe(context),
        Action::Uninstall => uninstall(context),
    }
}

/// `--status`: instalação, estado do timer e projetos subscritos (read-only).
fn status(context: &Context<'_>) -> Result<Value, Error> {
    let _span = katu_core::fn_span!(Level::Trace, events::WATCH_TICK, "watch_service::status");
    let installed = context.paths.script().is_file() && context.paths.timer().is_file();
    Ok(json!({
        "action": "status",
        "installed": installed,
        "active": probe_active(context),
        "watched": read_watched(context.paths)?,
        "script": context.paths.script().display().to_string(),
        "timer": context.paths.timer().display().to_string(),
    }))
}

/// `--install`: materializa o script + unidades e ativa o timer.
fn install(context: &Context<'_>) -> Result<Value, Error> {
    let _span = katu_core::fn_span!(Level::Trace, events::WATCH_TICK, "watch_service::install");
    write_script(&context.paths.script())?;
    write_file(
        &context.paths.service(),
        &service_unit(&context.paths.script()),
    )?;
    write_file(&context.paths.timer(), TIMER_UNIT)?;
    systemctl(context, &["daemon-reload"])?;
    systemctl(context, &["enable", "--now", TIMER_NAME])?;
    Ok(json!({
        "action": "install",
        "installed": true,
        "script": context.paths.script().display().to_string(),
        "timer": context.paths.timer().display().to_string(),
        "watched": read_watched(context.paths)?,
    }))
}

/// `--subscribe`: adiciona o projeto atual à lista (idempotente).
fn subscribe(context: &Context<'_>) -> Result<Value, Error> {
    let _span = katu_core::fn_span!(Level::Trace, events::WATCH_TICK, "watch_service::subscribe");
    let mut watched = read_watched(context.paths)?;
    let root = context.project.display().to_string();
    if !watched.iter().any(|entry| entry == &root) {
        watched.push(root);
        write_watched(context.paths, &watched)?;
    }
    Ok(json!({ "action": "subscribe", "watched": watched }))
}

/// `--unsubscribe`: remove o projeto atual da lista.
fn unsubscribe(context: &Context<'_>) -> Result<Value, Error> {
    let _span = katu_core::fn_span!(
        Level::Trace,
        events::WATCH_TICK,
        "watch_service::unsubscribe"
    );
    let root = context.project.display().to_string();
    let watched: Vec<String> = read_watched(context.paths)?
        .into_iter()
        .filter(|entry| entry != &root)
        .collect();
    write_watched(context.paths, &watched)?;
    Ok(json!({ "action": "unsubscribe", "watched": watched }))
}

/// `--uninstall`: desativa e remove o timer + script (mantém a lista de subscrições).
fn uninstall(context: &Context<'_>) -> Result<Value, Error> {
    let _span = katu_core::fn_span!(Level::Trace, events::WATCH_TICK, "watch_service::uninstall");
    if context.paths.timer().is_file() {
        systemctl(context, &["disable", "--now", TIMER_NAME])?;
    }
    remove_file(&context.paths.service())?;
    remove_file(&context.paths.timer())?;
    remove_file(&context.paths.script())?;
    drop(systemctl(context, &["daemon-reload"]));
    Ok(json!({ "action": "uninstall", "installed": false }))
}

/// Corre `systemctl --user <args>` e recusa em erro (fail-closed).
fn systemctl(context: &Context<'_>, args: &[&str]) -> Result<ExecResult, Error> {
    let _span = katu_core::trace_fn!("watch_service::systemctl");

    let result = context
        .process
        .run(&systemctl_request(context, args))
        .map_err(|error| Error::unavailable(format!("systemctl indisponível: {error:?}")))?;
    if result.exit_code != Some(0) {
        return Err(Error::unavailable(format!(
            "systemctl {} falhou: {}",
            args.join(" "),
            result.stderr.trim()
        )));
    }
    Ok(result)
}

/// `systemctl --user is-active <timer>` como texto (`unknown` se indisponível).
fn probe_active(context: &Context<'_>) -> String {
    let _span = katu_core::trace_fn!("watch_service::probe_active");

    match context
        .process
        .run(&systemctl_request(context, &["is-active", TIMER_NAME]))
    {
        Ok(result) => result.stdout.trim().to_string(),
        Err(_) => "unknown".to_string(),
    }
}

/// Pedido de execução para o `systemctl --user`.
fn systemctl_request(context: &Context<'_>, args: &[&str]) -> ExecRequest {
    let _span = katu_core::trace_fn!("watch_service::systemctl_request");

    ExecRequest {
        argv: once("systemctl")
            .chain(once("--user"))
            .chain(args.iter().copied())
            .map(str::to_string)
            .collect(),
        cwd: context.project.to_path_buf(),
        env: systemctl_env(context.env),
        timeout_ms: SYSTEMCTL_TIMEOUT_MS,
    }
}

/// Ambiente mínimo para o `systemctl --user` (sem segredos).
fn systemctl_env(env: &dyn Env) -> Vec<(String, String)> {
    const KEYS: &[&str] = &[
        "HOME",
        "PATH",
        "XDG_RUNTIME_DIR",
        "DBUS_SESSION_BUS_ADDRESS",
    ];
    KEYS.iter()
        .filter_map(|key| env.var(key).map(|value| ((*key).to_string(), value)))
        .collect()
}
