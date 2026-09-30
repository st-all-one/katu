//! Comando `run` (E10/E12-T05): monta o runtime e o provider e executa um turno do agente.
//!
//! A borda CLI decide; aqui vive a composição (runtime + provider) e a conversão do resultado no
//! envelope de máquina. Sem o adaptador de memória, o runtime **recusa** (fail-closed, E03-T07).

use katu_core::context::CompactionMode;
use katu_core::diag::{Level, events};
use katu_core::error::Error;
use katu_core::provider::{ModelSpec, Provider, Thinking};
use serde_json::{Value, json};
use std::path::Path;

use super::{Ports, TurnOptions, TurnRequest, run_turn};
use crate::ports::{StdEnv, StdFs, StdProcess, SystemClock};
use crate::report::Report;
use crate::runtime::{Runtime, RuntimeError};
use crate::tier::TierPolicy;

/// Instrução de sistema (prime) enviada ao modelo no turno.
pub(crate) const SYSTEM: &str =
    "És o katu, um agente de código. Usa as tools quando precisares e responde de forma concisa.";

/// Argumentos do comando `run` (um por flag).
pub(crate) struct RunArgs<'a> {
    /// Objetivo/mensagem do utilizador.
    pub goal: &'a str,
    /// Provider (`llama`, `opencode-go`, `opencode-zen`).
    pub provider: &'a str,
    /// Modelo (por omissão depende do provider).
    pub model: Option<&'a str>,
    /// Base URL (por omissão depende do provider).
    pub base: Option<&'a str>,
    /// Grau de pensamento resolvido (config/flags).
    pub thinking: Option<Thinking>,
    /// Teto de tokens de saída.
    pub max_tokens: u32,
    /// Máximo de passos (tool calls) por turno.
    pub max_steps: u32,
    /// Liga a compactação do histórico no turno (E09-T07).
    pub compact: bool,
    /// Retoma a sessão: `None` cria nova; `Some("last")` retoma a mais recente; `Some(id)` a indicada.
    pub resume: Option<&'a str>,
}

/// Abre o runtime: sessão **nova** (`resume == None`) ou **retomada** (`last`/id).
///
/// # Errors
/// [`RuntimeError`] se a memória, o layout, as regras ou a retomada falharem.
pub(crate) fn open_runtime<'a>(
    fs: &'a StdFs,
    clock: &'a SystemClock,
    start: &Path,
    goal: &str,
    resume: Option<&str>,
) -> Result<Runtime<'a>, RuntimeError> {
    let _span = katu_core::trace_fn!("agent::command::open_runtime");

    match resume {
        Some("last") => Runtime::resume(fs, clock, start, goal, None),
        Some(id) => Runtime::resume(fs, clock, start, goal, Some(id)),
        None => Runtime::open(fs, clock, start, goal),
    }
}

/// Executa um turno do agente e devolve o relatório do comando.
pub(crate) fn run(args: &RunArgs<'_>) -> Report {
    let _span = katu_core::fn_span!(Level::Debug, events::CLI_RUN, "command::run");
    let fs = StdFs;
    let clock = SystemClock;
    let env = StdEnv;
    let process = StdProcess;
    let start = std::env::current_dir().unwrap_or_default();
    let mut runtime = match open_runtime(&fs, &clock, &start, "cli: run", args.resume) {
        Ok(runtime) => runtime,
        Err(error) => return runtime_failure(error),
    };
    if args.compact {
        runtime.set_compaction(CompactionMode::Enabled);
    }
    let base = args
        .base
        .map_or_else(|| default_base(args.provider).to_string(), str::to_string);
    let provider = match build_provider(args.provider, &base, &env, runtime.session_id()) {
        Ok(provider) => provider,
        Err(message) => return Report::failed("run", &Error::invalid_input(message)),
    };
    let tiers = match TierPolicy::load() {
        Ok(tiers) => tiers,
        Err(message) => return Report::failed("run", &Error::invalid_input(message)),
    };
    let model = args.model.map_or_else(
        || {
            tiers.model_for(
                provider.as_ref(),
                runtime.phase(),
                default_model(args.provider),
            )
        },
        str::to_string,
    );
    let options = turn_options(model.clone(), args);
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };
    match run_turn(
        &mut runtime,
        TurnRequest {
            provider: provider.as_ref(),
            ports,
            goal: args.goal,
            options: &options,
        },
    ) {
        Ok(turn) => {
            let value = turn_value(&model, &turn, runtime.session_id());
            Report::ok("run", Some(value))
        }
        Err(error) => Report::failed("run", &error.into()),
    }
}

