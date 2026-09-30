//! Superfície de linha de comando (E01-T04).
//!
//! `clap` faz o parsing; aqui vive a **decisão** de cada subcomando. Nada de I/O de saída — isso é
//! de [`crate::report`]. Os comandos `recall`/`remember` montam o **runtime** (E03-T03) e passam
//! pelo caminho §42; sem o adaptador de memória **recusam** (fail-closed, E03-T07).

use clap::{Parser, Subcommand};
use serde_json::{Value, json};

use katu_core::diag;
use katu_policy::POLICY_VOCAB_VERSION;

#[cfg(feature = "memory-in-process")]
use crate::agent::{RunArgs, run};
#[cfg(feature = "memory-in-process")]
use crate::memory::commands::{memory_command, memory_recall, memory_remember, memory_status};
#[cfg(feature = "memory-in-process")]
use crate::tui::run_tui;

use crate::report::Report;

/// `katu` — loop possuído, política e memória.
#[derive(Debug, Parser)]
#[command(name = "katu", version, about, long_about = None, arg_required_else_help = true)]
pub(crate) struct Cli {
    /// Emite envelope JSON em `stdout` (dados) em vez de texto humano.
    #[arg(long, global = true)]
    pub(crate) json: bool,
    /// Subcomando a executar.
    #[command(subcommand)]
    pub(crate) command: Command,
}

/// Subcomandos do `katu`.
#[derive(Debug, Clone, Subcommand)]
pub(crate) enum Command {
    /// Imprime a versão e as versões de vocabulário.
    Version,
    /// Diagnóstico de portas, ambiente e instrumentação.
    Doctor,
    /// Estado da memória de primeira classe (fail-closed sem adaptador).
    Memory,
    /// Consulta a memória (recall pelo caminho §42).
    Recall {
        /// Consulta em linguagem natural.
        query: String,
        /// Número máximo de resultados.
        #[arg(long, default_value_t = 5)]
        limit: usize,
    },
    /// Regista uma nota (recall antes da escrita, pelo gate §42).
    Remember {
        /// Afirmação (uma por nota).
        statement: String,
        /// Âncora de código (ex.: `src/x.rs`), quando a nota é sobre código.
        #[arg(long)]
        anchor: Option<String>,
    },
    /// Executa um turno do agente (provider ↔ kernel ↔ tools).
    #[cfg(feature = "memory-in-process")]
    Run {
        /// Objetivo/mensagem do utilizador.
        goal: String,
        /// Provider (`llama`, `opencode-go`, `opencode-zen`).
        #[arg(long, default_value = "llama")]
        provider: String,
        /// Modelo (por omissão depende do provider).
        #[arg(long)]
        model: Option<String>,
        /// Base URL do endpoint (por omissão depende do provider).
        #[arg(long)]
        base: Option<String>,
        /// Teto de tokens de saída.
        #[arg(long, default_value_t = 512)]
        max_tokens: u32,
        /// Máximo de passos (tool calls) por turno.
        #[arg(long, default_value_t = 8)]
        max_steps: u32,
        /// Liga a compactação do histórico no turno (E09-T07).
        #[arg(long)]
        compact: bool,
    },
    /// Abre a UI de terminal sobre o loop de turnos (E10).
    #[cfg(feature = "memory-in-process")]
    Tui {
        /// Provider (`llama`, `opencode-go`, `opencode-zen`).
        #[arg(long, default_value = "llama")]
        provider: String,
        /// Modelo (por omissão depende do provider).
        #[arg(long)]
        model: Option<String>,
        /// Base URL do endpoint (por omissão depende do provider).
        #[arg(long)]
        base: Option<String>,
        /// Teto de tokens de saída.
        #[arg(long, default_value_t = 512)]
        max_tokens: u32,
        /// Máximo de passos (tool calls) por turno.
        #[arg(long, default_value_t = 8)]
        max_steps: u32,
        /// Liga a compactação do histórico no turno (E09-T07).
        #[arg(long)]
        compact: bool,
    },
}

