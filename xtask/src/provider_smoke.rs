#![allow(
    clippy::print_stdout,
    reason = "xtask dev-only: relatório de medição de latência do provider"
)]
//! `provider-smoke` (E12-T06/T07, dev-only): mede **TTFT** e total de uma chamada real.
//!
//! Não é produto: é o instrumento de monitorização enquanto a camada de provider amadurece.
//! Fala com o `llama-server` local (`--provider llama --base http://127.0.0.1:8080/v1`) ou com o
//! built-in `opencode go/zen` (`KATU_OPENCODE_KEY`). Sem artefacto commitado: a medição entra em
//! `bench/` só quando a série for estável (E12-T07).

use std::time::{Duration, Instant};

use katu_core::kernel::Message;
use katu_core::provider::{
    CollectSink, Flow, ModelSpec, Provider, ProviderError, ProviderEvent, ProviderRequest,
    ProviderSink,
};
use katu_providers::{Llama, LlamaConfig, OpenCode, OpenCodeConfig, UreqTransport};

/// Sink que mede o tempo até ao primeiro evento (TTFT).
struct Timed {
    inner: CollectSink,
    start: Instant,
    ttft: Option<Duration>,
}

impl Timed {
    fn new(start: Instant) -> Self {
        Self {
            inner: CollectSink::default(),
            start,
            ttft: None,
        }
    }
}

impl ProviderSink for Timed {
    fn on_event(&mut self, event: ProviderEvent) -> Flow {
        if self.ttft.is_none() {
            self.ttft = Some(self.start.elapsed());
        }
        self.inner.on_event(event)
    }
}

/// Lê o valor de `--chave`.
fn value<'a>(args: &'a [String], key: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == key)?;
    args.get(index.saturating_add(1)).map(String::as_str)
}

/// Ponto de entrada do `xtask provider-smoke`.
/// Contexto de uma medição (agrupa os argumentos para não exceder o limite de parâmetros).
struct Ctx<'a> {
    args: &'a [String],
    prompt: &'a str,
    max_tokens: u32,
    turns: u32,
    warm: bool,
}

#[allow(
    clippy::disallowed_methods,
    reason = "instrumento dev-only: o relógio monotónico é o objeto da medição"
)]
pub(crate) fn run(args: &[String]) {
    let provider = value(args, "--provider").unwrap_or("llama");
    let ctx = Ctx {
        args,
        prompt: value(args, "--prompt").unwrap_or("Diga apenas: ola"),
        max_tokens: value(args, "--max-tokens")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(64),
        turns: value(args, "--turns")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(1),
        warm: args.iter().any(|arg| arg == "--warm"),
    };
    let compress = args.iter().any(|arg| arg == "--compress");
    let transport = UreqTransport::new(Duration::from_secs(5), Duration::from_secs(120));
    let transport = if compress {
        transport.with_request_compression(4096)
    } else {
        transport
    };
    dispatch(provider, transport, &ctx);
}

/// Escolhe e executa o provider pedido.
fn dispatch(provider: &str, transport: UreqTransport, ctx: &Ctx<'_>) {
    match provider {
        "llama" => {
            let base = value(ctx.args, "--base").unwrap_or("http://127.0.0.1:8080/v1");
            let model = value(ctx.args, "--model").unwrap_or("qwen");
            let config = LlamaConfig {
                base_url: base.to_string(),
                health_url: health_of(base),
                max_tokens: Some(ctx.max_tokens),
                temperature: Some(0.0),
                reasoning_format: None,
            };
            let llama = Llama::new(transport, config);
            if ctx.warm {
                llama.warm();
            }
            report(&llama, model, ctx.prompt, ctx.max_tokens, ctx.turns);
        }
        "opencode-go" | "opencode-zen" => {
            let Some(key) = std::env::var_os("KATU_OPENCODE_KEY") else {
                println!("{{\"skipped\":\"KATU_OPENCODE_KEY ausente\"}}");
                return;
            };
            let key = key.to_string_lossy().to_string();
            let default_base = if provider == "opencode-go" {
                "https://opencode.ai/zen/go/v1"
            } else {
                "https://opencode.ai/zen/v1"
            };
            let base = value(ctx.args, "--base").unwrap_or(default_base);
            let model = value(ctx.args, "--model").unwrap_or("longcat-2.5-preview-free");
            let session = value(ctx.args, "--session").unwrap_or("katu-provider-smoke");
            let config = if provider == "opencode-go" {
                OpenCodeConfig::go(key)
            } else {
                OpenCodeConfig::zen(key)
            }
            .with_base_url(base)
            .with_session(session);
            let opencode = OpenCode::new(transport, config);
            if ctx.warm {
                opencode.warm();
            }
            report(&opencode, model, ctx.prompt, ctx.max_tokens, ctx.turns);
        }
        other => println!("{{\"error\":\"provider desconhecido: {other}\"}}"),
    }
}

/// Executa `turns` chamadas sobre o mesmo provider (prova *keep-alive*) e imprime cada medição.
fn report<P: Provider>(provider: &P, model: &str, prompt: &str, max_tokens: u32, turns: u32) {
    for _turn in 0..turns {
        match measure(provider, model, prompt, max_tokens) {
            Ok(line) => println!("{line}"),
            Err(error) => println!("{{\"error\":\"{error}\"}}"),
        }
    }
}

/// Executa uma chamada e devolve a linha JSON de medição.
#[allow(
    clippy::disallowed_methods,
    reason = "instrumento dev-only: o relógio monotónico é o objeto da medição"
)]
fn measure<P: Provider>(
    provider: &P,
    model: &str,
    prompt: &str,
    max_tokens: u32,
) -> Result<String, ProviderError> {
    let request = ProviderRequest {
        model: ModelSpec::new(model),
        system: Some("Responde em uma frase curta.".to_string()),
        messages: vec![Message::User {
            text: prompt.to_string(),
        }],
        tools: Vec::new(),
        max_tokens: Some(max_tokens),
        temperature: Some(0.0),
    };
    let start = Instant::now();
    let mut sink = Timed::new(start);
    let outcome = provider.stream(&request, &mut sink)?;
    let total = start.elapsed();
    let ttft_ms = sink.ttft.map_or(0, |ttft| {
        u64::try_from(ttft.as_millis()).unwrap_or(u64::MAX)
    });
    let total_ms = u64::try_from(total.as_millis()).unwrap_or(u64::MAX);
    let usage = outcome.usage.map_or_else(
        || "null".to_string(),
        |usage| {
            format!(
                "{{\"input\":{},\"output\":{},\"cached\":{},\"reasoning\":{}}}",
                opt(usage.input),
                opt(usage.output),
                opt(usage.cached_input),
                opt(usage.reasoning),
            )
        },
    );
    Ok(format!(
        "{{\"provider\":\"{id}\",\"model\":\"{model}\",\"ttft_ms\":{ttft_ms},\"total_ms\":{total_ms},\"chars\":{chars},\"calls\":{calls},\"usage\":{usage},\"stop\":\"{stop:?}\"}}",
        id = provider.id(),
        chars = sink.inner.text.chars().count(),
        calls = sink.inner.calls.len(),
        stop = outcome.stop,
    ))
}

/// Formata um `Option<u64>` como número ou `null`.
fn opt(value: Option<u64>) -> String {
    value.map_or_else(|| "null".to_string(), |value| value.to_string())
}

/// Deriva `/health` a partir da base `/v1`.
fn health_of(base: &str) -> String {
    format!(
        "{}/health",
        base.trim_end_matches("/v1").trim_end_matches('/')
    )
}
