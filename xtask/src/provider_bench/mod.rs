#![allow(
    clippy::print_stdout,
    reason = "xtask dev-only: relatório de medição de latência do provider"
)]
//! `bench-provider` / `gate:provider` (E12-T07, F4/E18-T04): mede a latência de comunicação do
//! provider e publica o número com base tipada e artefacto cru (DF5).
//!
//! - **Offline (determinístico):** dirige o built-in sobre um [`MockTransport`] com um SSE
//!   canónico grande e mede o **overhead de cliente por turno** (encode + decode + eventos). Sem
//!   rede, é o número que o gate A/B trava (E18 §0.3).
//! - **Live (dev-only):** mede TTFT/total/usage contra o `llama-server` local ou o built-in
//!   `opencode`; grava o artefacto com a máquina (`os`/`arch`). Percentis variam por máquina.
//!
//! O gate lê o **orçamento** de `bench/providers/budget.toml` (dado versionado, não constante de
//! código): regressão acima do orçamento **falha**. Nada aqui toca o caminho de produção.

mod report;

use std::time::{Duration, Instant};

use katu_core::provider::{
    CollectSink, Flow, Provider, ProviderEvent, ProviderRequest, ProviderSink, TokenUsage,
};
use katu_providers::{Llama, LlamaConfig, MockTransport, OpenCode, OpenCodeConfig, UreqTransport};
use serde::Deserialize;
use serde_json::{Value, json};

use report::{DELTAS, corpus, nanos, percentiles, request, usage_value};

/// Repetições do gate (rápido e estável).
const GATE_REPS: u32 = 200;
/// Artefacto cru commitado.
const ARTIFACT: &str = "bench/providers/latency.json";
/// Orçamento de latência (dado).
const BUDGET: &str = "bench/providers/budget.toml";
const TIMEOUT_CONNECT: Duration = Duration::from_secs(5);
const TIMEOUT_READ: Duration = Duration::from_secs(120);

/// Orçamento versionado (E12-T07).
#[derive(Debug, Deserialize)]
struct BudgetFile {
    provider: Budget,
}

/// Limites de latência, em `nanos`.
#[derive(Debug, Deserialize)]
struct Budget {
    client_overhead_p95_nanos: u64,
}

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

/// Amostra offline (client-side).
struct Offline {
    samples: Vec<u64>,
    usage: Option<TokenUsage>,
    corpus_bytes: usize,
}

/// Mede `reps` turnos offline pelo [`MockTransport`].
#[allow(
    clippy::disallowed_methods,
    reason = "instrumento dev-only: o relógio monotónico é o objeto da medição"
)]
fn offline(reps: u32) -> Result<Offline, String> {
    let body = corpus();
    let corpus_bytes = body.len();
    let provider = OpenCode::new(
        MockTransport::ok(body, 8192),
        OpenCodeConfig::at("https://offline.invalid/v1", "k"),
    );
    let request = request("offline");
    let mut samples = Vec::with_capacity(usize::try_from(reps).unwrap_or(0));
    let mut usage = None;
    for _ in 0..reps {
        let start = Instant::now();
        let mut sink = CollectSink::default();
        let outcome = provider
            .stream(&request, &mut sink)
            .map_err(|error| format!("offline: {error}"))?;
        samples.push(nanos(start.elapsed()));
        usage = outcome.usage;
    }
    Ok(Offline {
        samples,
        usage,
        corpus_bytes,
    })
}

/// Mede `reps` turnos live (TTFT/total/usage).
#[allow(
    clippy::disallowed_methods,
    reason = "instrumento dev-only: o relógio monotónico é o objeto da medição"
)]
fn live(provider_name: &str, args: &[String], reps: u32, model: &str) -> Result<Value, String> {
    let transport = UreqTransport::new(TIMEOUT_CONNECT, TIMEOUT_READ);
    let request = request(model);
    let mut ttft = Vec::with_capacity(usize::try_from(reps).unwrap_or(0));
    let mut total = Vec::with_capacity(usize::try_from(reps).unwrap_or(0));
    let mut usage = None;
    match provider_name {
        "llama" => {
            let base = value(args, "--base").unwrap_or("http://127.0.0.1:8080/v1");
            let health_url = format!(
                "{}/health",
                base.trim_end_matches("/v1").trim_end_matches('/')
            );
            let config = LlamaConfig {
                base_url: base.to_string(),
                health_url,
                max_tokens: Some(64),
                temperature: Some(0.0),
                reasoning_format: None,
            };
            let provider = Llama::new(transport, config);
            for _ in 0..reps {
                measure(&provider, &request, &mut ttft, &mut total, &mut usage)?;
            }
        }
        "opencode-go" | "opencode-zen" => {
            let Some(key) = std::env::var_os("KATU_OPENCODE_KEY") else {
                return Err("KATU_OPENCODE_KEY ausente".to_string());
            };
            let config = if provider_name == "opencode-go" {
                OpenCodeConfig::go(key.to_string_lossy().into_owned())
            } else {
                OpenCodeConfig::zen(key.to_string_lossy().into_owned())
            };
            let provider = OpenCode::new(transport, config);
            for _ in 0..reps {
                measure(&provider, &request, &mut ttft, &mut total, &mut usage)?;
            }
        }
        other => return Err(format!("provider desconhecido: {other}")),
    }
    let (t50, t95, _, tmax) = percentiles(&ttft);
    let (w50, w95, _, wmax) = percentiles(&total);
    Ok(json!({
        "provider": provider_name,
        "model": model,
        "reps": reps,
        "ttft_ms": { "p50": t50 / 1_000_000, "p95": t95 / 1_000_000, "max": tmax / 1_000_000 },
        "total_ms": { "p50": w50 / 1_000_000, "p95": w95 / 1_000_000, "max": wmax / 1_000_000 },
        "usage": usage_value(usage),
    }))
}

