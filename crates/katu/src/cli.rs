//! Superfície de linha de comando (E01-T04).
//!
//! `clap` faz o parsing; aqui vive a **decisão** de cada subcomando. Nada de I/O de saída — isso é
//! de [`crate::report`].

use clap::{Parser, Subcommand};
use serde_json::{Value, json};

use katu_core::diag;

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
#[derive(Debug, Clone, Copy, Subcommand)]
pub(crate) enum Command {
    /// Imprime a versão e as versões de vocabulário.
    Version,
    /// Diagnóstico de portas, ambiente e instrumentação.
    Doctor,
}

impl Command {
    /// Nome estável do comando (para o envelope de máquina).
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Version => "version",
            Self::Doctor => "doctor",
        }
    }
}

/// Executa o comando pedido e devolve o relatório.
pub(crate) fn execute(cli: &Cli) -> Report {
    match cli.command {
        Command::Version => Report::ok(
            Command::Version.name(),
            Some(json!({
                "katu": env!("CARGO_PKG_VERSION"),
                "policy_vocab": katu_policy::POLICY_VOCAB_VERSION,
            })),
        ),
        Command::Doctor => doctor(),
    }
}

/// Diagnóstico de arranque: portas, instrumentação, MSRV efetivo e memória.
fn doctor() -> Report {
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

/// Estado do backend de memória (adaptador in-process do knudge, E03-T07).
#[cfg(feature = "memory-in-process")]
fn memory_status() -> Value {
    use crate::memory::KnudgeMemory;
    use katu_core::memory::Memory;

    let root = std::env::current_dir().unwrap_or_default();
    match KnudgeMemory::open(&root) {
        Ok(memory) => match memory.status() {
            Ok(status) => json!({
                "backend": status.backend,
                "health": format!("{:?}", status.health),
                "warnings": status.warnings,
                "knowledge_dir": memory.knowledge_dir().display().to_string(),
            }),
            Err(error) => json!({ "error": error.to_string() }),
        },
        Err(error) => json!({ "error": error.to_string() }),
    }
}
