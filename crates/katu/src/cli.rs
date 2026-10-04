//! Superfície de linha de comando (E01-T04, E20): verbos exclusivos, sem inferência.
//!
//! `clap` faz o parsing; aqui vive a **decisão** de cada verbo. Nada de I/O de saída — isso é de
//! [`crate::report`]. Sem subcomando, `katu` abre a TUI; `--init` faz o bootstrap e sai.

use clap::{Parser, Subcommand};
use katu_core::diag::{Level, events};
use serde_json::json;

use crate::bootstrap::{self, GitMode};
use crate::report::Report;

mod config_cmd;
#[cfg(feature = "memory-in-process")]
mod input;
mod memo;
mod params;
mod prime;
#[cfg(feature = "memory-in-process")]
mod run_cmd;
#[cfg(feature = "memory-in-process")]
mod run_params;
mod sessions;
#[cfg(test)]
mod tests;
mod upgrade;

/// Nível de log emitido em `stderr` (default `quiet`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum LogLevel {
    /// Sem logs.
    Quiet,
    /// Só erros.
    Error,
    /// Erros e avisos.
    Warn,
    /// Erros, avisos e informação.
    Info,
    /// Inclui depuração.
    Debug,
    /// Inclui rastreio fino.
    Trace,
}

/// `katu` — loop possuído, política e memória.
#[allow(
    clippy::struct_excessive_bools,
    reason = "flags de bootstrap do clap (--init/--git-excluded/--git-tracked)"
)]
#[derive(Debug, Parser)]
#[command(
    name = "katu",
    version,
    about = "katu — agente de código com loop possuído",
    long_about = "katu — agente de código com loop possuído, política e memória.\n\n\
                  Verbos exclusivos: `prime`, `upgrade`, `config`, `memo`, `run`, `tui`. Sem\n\
                  subcomando abre a TUI; `--init` faz o bootstrap e sai.",
    after_help = "Ciclo de uso:\n\
                  katu prime            contexto de arranque (estático)\n\
                  katu --init           bootstrap do projeto (.katu/)\n\
                  katu                  abre a TUI (multi-turno)\n\
                  katu run \"...\"        uma rodada: id da sessão + exit code\n\
                  katu memo ask \"...\"   consulta a memória (só leitura)\n\
                  katu config list      configuração efetiva (projeto > global)\n\n\
                  Use `katu help <verbo>` para o detalhe de cada verbo."
)]
pub(crate) struct Cli {
    /// Nível de log em `stderr` (default `quiet`).
    #[arg(long, global = true, default_value = "quiet")]
    pub(crate) log_level: LogLevel,
    /// Faz o bootstrap do `.katu/` e sai.
    #[arg(long)]
    pub(crate) init: bool,
    /// Com `--init`, exclui o `.katu/` do git.
    #[arg(long, requires = "init")]
    pub(crate) git_excluded: bool,
    /// Com `--init`, versiona o `.katu/`.
    #[arg(long, requires = "init", conflicts_with = "git_excluded")]
    pub(crate) git_tracked: bool,
    /// Com `--init`, refaz o bootstrap preservando só o conhecimento personalizado
    /// (`knowledge/`, `guardrails/`, `audit/`).
    #[arg(long, requires = "init")]
    pub(crate) force: bool,
    /// Subcomando a executar (sem subcomando, abre a TUI).
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

/// Verbos do `katu`.
#[allow(
    clippy::large_enum_variant,
    reason = "args de clap; boxear complicaria a derivação do subcomando"
)]
#[derive(Debug, Clone, Subcommand)]
pub(crate) enum Command {
    /// Contexto de arranque estático para IA.
    Prime(prime::PrimeArgs),
    /// Sincronização de versão (canal ainda não configurado).
    Upgrade(upgrade::UpgradeArgs),
    /// Configuração global e do projeto.
    Config(config_cmd::ConfigArgs),
    /// Memória: consulta e visão geral (sem escrita).
    Memo(memo::MemoArgs),
    /// Executa uma rodada e sai (id da sessão + exit code).
    #[cfg(feature = "memory-in-process")]
    Run(run_cmd::RunCli),
    /// Abre a UI de terminal.
    #[cfg(feature = "memory-in-process")]
    Tui(run_cmd::TuiCli),
}