/// Executa uma chamada e acumula TTFT/total.
#[allow(
    clippy::disallowed_methods,
    reason = "instrumento dev-only: o relógio monotónico é o objeto da medição"
)]
fn measure<P: Provider>(
    provider: &P,
    request: &ProviderRequest,
    ttft: &mut Vec<u64>,
    total: &mut Vec<u64>,
    usage: &mut Option<TokenUsage>,
) -> Result<(), String> {
    let start = Instant::now();
    let mut sink = Timed::new(start);
    let outcome = provider
        .stream(request, &mut sink)
        .map_err(|error| format!("live: {error}"))?;
    ttft.push(nanos(sink.ttft.unwrap_or_else(|| start.elapsed())));
    total.push(nanos(start.elapsed()));
    *usage = outcome.usage;
    Ok(())
}

/// Ponto de entrada de `bench-provider`: mede, grava o artefacto e imprime o resumo.
pub(crate) fn run(args: &[String]) {
    let reps = value(args, "--reps")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(GATE_REPS);
    let out = value(args, "--out").unwrap_or(ARTIFACT);
    let offline = match offline(reps) {
        Ok(offline) => offline,
        Err(error) => {
            println!("{{\"error\":\"{error}\"}}");
            return;
        }
    };
    let (p50, p95, p99, max) = percentiles(&offline.samples);
    let live = args.iter().any(|arg| arg == "--live").then(|| {
        let model = value(args, "--model").unwrap_or("qwen");
        live(
            value(args, "--provider").unwrap_or("llama"),
            args,
            reps,
            model,
        )
    });
    let live = match live {
        Some(Ok(value)) => Some(value),
        Some(Err(error)) => {
            println!("{{\"error\":\"{error}\"}}");
            None
        }
        None => None,
    };
    let artifact = json!({
        "machine": { "os": std::env::consts::OS, "arch": std::env::consts::ARCH },
        "offline": {
            "reps": reps,
            "corpus_bytes": offline.corpus_bytes,
            "deltas": DELTAS,
            "overhead_nanos": { "p50": p50, "p95": p95, "p99": p99, "max": max },
            "raw_nanos": offline.samples,
            "usage": usage_value(offline.usage),
        },
        "live": live,
    });
    match serde_json::to_string_pretty(&artifact) {
        Ok(pretty) => {
            if let Err(error) = std::fs::write(out, format!("{pretty}\n")) {
                println!("{{\"error\":\"gravando {out}: {error}\"}}");
                return;
            }
        }
        Err(error) => {
            println!("{{\"error\":\"serializando artefacto: {error}\"}}");
            return;
        }
    }
    println!("{{\"artifact\":\"{out}\",\"reps\":{reps},\"offline_p95_nanos\":{p95}}}");
}

/// Ponto de entrada de `gate:provider`: trava a regressão contra o orçamento versionado.
pub(crate) fn gate(args: &[String]) -> Result<(), String> {
    let path = args.first().map_or(BUDGET, String::as_str);
    let text = std::fs::read_to_string(path).map_err(|err| format!("lendo {path}: {err}"))?;
    let file: BudgetFile =
        toml::from_str(&text).map_err(|err| format!("{path} inválido: {err}"))?;
    let offline = offline(GATE_REPS)?;
    let (_, p95, _, _) = percentiles(&offline.samples);
    let budget = file.provider.client_overhead_p95_nanos;
    let artifact =
        std::fs::read_to_string(ARTIFACT).map_err(|err| format!("lendo {ARTIFACT}: {err}"))?;
    serde_json::from_str::<Value>(&artifact)
        .map_err(|err| format!("{ARTIFACT} inválido: {err}"))?;
    if p95 > budget {
        return Err(format!(
            "gate:provider falhou: overhead p95={p95} ns > orçamento {budget} ns"
        ));
    }
    println!("gate:provider ok: overhead p95={p95} ns (orçamento {budget} ns)");
    Ok(())
}

/// Lê o valor de `--chave`.
fn value<'a>(args: &'a [String], key: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == key)?;
    args.get(index.saturating_add(1)).map(String::as_str)
}