/// Opções do turno (modelo + pensamento resolvidos, E20-T17).
fn turn_options(model: String, args: &RunArgs<'_>) -> TurnOptions {
    let _span = katu_core::trace_fn!("agent::command::turn_options");

    TurnOptions {
        model: ModelSpec {
            model,
            thinking: args.thinking.unwrap_or_default(),
        },
        system: Some(SYSTEM.to_string()),
        max_tokens: args.max_tokens,
        temperature: 0.0,
        max_steps: args.max_steps,
    }
}

/// Converte a falha do runtime na taxonomia estável de erro do katu.
fn runtime_failure(error: RuntimeError) -> Report {
    let _span = katu_core::trace_fn!("agent::command::runtime_failure");

    let core: Error = error.into();
    Report::failed("run", &core)
}

/// Constrói o provider a partir das flags da CLI.
pub(crate) fn build_provider(
    name: &str,
    base: &str,
    env: &StdEnv,
    session: Option<&str>,
) -> Result<Box<dyn Provider>, String> {
    use katu_core::ports::Env;
    use katu_providers::{Llama, LlamaConfig, OpenCode, OpenCodeConfig, UreqTransport};
    use std::time::Duration;

    let transport = UreqTransport::new(Duration::from_secs(5), Duration::from_secs(120));
    match name {
        "llama" => Ok(Box::new(Llama::new(
            transport,
            LlamaConfig {
                base_url: base.to_string(),
                health_url: format!(
                    "{}/health",
                    base.trim_end_matches("/v1").trim_end_matches('/')
                ),
                max_tokens: None,
                temperature: None,
                reasoning_format: None,
            },
        ))),
        "opencode-go" | "opencode-zen" => {
            let key = env
                .var("KATU_OPENCODE_KEY")
                .ok_or_else(|| "KATU_OPENCODE_KEY ausente (exporte a chave)".to_string())?;
            let config = if name == "opencode-go" {
                OpenCodeConfig::go(key)
            } else {
                OpenCodeConfig::zen(key)
            };
            let config = match session {
                Some(session) => config.with_session(session),
                None => config,
            };
            Ok(Box::new(OpenCode::new(
                transport,
                config.with_base_url(base),
            )))
        }
        other => Err(format!("provider desconhecido: {other}")),
    }
}

/// Envelope do resultado de um turno (id da sessão + exit da rodada).
fn turn_value(model: &str, turn: &super::TurnReport, session: Option<&str>) -> Value {
    let _span = katu_core::trace_fn!("agent::command::turn_value");

    let usage = turn.usage.as_ref().map(|usage| {
        json!({
            "input": usage.input,
            "output": usage.output,
            "cached": usage.cached_input,
            "basis": usage.basis.as_str(),
        })
    });
    json!({
        "session": session,
        "round_exit": 0,
        "model": model,
        "steps": turn.steps,
        "chars": turn.text.chars().count(),
        "calls": turn.calls,
        "cancelled": turn.cancelled,
        "usage": usage,
    })
}

/// Base URL por omissão de cada provider.
pub(crate) fn default_base(provider: &str) -> &'static str {
    let _span = katu_core::trace_fn!("agent::command::default_base");

    match provider {
        "opencode-go" => "https://opencode.ai/zen/go/v1",
        "opencode-zen" => "https://opencode.ai/zen/v1",
        _ => "http://127.0.0.1:8080/v1",
    }
}

/// Modelo por omissão de cada provider.
pub(crate) fn default_model(provider: &str) -> &'static str {
    let _span = katu_core::trace_fn!("agent::command::default_model");

    match provider {
        "opencode-go" | "opencode-zen" => "longcat-2.5-preview-free",
        _ => "qwen",
    }
}
