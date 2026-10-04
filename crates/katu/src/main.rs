//! `katu` — binário: CLI, composição e adaptador in-process do `knudge`.
//!
//! E01 define o esqueleto, os adaptadores das portas, o modelo de erro e o diagnóstico estruturado;
//! o wiring real chega com E04 (kernel). Toda a operação de topo abre um `span!` (DF9/E19).

#![deny(unsafe_code)]
#![allow(
    clippy::redundant_pub_crate,
    reason = "binário: sem API externa; os módulos internos usam pub(crate)"
)]

mod cli;

mod bootstrap;
mod config;

#[cfg(feature = "memory-in-process")]
mod defaults;

#[cfg(feature = "memory-in-process")]
mod agent;

#[cfg(feature = "memory-in-process")]
mod kernel;

#[cfg(feature = "profile")]
mod diag;

#[cfg(feature = "memory-in-process")]
mod login;

#[cfg(feature = "memory-in-process")]
mod memory;

#[cfg(feature = "memory-in-process")]
mod pricing;

#[cfg(feature = "memory-in-process")]
mod watch_service;

#[cfg_attr(
    not(feature = "memory-in-process"),
    allow(
        dead_code,
        reason = "sem agente/TUI (feature `memory-in-process`) os adaptadores de processo não têm consumidor"
    )
)]
mod ports;

mod report;

#[cfg(feature = "memory-in-process")]
mod runtime;

#[cfg(feature = "memory-in-process")]
mod scope;

#[cfg(feature = "memory-in-process")]
mod tier;

#[cfg(feature = "memory-in-process")]
mod tui;

use std::process::ExitCode;

use clap::Parser;
use katu_core::diag::{Level, events};

fn main() -> ExitCode {
    let _span = katu_core::trace_fn!("main");

    let cli = cli::Cli::parse();
    #[cfg(feature = "profile")]
    setup_diag(&cli);
    let _span = katu_core::fn_span!(Level::Info, events::KATU_RUN, "main::main");
    let report = cli::execute(&cli);
    let _shutdown = katu_core::fn_span!(Level::Info, events::KATU_SHUTDOWN, "main::shutdown");
    ExitCode::from(report::emit(&report, cli.json()))
}

/// Instala o diagnóstico estruturado conforme `--log-level` (DF9/E19).
///
/// `--log-level=quiet` (default) desliga; qualquer outro nível liga o sink de `stderr`. A variável
/// `KATU_INSTRUMENT` continua a forçar a ligação, e `KATU_INSTRUMENT_FILTER` restringe por prefixo.
#[cfg(feature = "profile")]
fn setup_diag(cli: &cli::Cli) {
    use std::sync::Arc;

    use katu_core::diag::{Sink, install, set_filter, set_level};
    use katu_core::ports::Env;

    let _span = katu_core::fn_span!(Level::Info, events::KATU_SETUP, "main::setup_diag");
    let env = ports::StdEnv;
    let requested = cli.log_level != cli::LogLevel::Quiet
        || env
            .var("KATU_INSTRUMENT")
            .is_some_and(|value| value == "1" || value == "true");
    if !requested {
        return;
    }
    let sink: Arc<dyn Sink> = Arc::new(diag::StderrSink);
    install(sink);
    set_level(level(cli.log_level));
    if let Some(prefix) = env.var("KATU_INSTRUMENT_FILTER") {
        set_filter(&prefix);
    }
}

/// Traduz o nível da borda para o nível do núcleo.
#[cfg(feature = "profile")]
const fn level(log: cli::LogLevel) -> Level {
    use cli::LogLevel;

    match log {
        LogLevel::Quiet | LogLevel::Error => Level::Error,
        LogLevel::Warn => Level::Warn,
        LogLevel::Info => Level::Info,
        LogLevel::Debug => Level::Debug,
        LogLevel::Trace => Level::Trace,
    }
}