impl Cli {
    /// Se o comando pede envelope JSON em `stdout` (**por comando**, não global).
    pub(crate) fn json(&self) -> bool {
        let _span = katu_core::trace_fn!("cli::json");

        match &self.command {
            Some(command) => command.json(),
            None => false,
        }
    }
}

impl Command {
    /// Se o verbo pede envelope JSON.
    pub(crate) fn json(&self) -> bool {
        let _span = katu_core::trace_fn!("cli::command::json");

        match self {
            Self::Prime(args) => args.json,
            Self::Upgrade(args) => args.json,
            Self::Config(args) => args.json,
            Self::Memo(args) => args.json,
            #[cfg(feature = "memory-in-process")]
            Self::Run(args) => args.wants_json(),
            #[cfg(feature = "memory-in-process")]
            Self::Tui(_) => false,
        }
    }
}

/// Executa o comando pedido e devolve o relatório.
pub(crate) fn execute(cli: &Cli) -> Report {
    let _span = katu_core::trace_fn!("cli::execute");

    if cli.init {
        return init_project(cli);
    }
    match &cli.command {
        Some(Command::Prime(args)) => prime::execute(args),
        Some(Command::Upgrade(args)) => upgrade::execute(args),
        Some(Command::Config(args)) => config_cmd::execute(args),
        Some(Command::Memo(args)) => memo::execute(args),
        #[cfg(feature = "memory-in-process")]
        Some(Command::Run(args)) => {
            ensure_project("run").unwrap_or_else(|| run_cmd::execute_run(args))
        }
        #[cfg(feature = "memory-in-process")]
        Some(Command::Tui(args)) => {
            ensure_project("tui").unwrap_or_else(|| run_cmd::execute_tui(args))
        }
        None => default_tui(),
    }
}

/// Garante o `.katu/` antes de abrir sessão; devolve um relatório de erro, se falhar.
#[cfg(feature = "memory-in-process")]
fn ensure_project(command: &'static str) -> Option<Report> {
    let _span = katu_core::trace_fn!("cli::ensure_project");

    bootstrap::ensure_current(GitMode::Default, false)
        .err()
        .map(|error| Report::failed(command, &error))
}

/// `katu --init`: bootstrap do projeto e saída (E20-T19).
fn init_project(cli: &Cli) -> Report {
    let _span = katu_core::fn_span!(Level::Debug, events::BOOTSTRAP_INIT, "cli::init_project");
    let mode = if cli.git_excluded {
        GitMode::Excluded
    } else if cli.git_tracked {
        GitMode::Tracked
    } else {
        GitMode::Default
    };
    match bootstrap::ensure_current(mode, cli.force) {
        Ok(report) => Report::ok(
            "init",
            Some(json!({
                "created": report.created,
                "config": report.config,
                "git": report.git,
            })),
        ),
        Err(error) => Report::failed("init", &error),
    }
}

/// `katu` sem subcomando: abre a TUI (E20-T03), falhando fechado sem TTY.
#[cfg(feature = "memory-in-process")]
fn default_tui() -> Report {
    use std::io::IsTerminal;

    use katu_core::error::Error;

    let _span = katu_core::fn_span!(Level::Debug, events::CLI_TUI, "cli::default_tui");
    if !std::io::stdout().is_terminal() {
        return Report::failed(
            "tui",
            &Error::io("<stdout>", std::io::Error::other("sem TTY")),
        );
    }
    if let Some(report) = ensure_project("tui") {
        return report;
    }
    run_cmd::execute_tui(&run_cmd::TuiCli::default())
}

/// Sem o adaptador, não há TUI.
#[cfg(not(feature = "memory-in-process"))]
fn default_tui() -> Report {
    use katu_core::error::Error;

    Report::failed(
        "tui",
        &Error::unavailable("TUI não compilada (feature `memory-in-process`)"),
    )
}
