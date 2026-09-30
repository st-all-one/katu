//! Comando `run` (E10/E12-T05): monta o runtime e o provider e executa um turno do agente.
//!
//! A borda CLI decide; aqui vive a composição (runtime + provider) e a conversão do resultado no
//! envelope de máquina. Sem o adaptador de memória, o runtime **recusa** (fail-closed, E03-T07).

use katu_core::error::Error;
use katu_core::provider::{ModelSpec, Provider};
use serde_json::{Value, json};

use super::{Ports, TurnOptions, run_turn};
use crate::ports::{StdEnv, StdFs, StdProcess, SystemClock};
use crate::report::Report;
use crate::runtime::{Runtime, RuntimeError};

/// Instrução de sistema (prime) enviada ao modelo no turno.
const SYSTEM: &str =
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
    /// Teto de tokens de saída.
    pub max_tokens: u32,
    /// Máximo de passos (tool calls) por turno.
    pub max_steps: u32,
}

/// Executa um turno do agente e devolve o relatório do comando.
pub(crate) fn run(args: &RunArgs<'_>) -> Report {
    let fs = StdFs;
    let clock = SystemClock;
    let env = StdEnv;
    let process = StdProcess;
    let start = std::env::current_dir().unwrap_or_default();
    let mut runtime = match Runtime::open(&fs, &clock, &start, "cli: run") {
        Ok(runtime) => runtime,
        Err(error) => return runtime_failure(error),
    };
    let model = args
        .model
        .map_or_else(|| default_model(args.provider).to_string(), str::to_string);
    let base = args
        .base
        .map_or_else(|| default_base(args.provider).to_string(), str::to_string);
    let provider = match build_provider(args.provider, &base, &env, runtime.session_id()) {
        Ok(provider) => provider,
        Err(message) => return Report::failed("run", &Error::invalid_input(message)),
    };
    let options = TurnOptions {
        model: ModelSpec::new(model.clone()),
        system: Some(SYSTEM.to_string()),
        max_tokens: args.max_tokens,
        temperature: 0.0,
        max_steps: args.max_steps,
    };
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };
    match run_turn(&mut runtime, provider.as_ref(), &ports, args.goal, &options) {
        Ok(turn) => Report::ok("run", Some(turn_value(&model, &turn))),
        Err(error) => Report::failed("run", &error.into()),
    }
}

/// Converte a falha do runtime na taxonomia estável de erro do katu.
fn runtime_failure(error: RuntimeError) -> Report {
    let core: Error = error.into();
    Report::failed("run", &core)
}

/// Constrói o provider a partir das flags da CLI.
fn build_provider(
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

/// Envelope do resultado de um turno.
fn turn_value(model: &str, turn: &super::TurnReport) -> Value {
    let usage = turn.usage.as_ref().map(|usage| {
        json!({
            "input": usage.input,
            "output": usage.output,
            "cached": usage.cached_input,
            "basis": usage.basis.as_str(),
        })
    });
    json!({
        "model": model,
        "steps": turn.steps,
        "chars": turn.text.chars().count(),
        "calls": turn.calls,
        "usage": usage,
    })
}

/// Base URL por omissão de cada provider.
fn default_base(provider: &str) -> &'static str {
    match provider {
        "opencode-go" => "https://opencode.ai/zen/go/v1",
        "opencode-zen" => "https://opencode.ai/zen/v1",
        _ => "http://127.0.0.1:8080/v1",
    }
}

/// Modelo por omissão de cada provider.
fn default_model(provider: &str) -> &'static str {
    match provider {
        "opencode-go" | "opencode-zen" => "longcat-2.5-preview-free",
        _ => "qwen",
    }
}
