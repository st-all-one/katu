//! `katu` — binário: CLI, composição e adaptador in-process do `knudge`.
//!
//! E01 define o esqueleto, os adaptadores das portas, o modelo de erro e o diagnóstico estruturado;
//! o wiring real chega com E04 (kernel). Toda a operação de topo abre um `span!` (DF9/E19).

#![forbid(unsafe_code)]
#![allow(
    clippy::redundant_pub_crate,
    reason = "binário: sem API externa; os módulos internos usam pub(crate)"
)]

mod cli;

#[cfg(feature = "memory-in-process")]
mod agent;

#[cfg(feature = "profile")]
mod diag;

#[cfg(feature = "memory-in-process")]
mod memory;

#[allow(dead_code, reason = "adaptadores ligados ao kernel em E04")]
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
    #[cfg(feature = "profile")]
    setup_diag();
    let _span = katu_core::span!(Level::Info, events::KATU_RUN);
    let cli = cli::Cli::parse();
    let report = cli::execute(&cli);
    ExitCode::from(report::emit(&report, cli.json))
}

/// Instala o diagnóstico estruturado quando `KATU_INSTRUMENT` o pede (DF9/E19).
#[cfg(feature = "profile")]
fn setup_diag() {
    use std::sync::Arc;

    use katu_core::diag::{Sink, install, set_filter};
    use katu_core::ports::Env;

    let _span = katu_core::span!(Level::Info, events::KATU_SETUP);
    let env = ports::StdEnv;
    let requested = env
        .var("KATU_INSTRUMENT")
        .is_some_and(|value| value == "1" || value == "true");
    if requested {
        let sink: Arc<dyn Sink> = Arc::new(diag::StderrSink);
        install(sink);
        if let Some(prefix) = env.var("KATU_INSTRUMENT_FILTER") {
            set_filter(&prefix);
        }
    }
}