impl Command {
    /// Nome estável do comando (para o envelope de máquina).
    pub(crate) const fn name(&self) -> &'static str {
        match self {
            Self::Version => "version",
            Self::Doctor => "doctor",
            Self::Memory => "memory",
            Self::Recall { .. } => "recall",
            Self::Remember { .. } => "remember",
            #[cfg(feature = "memory-in-process")]
            Self::Run { .. } => "run",
            #[cfg(feature = "memory-in-process")]
            Self::Tui { .. } => "tui",
        }
    }
}

/// Executa o comando pedido e devolve o relatório.
pub(crate) fn execute(cli: &Cli) -> Report {
    match &cli.command {
        Command::Version => Report::ok(
            Command::Version.name(),
            Some(json!({
                "katu": env!("CARGO_PKG_VERSION"),
                "policy_vocab": POLICY_VOCAB_VERSION,
            })),
        ),
        Command::Doctor => doctor(),
        Command::Memory => memory_command(),
        Command::Recall { query, limit } => memory_recall(query, *limit),
        Command::Remember { statement, anchor } => memory_remember(statement, anchor.as_deref()),
        #[cfg(feature = "memory-in-process")]
        Command::Run {
            goal,
            provider,
            model,
            base,
            max_tokens,
            max_steps,
            compact,
        } => run(&RunArgs {
            goal,
            provider,
            model: model.as_deref(),
            base: base.as_deref(),
            max_tokens: *max_tokens,
            max_steps: *max_steps,
            compact: *compact,
        }),
        #[cfg(feature = "memory-in-process")]
        Command::Tui {
            provider,
            model,
            base,
            max_tokens,
            max_steps,
            compact,
        } => run_tui(&RunArgs {
            goal: "tui",
            provider,
            model: model.as_deref(),
            base: base.as_deref(),
            max_tokens: *max_tokens,
            max_steps: *max_steps,
            compact: *compact,
        }),
    }
}

/// Diagnóstico de arranque: portas, instrumentação, MSRV efetivo e memória.
fn doctor() -> Report {
    #[cfg_attr(
        not(feature = "memory-in-process"),
        allow(unused_mut, reason = "sem o adaptador, `data` não é mutado")
    )]
    let mut data: Value = json!({
        "instrumented": diag::enabled(),
        "rust_version": env!("CARGO_PKG_RUST_VERSION"),
    });
    #[cfg(feature = "memory-in-process")]
    if let Some(object) = data.as_object_mut() {
        object.insert("memory".to_string(), memory_status());
    }
    Report::ok(Command::Doctor.name(), Some(data))
}

/// Sem o adaptador compilado, a memória é invariante: os comandos **recusam** (DF4, fail-closed).
#[cfg(not(feature = "memory-in-process"))]
fn memory_command() -> Report {
    unavailable_report(Command::Memory.name())
}

#[cfg(not(feature = "memory-in-process"))]
fn memory_recall(_query: &str, _limit: usize) -> Report {
    unavailable_report("recall")
}

#[cfg(not(feature = "memory-in-process"))]
fn memory_remember(_statement: &str, _anchor: Option<&str>) -> Report {
    unavailable_report("remember")
}

#[cfg(not(feature = "memory-in-process"))]
fn unavailable_report(command: &'static str) -> Report {
    use katu_core::error::Error;

    Report::failed(
        command,
        &Error::unavailable("adaptador de memória não compilado (feature `memory-in-process`)"),
    )
}

#[cfg(test)]
mod tests {
    use super::{Cli, Command, execute};
    use clap::Parser;

    #[test]
    fn version_report_is_ok() {
        let cli = Cli::parse_from(["katu", "version", "--json"]);
        let report = execute(&cli);
        assert!(report.success);
        assert_eq!(report.command, "version");
    }

    #[test]
    fn command_names_are_stable() {
        assert_eq!(Command::Version.name(), "version");
        assert_eq!(Command::Doctor.name(), "doctor");
        assert_eq!(Command::Memory.name(), "memory");
        assert_eq!(
            Command::Recall {
                query: String::new(),
                limit: 5
            }
            .name(),
            "recall"
        );
        assert_eq!(
            Command::Remember {
                statement: String::new(),
                anchor: None
            }
            .name(),
            "remember"
        );
    }
}
