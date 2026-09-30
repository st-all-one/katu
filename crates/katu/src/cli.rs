//! Superfície de linha de comando (E01-T04).
//!
//! `clap` faz o parsing; aqui vive a **decisão** de cada subcomando. Nada de I/O de saída — isso é
//! de [`crate::report`]. Os comandos `recall`/`remember` montam o **runtime** (E03-T03) e passam
//! pelo caminho §42; sem o adaptador de memória **recusam** (fail-closed, E03-T07).

use clap::{Parser, Subcommand};
use serde_json::{Value, json};

use katu_core::diag;

#[cfg(feature = "memory-in-process")]
use crate::runtime::{Runtime, RuntimeError};
#[cfg(feature = "memory-in-process")]
use katu_core::error::Error;
#[cfg(feature = "memory-in-process")]
use katu_core::kernel::Dispatch;

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
                "policy_vocab": katu_policy::POLICY_VOCAB_VERSION,
            })),
        ),
        Command::Doctor => doctor(),
        Command::Memory => memory_command(),
        Command::Recall { query, limit } => memory_recall(query, *limit),
        Command::Remember { statement, anchor } => memory_remember(statement, anchor.as_deref()),
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

/// Comando de memória de primeira classe (E03-T07): falha fechada sem adaptador.
#[cfg(feature = "memory-in-process")]
fn memory_command() -> Report {
    use crate::memory::KnudgeMemory;
    use katu_core::memory::Memory;

    let root = std::env::current_dir().unwrap_or_default();
    match KnudgeMemory::open(&root) {
        Ok(memory) => match memory.status() {
            Ok(status) => Report::ok(
                Command::Memory.name(),
                Some(json!({
                    "backend": status.backend,
                    "health": format!("{:?}", status.health),
                    "warnings": status.warnings,
                    "knowledge_dir": memory.knowledge_dir().display().to_string(),
                })),
            ),
            Err(error) => runtime_failure(Command::Memory.name(), RuntimeError::Memory(error)),
        },
        Err(error) => runtime_failure(Command::Memory.name(), RuntimeError::Memory(error)),
    }
}

/// Comando `recall`: monta o runtime e consulta a memória pelo caminho §42.
#[cfg(feature = "memory-in-process")]
fn memory_recall(query: &str, limit: usize) -> Report {
    use crate::ports::{StdFs, SystemClock};

    let fs = StdFs;
    let clock = SystemClock;
    let start = std::env::current_dir().unwrap_or_default();
    let mut runtime = match Runtime::open(&fs, &clock, &start, "cli: recall") {
        Ok(runtime) => runtime,
        Err(error) => return runtime_failure("recall", error),
    };
    let project = runtime.root().display().to_string();
    match runtime.recall(query, limit) {
        Ok(dispatch) => Report::ok("recall", Some(dispatch_value(&project, &dispatch))),
        Err(error) => runtime_failure("recall", error),
    }
}

/// Comando `remember`: recall prévio + escrita pelo gate de E05.
#[cfg(feature = "memory-in-process")]
fn memory_remember(statement: &str, anchor: Option<&str>) -> Report {
    use crate::ports::{StdFs, SystemClock};
    use katu_core::memory::NoteType;

    let fs = StdFs;
    let clock = SystemClock;
    let start = std::env::current_dir().unwrap_or_default();
    let mut runtime = match Runtime::open(&fs, &clock, &start, "cli: remember") {
        Ok(runtime) => runtime,
        Err(error) => return runtime_failure("remember", error),
    };
    let project = runtime.root().display().to_string();
    let req = Runtime::note(statement, NoteType::Fact, anchor);
    match runtime.remember(&req) {
        Ok(dispatch) => Report::ok("remember", Some(dispatch_value(&project, &dispatch))),
        Err(error) => runtime_failure("remember", error),
    }
}

/// Converte a falha do runtime na taxonomia estável de erro do katu.
#[cfg(feature = "memory-in-process")]
fn runtime_failure(command: &'static str, error: RuntimeError) -> Report {
    let core: Error = error.into();
    Report::failed(command, &core)
}

/// Envelope de dados de um `Dispatch` (resultado + relatório TOON projetado para JSON).
#[cfg(feature = "memory-in-process")]
fn dispatch_value(project: &str, dispatch: &Dispatch) -> Value {
    let report = dispatch
        .report()
        .and_then(|report| report.to_json().ok())
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .unwrap_or(Value::Null);
    json!({
        "project": project,
        "ran": dispatch.ran(),
        "outcome": format!("{:?}", dispatch.outcome()),
        "report": report,
    })
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
