//! Comando `run` (E10/E12-T05): monta o runtime e o provider e executa um turno do agente.
//!
//! A borda CLI decide; aqui vive a composição (runtime + provider) e a conversão do resultado no
//! envelope de máquina. Sem o adaptador de memória, o runtime **recusa** (fail-closed, E03-T07).
//!
//! `KERNEL_SURFACE` F2: o CLI é um **cliente** do kernel. O `Kernel` é a thread que possui o
//! runtime/provider; o CLI envia `Submit` e bloqueia no evento terminal (é naturalmente sequencial).

use std::path::Path;
use std::time::Duration;

use katu_core::api::{Command, Event, KernelBus, KernelHandle, RecvError, TurnSummary, channel};
use katu_core::context::CompactionMode;
use katu_core::diag::{Level, events};
use katu_core::error::{Error, ErrorKind};
use katu_core::provider::{ModelSpec, Provider, Thinking};
use katu_providers::PriceTable;
use serde_json::{Value, json};

use crate::defaults;
use crate::kernel::Kernel;
use crate::login;
use crate::ports::{StdEnv, StdFs, StdProcess, SystemClock};
use crate::report::{Output, Report};
use crate::runtime::{Runtime, RuntimeError};
use crate::tier::TierPolicy;

mod progress;

/// Instrução de sistema (prime) enviada ao modelo no turno.
pub(crate) const SYSTEM: &str =
    "És o katu, um agente de código. Usa as tools quando precisares e responde de forma concisa.";

/// Período de espera por um evento do kernel (ms).
const EVENT_POLL_MS: u64 = 20;

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
    /// Formato de saída do comando (`LIVE_FLOW` LF5).
    pub output: Output,
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

/// Executa um turno do agente (pelo kernel) e devolve o relatório do comando.
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
    // K1: o kernel é a thread dona do runtime/provider; o CLI só envia comandos e consome eventos.
    let kernel = Kernel {
        runtime,
        provider: std::sync::Arc::from(provider),
        fs: &fs,
        process,
        env,
        model: ModelSpec::new(model.clone()),
        turn_model: Some(ModelSpec {
            model,
            thinking: args.thinking.unwrap_or_default(),
        }),
        tiers,
        // O envelope do CLI não mostra custo; a linha de uso da TUI é que o usa.
        prices: PriceTable::new(),
        max_tokens: args.max_tokens,
        max_steps: args.max_steps,
    };
    let result = run_scoped(
        |bus| kernel.run(bus),
        |handle| drive(handle, args.goal, args.output),
    );
    match result {
        Ok(summary) => Report::ok("run", Some(envelope(&summary))),
        Err((kind, message)) => Report::failed_parts("run", kind, &message),
    }
}

/// Corre o kernel na sua thread e o `drive` na thread da superfície, fechando o canal no fim.
///
/// O `handle` nasce **dentro** do escopo: quando o `drive` termina, o canal fecha e a thread do
/// kernel termina, pelo que o `join` implícito do escopo nunca fica pendurado. Criar o `handle`
/// **fora** do escopo faria o `join` esperar para sempre (o kernel bloqueado em `recv`) — era o
/// travamento do CLI no fim do turno.
fn run_scoped<K, F, T>(kernel: K, drive: F) -> T
where
    K: FnOnce(KernelBus) + Send,
    F: FnOnce(&KernelHandle) -> T,
{
    let _span = katu_core::trace_fn!("agent::command::run_scoped");

    std::thread::scope(|scope| {
        let (bus, handle) = channel();
        let _kernel = scope.spawn(move || kernel(bus));
        drive(&handle)
    })
}

/// Envia `Submit` e bloqueia no evento terminal (o kernel é a thread; o CLI é sequencial).
fn drive(
    handle: &KernelHandle,
    goal: &str,
    output: Output,
) -> Result<TurnSummary, (ErrorKind, String)> {
    let _span = katu_core::trace_fn!("agent::command::drive");

    if handle.send(Command::Submit(goal.to_string())).is_err() {
        return Err((
            ErrorKind::Internal,
            "kernel terminou antes do turno".to_string(),
        ));
    }
    let mut summary = None;
    let mut progress = progress::Progress::new(output);
    loop {
        match handle.recv(Duration::from_millis(EVENT_POLL_MS)) {
            Ok(Event::Turn(turn)) => summary = Some(*turn),
            Ok(Event::Failure { kind, message }) => return Err((kind, message)),
            // CLI não é interativo: uma escalação é recusada (fail-closed), como o `NoActivity`.
            Ok(Event::ApprovalRequest(_)) => {
                let _sent = handle.send(Command::Approval(None));
            }
            Ok(Event::Done) | Err(RecvError::Closed) => break,
            Ok(event) => progress.show(&event),
            Err(RecvError::Timeout) => {}
        }
    }
    summary.ok_or_else(|| {
        (
            ErrorKind::Internal,
            "o turno não devolveu resultado".to_string(),
        )
    })
}

/// Envelope de máquina de um turno concluído: o que o modelo viu (estado, política) e o que gastou.
fn envelope(turn: &TurnSummary) -> Value {
    let _span = katu_core::trace_fn!("agent::command::envelope");

    let usage = turn.usage.as_ref().map(|usage| {
        json!({
            "input": usage.input,
            "output": usage.output,
            "cached": usage.cached_input,
            "basis": usage.basis.as_str(),
        })
    });
    json!({
        "session": turn.session,
        "round_exit": turn.round_exit,
        "termination": turn.termination,
        "model": turn.model,
        "steps": turn.steps,
        "chars": turn.text.chars().count(),
        "calls": turn.calls,
        "cancelled": turn.cancelled,
        "stop": turn.stop,
        "usage": usage,
        "state": turn.state,
        "context_selection": turn.selection,
        "text": turn.text,
    })
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
    // B1/W8-1 (ADR 0025): **opt-in** — o `response_format` derivado das tools obriga o modelo a
    // emitir sempre uma tool call (o `oneOf` não tem variante de resposta final), pelo que ligá-lo
    // por omissão impede o turno de terminar em texto. A adoção por omissão exige A/B.
    let structured_output = defaults::current().structured_output.unwrap_or(false);
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
                structured_output,
            },
        ))),
        "opencode-go" | "opencode-zen" => {
            let key = env
                .var("KATU_OPENCODE_KEY")
                .or_else(|| env.var("OPENCODE_API_KEY"))
                .map(|key| key.trim().to_string())
                .filter(|key| !key.is_empty())
                .or_else(login::stored_key)
                .ok_or_else(|| {
                    "chave do opencode ausente: exporte KATU_OPENCODE_KEY ou faça `katu config login`"
                        .to_string()
                })?;
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

#[cfg(test)]
mod tests;
