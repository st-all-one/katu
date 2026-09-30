//! Grupo `memo` (E20-T06): **consulta e visão geral** da memória, espelhando o `kd`.
//!
//! Nunca escreve: a escrita é do **agente** (tools no loop) e do `kd`. Sem o adaptador de memória,
//! `ask`/`doctor` recusam (fail-closed); `sessions`/`prime` continuam a funcionar.

use clap::{Args, Subcommand};
use katu_core::error::Error;
use serde_json::json;

use crate::report::Report;

#[cfg(feature = "memory-in-process")]
use crate::memory::commands::{memory_drain, memory_status};

#[cfg(feature = "memory-in-process")]
mod ask;
use super::prime::{self, Group};
use super::sessions;

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
#[derive(Debug, Clone, Subcommand)]
pub(crate) enum MemoCommand {
    /// Consulta a memória (recall pelo caminho §42).
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

/// Argumentos de `memo ask`.
#[derive(Debug, Clone, Args)]
pub(crate) struct AskArgs {
    /// Consulta (posicional = body; `-`/ausente lê `stdin`).
    pub(crate) query: Option<String>,
    /// Número máximo de resultados.
    #[arg(long)]
    pub(crate) limit: Option<usize>,
    /// Config universal do comando (JSON; XOR com as flags explícitas).
    #[arg(long)]
    pub(crate) params: Option<String>,
    /// Lote JSONL (uma linha = um item; XOR com `--params` e flags).
    #[arg(long)]
    pub(crate) batch: Option<String>,
}

/// Argumentos de `memo knowledge`.
#[derive(Debug, Clone, Args)]
pub(crate) struct KnowledgeArgs {
    /// Eixo do mapa (`tag`, `class`, `anchor`, …).
    #[arg(long)]
    pub(crate) axis: Option<String>,
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
    match &args.command {
        MemoCommand::Ask(args) => ask_report(args),
        MemoCommand::Knowledge(_) => knowledge(),
        MemoCommand::Doctor(doctor) => diagnose(doctor),
        MemoCommand::Sessions => sessions::list(),
        MemoCommand::Drain(args) => drain(args),
        MemoCommand::Prime { long } => prime::report(Group::Memo, *long),
    }
}

/// `memo ask`: resolve o body e consulta a memória pelo caminho §42.
#[cfg(feature = "memory-in-process")]
fn ask_report(args: &AskArgs) -> Report {
    ask::report(args)
}

/// Sem adaptador de memória, `ask` recusa (fail-closed).
#[cfg(not(feature = "memory-in-process"))]
fn ask_report(_args: &AskArgs) -> Report {
    Report::failed(
        "memo.ask",
        &Error::unavailable("adaptador de memória não compilado (feature `memory-in-process`)"),
    )
}

/// `memo knowledge`: visão geral do mapa (ainda não implementado — E20-T06).
fn knowledge() -> Report {
    Report::failed(
        "memo.knowledge",
        &Error::unavailable("memo knowledge ainda não implementado (E20-T06)"),
    )
}

/// `memo doctor`: diagnóstico do backend de memória.
#[cfg(feature = "memory-in-process")]
fn diagnose(args: &DoctorArgs) -> Report {
    use katu_core::diag;

    if args.fix {
        return fix_report();
    }
    let data = json!({
        "instrumented": diag::enabled(),
        "rust_version": env!("CARGO_PKG_RUST_VERSION"),
        "memory": memory_status(),
    });
    Report::ok("memo.doctor", Some(data))
}

/// Sem adaptador, `doctor` ainda reporta o ambiente (sem memória).
#[cfg(not(feature = "memory-in-process"))]
fn diagnose(args: &DoctorArgs) -> Report {
    use katu_core::diag;

    if args.fix {
        return fix_report();
    }
    let data = json!({
        "instrumented": diag::enabled(),
        "rust_version": env!("CARGO_PKG_RUST_VERSION"),
        "memory": { "error": "adaptador não compilado" },
    });
    Report::ok("memo.doctor", Some(data))
}

/// `memo doctor --fix`: garante o layout do projeto (idempotente, E20-T07).
fn fix_report() -> Report {
    use crate::bootstrap::{self, GitMode};

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
    Report::failed(
        "memo.drain",
        &Error::unavailable("memo drain exige --status, --digest ou --watch-service"),
    )
}
