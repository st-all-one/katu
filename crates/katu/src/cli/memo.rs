//! Grupo `memo` (E20-T06): **consulta e visão geral** da memória, espelhando o `kd`.
//!
//! Nunca escreve: a escrita é do **agente** (tools no loop) e do `kd`. Sem o adaptador de memória,
//! `ask`/`doctor` recusam (fail-closed); `sessions`/`prime` continuam a funcionar.

use clap::{Args, Subcommand};
use katu_core::diag::{Level, events};
use katu_core::error::Error;
use serde_json::json;

use crate::defaults;
use crate::report::Report;

#[cfg(feature = "memory-in-process")]
use crate::memory::commands::{memory_drain, memory_status};

mod args;
#[cfg(feature = "memory-in-process")]
mod ask;
use super::prime::{self, Group};
use super::sessions;
use args::{AskArgs, KnowledgeArgs};

/// Argumentos de `katu memo`.
#[derive(Debug, Clone, Args)]
#[command(after_help = "Ciclo de uso:\n\
                  katu memo ask \"...\"        consulta (recall §42)\n\
                  katu memo doctor --fix        repara o layout do projeto\n\
                  katu memo sessions            sessões do projeto\n\
                  katu memo prime               contexto do grupo (igual a `katu prime --group memo`)")]
pub(crate) struct MemoArgs {
    /// Subcomando de `memo`.
    #[command(subcommand)]
    pub(crate) command: MemoCommand,
    /// Emite envelope JSON em `stdout`.
    #[arg(long, global = true)]
    pub(crate) json: bool,
}

/// Subcomandos de `memo` (só consulta).
#[allow(
    clippy::large_enum_variant,
    reason = "args de clap; boxear complicaria a derivação do subcomando"
)]
#[derive(Debug, Clone, Subcommand)]
pub(crate) enum MemoCommand {
    /// Consulta rica à memória (espelha `kd ask`; E20-T06).
    Ask(AskArgs),
    /// Visão geral do mapa de conhecimento.
    Knowledge(KnowledgeArgs),
    /// Diagnóstico do backend de memória.
    Doctor(DoctorArgs),
    /// Lista as sessões do projeto.
    Sessions,
    /// Drena/reconcilia o índice de conhecimento.
    Drain(DrainArgs),
    /// Contexto de arranque do grupo `memo`.
    Prime {
        /// Prime longo.
        #[arg(long)]
        long: bool,
    },
}

/// Argumentos de `memo doctor`.
#[derive(Debug, Clone, Args)]
pub(crate) struct DoctorArgs {
    /// Repara o que for reparável (idempotente).
    #[arg(long)]
    pub(crate) fix: bool,
}

/// Argumentos de `memo drain`.
#[allow(
    clippy::struct_excessive_bools,
    reason = "flags de clap do drain (modos + ações do worker)"
)]
#[derive(Debug, Clone, Args)]
pub(crate) struct DrainArgs {
    /// Mostra o estado sem tocar no índice.
    #[arg(long, conflicts_with = "digest")]
    pub(crate) status: bool,
    /// Digere o índice (drena a fila de embeddings).
    #[arg(long, conflicts_with = "watch_service")]
    pub(crate) digest: bool,
    /// Apaga `.idx/` (100% derivado) e redigeri tudo do zero (exige `--digest`).
    #[arg(long, requires = "digest")]
    pub(crate) force: bool,
    /// Gere o worker de auto-drain (E20-T20; ainda fail-closed).
    #[arg(long)]
    pub(crate) watch_service: bool,
    /// Instala o worker de auto-drain.
    #[arg(long, requires = "watch_service")]
    pub(crate) install: bool,
    /// Subscreve o projeto atual ao worker.
    #[arg(long, requires = "watch_service")]
    pub(crate) subscribe: bool,
    /// Remove a subscrição do projeto atual.
    #[arg(long, requires = "watch_service")]
    pub(crate) unsubscribe: bool,
    /// Desinstala o worker de auto-drain.
    #[arg(long, requires = "watch_service")]
    pub(crate) uninstall: bool,
}

/// Executa `katu memo`.
pub(crate) fn execute(args: &MemoArgs) -> Report {
    let _span = katu_core::fn_span!(Level::Debug, events::CLI_MEMO, "memo::execute");
    match &args.command {
        MemoCommand::Ask(args) => ask_report(args),
        MemoCommand::Knowledge(args) => knowledge_report(args),
        MemoCommand::Doctor(doctor) => diagnose(doctor),
        MemoCommand::Sessions => sessions::list(),
        MemoCommand::Drain(args) => drain(args),
        MemoCommand::Prime { long } => prime::report(Group::Memo, *long),
    }
}

/// `memo ask`: resolve a consulta rica (E20-T06).
#[cfg(feature = "memory-in-process")]
fn ask_report(args: &AskArgs) -> Report {
    let _span = katu_core::trace_fn!("cli::memo::ask_report");

    ask::report(args)
}

/// Sem adaptador de memória, `ask` recusa (fail-closed).
#[cfg(not(feature = "memory-in-process"))]
fn ask_report(_args: &AskArgs) -> Report {
    let _span = katu_core::trace_fn!("cli::memo::ask_report");

    Report::failed(
        "memo.ask",
        &Error::unavailable("adaptador de memória não compilado (feature `memory-in-process`)"),
    )
}

