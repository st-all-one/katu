//! `run` e `tui` (E20-T04/T03): a borda que monta o runtime e o provider.
//!
//! Vive num módulo filho (e só com `memory-in-process`) para manter `cli.rs` sob o teto e o
//! firewall: `agent`/`tui` não são puxados para a borda sem o adaptador. O `run` aceita
//! `--params`/`--batch` (E20-T08); a resolução vive em [`super::run_params`].

use clap::Args;
use katu_core::diag::{Level, events};

use crate::agent::{RunArgs, run};
use crate::report::{Output, Report};
use crate::tui::run_tui;

use super::run_params;

/// Argumentos de `katu run`.
#[derive(Debug, Clone, Args)]
pub(crate) struct RunCli {
    /// Objetivo (posicional = body; `-`/ausente lê `stdin`).
    pub(crate) body: Option<String>,
    /// Provider (`llama`, `opencode-go`, `opencode-zen`).
    #[arg(long)]
    pub(crate) provider: Option<String>,
    /// Modelo (por omissão depende do provider).
    #[arg(long)]
    pub(crate) model: Option<String>,
    /// Base URL do endpoint (por omissão depende do provider).
    #[arg(long)]
    pub(crate) base: Option<String>,
    /// Grau de pensamento (`off`/`low`/`medium`/`high`).
    #[arg(long)]
    pub(crate) thinking: Option<String>,
    /// Teto de tokens de saída.
    #[arg(long)]
    pub(crate) max_tokens: Option<u32>,
    /// Máximo de passos (tool calls) por turno.
    #[arg(long)]
    pub(crate) max_steps: Option<u32>,
    /// Liga a compactação do histórico no turno (E09-T07).
    #[arg(long, num_args = 0..=1, default_missing_value = "true")]
    pub(crate) compact: Option<bool>,
    /// Retoma a sessão: sem valor usa a mais recente; com valor, o id indicado.
    #[arg(long, num_args = 0..=1, default_missing_value = "last")]
    pub(crate) resume: Option<String>,
    /// Config universal do comando (JSON; XOR com as flags explícitas).
    #[arg(long)]
    pub(crate) params: Option<String>,
    /// Lote JSONL (uma linha = um item; XOR com `--params` e flags).
    #[arg(long)]
    pub(crate) batch: Option<String>,
    /// Emite envelope JSON em `stdout`.
    #[arg(long)]
    pub(crate) json: bool,
    /// Formato de saída (`text`/`json`/`stream-json`); `stream-json` emite um evento por linha.
    #[arg(long, value_name = "FORMATO")]
    pub(crate) output: Option<String>,
}

impl RunCli {
    /// `true` se o envelope final é JSON (`--json` ou `--output json|stream-json`).
    pub(crate) fn wants_json(&self) -> bool {
        let _span = katu_core::trace_fn!("cli::run_cmd::wants_json");

        self.json || matches!(self.output.as_deref(), Some("json" | "stream-json"))
    }
}

/// Argumentos de `katu tui`.
#[derive(Debug, Clone, Default, Args)]
pub(crate) struct TuiCli {
    /// Provider (`llama`, `opencode-go`, `opencode-zen`).
    #[arg(long)]
    pub(crate) provider: Option<String>,
    /// Modelo (por omissão depende do provider).
    #[arg(long)]
    pub(crate) model: Option<String>,
    /// Base URL do endpoint (por omissão depende do provider).
    #[arg(long)]
    pub(crate) base: Option<String>,
    /// Grau de pensamento (`off`/`low`/`medium`/`high`).
    #[arg(long)]
    pub(crate) thinking: Option<String>,
    /// Teto de tokens de saída.
    #[arg(long)]
    pub(crate) max_tokens: Option<u32>,
    /// Máximo de passos (tool calls) por turno.
    #[arg(long)]
    pub(crate) max_steps: Option<u32>,
    /// Liga a compactação do histórico no turno (E09-T07).
    #[arg(long, num_args = 0..=1, default_missing_value = "true")]
    pub(crate) compact: Option<bool>,
    /// Retoma a sessão: sem valor usa a mais recente; com valor, o id indicado.
    #[arg(long, num_args = 0..=1, default_missing_value = "last")]
    pub(crate) resume: Option<String>,
    /// Config universal do comando (JSON; XOR com as flags explícitas).
    #[arg(long)]
    pub(crate) params: Option<String>,
}

/// Executa `katu run` (uma rodada ou um lote).
pub(crate) fn execute_run(args: &RunCli) -> Report {
    let _span = katu_core::trace_fn!("cli::run_cmd::execute_run");

    run_params::execute(args)
}

/// Executa `katu tui`: abre a UI sobre o loop de turnos.
pub(crate) fn execute_tui(args: &TuiCli) -> Report {
    let _span = katu_core::fn_span!(Level::Debug, events::CLI_TUI, "run_cmd::execute_tui");
    match run_params::resolve_tui(args) {
        Ok(config) => run_tui(&RunArgs {
            goal: &config.goal,
            provider: &config.provider,
            model: config.model.as_deref(),
            base: config.base.as_deref(),
            thinking: config.thinking,
            max_tokens: config.max_tokens,
            max_steps: config.max_steps,
            compact: config.compact,
            resume: config.resume.as_deref(),
            output: Output::Text,
        }),
        Err(error) => Report::failed("tui", &error),
    }
}

/// Corre **uma** rodada a partir de uma config resolvida.
pub(crate) fn run_once(config: &run_params::RunConfig) -> Report {
    let _span = katu_core::fn_span!(Level::Debug, events::CLI_RUN, "run_cmd::run_once");
    run(&RunArgs {
        goal: &config.goal,
        provider: &config.provider,
        model: config.model.as_deref(),
        base: config.base.as_deref(),
        thinking: config.thinking,
        max_tokens: config.max_tokens,
        max_steps: config.max_steps,
        compact: config.compact,
        resume: config.resume.as_deref(),
        output: config.output,
    })
}