/// `memo knowledge`: visão geral do mapa estrutural (E20-T06).
#[cfg(feature = "memory-in-process")]
fn knowledge_report(args: &KnowledgeArgs) -> Report {
    let _span = katu_core::trace_fn!("cli::memo::knowledge_report");

    ask::knowledge(args)
}

/// Sem adaptador de memória, `knowledge` recusa (fail-closed).
#[cfg(not(feature = "memory-in-process"))]
fn knowledge_report(_args: &KnowledgeArgs) -> Report {
    let _span = katu_core::trace_fn!("cli::memo::knowledge_report");

    Report::failed(
        "memo.knowledge",
        &Error::unavailable("adaptador de memória não compilado (feature `memory-in-process`)"),
    )
}

/// Estado do serviço de embeddings (E20-T17): a **segunda IA**, externa e plugável.
fn embeddings_status() -> serde_json::Value {
    let _span = katu_core::trace_fn!("cli::memo::embeddings_status");

    let embeddings = defaults::current().embeddings;
    let enabled = embeddings.url.as_deref().is_some_and(|url| !url.is_empty());
    json!({
        "enabled": enabled,
        "url": embeddings.url,
        "model": embeddings.model,
        "command": embeddings.command,
    })
}

/// `memo doctor`: diagnóstico do backend de memória.
#[cfg(feature = "memory-in-process")]
fn diagnose(args: &DoctorArgs) -> Report {
    use katu_core::diag;

    let _span = katu_core::fn_span!(Level::Debug, events::CLI_MEMO, "memo::diagnose");
    if args.fix {
        return fix_report();
    }
    let data = json!({
        "instrumented": diag::enabled(),
        "rust_version": env!("CARGO_PKG_RUST_VERSION"),
        "memory": memory_status(),
        "embeddings": embeddings_status(),
    });
    Report::ok("memo.doctor", Some(data))
}

/// Sem adaptador, `doctor` ainda reporta o ambiente (sem memória).
#[cfg(not(feature = "memory-in-process"))]
fn diagnose(args: &DoctorArgs) -> Report {
    use katu_core::diag;

    let _span = katu_core::fn_span!(Level::Debug, events::CLI_MEMO, "memo::diagnose");
    if args.fix {
        return fix_report();
    }
    let data = json!({
        "instrumented": diag::enabled(),
        "rust_version": env!("CARGO_PKG_RUST_VERSION"),
        "memory": { "error": "adaptador não compilado" },
        "embeddings": embeddings_status(),
    });
    Report::ok("memo.doctor", Some(data))
}

/// `memo doctor --fix`: garante o layout do projeto (idempotente, E20-T07).
fn fix_report() -> Report {
    use crate::bootstrap::{self, GitMode};

    let _span = katu_core::fn_span!(Level::Debug, events::BOOTSTRAP_FIX, "memo::fix_report");
    match bootstrap::ensure_current(GitMode::Default, false) {
        Ok(report) => Report::ok(
            "memo.doctor",
            Some(json!({
                "fixed": report.created,
                "config": report.config,
                "git": report.git,
            })),
        ),
        Err(error) => Report::failed("memo.doctor", &error),
    }
}

/// `memo drain --status`/`--digest`: estado sem tocar no índice ou dreno real (E20-T20).
#[cfg(feature = "memory-in-process")]
fn drain(args: &DrainArgs) -> Report {
    let _span = katu_core::fn_span!(Level::Debug, events::WATCH_DRAIN, "memo::drain");
    if args.watch_service {
        return watch_action(args);
    }
    if args.status {
        return Report::ok("memo.drain", Some(json!({ "status": memory_status() })));
    }
    if args.digest {
        return memory_drain(args.force);
    }
    drain_unavailable()
}

/// `--watch-service`: instala/remove o worker de auto-drain (E20-T20).
#[cfg(feature = "memory-in-process")]
fn watch_action(args: &DrainArgs) -> Report {
    use crate::watch_service::{self, Action};
    let _span = katu_core::fn_span!(Level::Debug, events::WATCH_TICK, "memo::watch_action");
    let action = if args.install {
        Action::Install
    } else if args.subscribe {
        Action::Subscribe
    } else if args.unsubscribe {
        Action::Unsubscribe
    } else if args.uninstall {
        Action::Uninstall
    } else {
        Action::Status
    };
    watch_service::run(action)
}

/// Sem adaptador, `drain` recusa (fail-closed).
#[cfg(not(feature = "memory-in-process"))]
fn drain(args: &DrainArgs) -> Report {
    let _span = katu_core::trace_fn!("cli::memo::drain");

    if args.status {
        return Report::failed(
            "memo.drain",
            &Error::unavailable("adaptador de memória não compilado (feature `memory-in-process`)"),
        );
    }
    drain_unavailable()
}

/// `drain` sem modo é help.
fn drain_unavailable() -> Report {
    let _span = katu_core::trace_fn!("cli::memo::drain_unavailable");

    Report::failed(
        "memo.drain",
        &Error::unavailable("memo drain exige --status, --digest ou --watch-service"),
    )
}
