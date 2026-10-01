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
mod sse;

use std::time::{Duration, Instant};

use katu_core::provider::{
    CollectSink, Flow, Provider, ProviderEvent, ProviderRequest, ProviderSink, TokenUsage,
};
use katu_providers::{Llama, LlamaConfig, MockTransport, OpenCode, OpenCodeConfig, UreqTransport};
use serde::Deserialize;
use serde_json::{Value, json};

use report::{DELTAS, corpus, nanos, request, usage_value};
use sse::ab;

use crate::stats::{aggregate, ci95_json};
use katu_core::stats::Summary;

/// Repetições do gate (rápido e estável).
const GATE_REPS: u32 = 200;
/// Repetições do A/B do parser SSE (P-04): mede-se o mínimo, que precisa de mais amostras.
const SSE_REPS: u32 = 1_000;
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

/// Amostras de uma medição live (TTFT/total/usage).
struct LiveSamples {
    ttft: Vec<u64>,
    total: Vec<u64>,
    usage: Option<TokenUsage>,
}

/// Dados do artefacto offline.
struct OfflineReport<'a> {
    reps: u32,
    offline: &'a Offline,
    summary: Summary,
    p99: u64,
    max: u64,
    sse: &'a Value,
    live: Option<&'a Value>,
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

/// Mede `reps` turnos live (TTFT/total/usage) e devolve as amostras cruas.
#[allow(
    clippy::disallowed_methods,
    reason = "instrumento dev-only: o relógio monotónico é o objeto da medição"
)]
fn collect_live(
    provider_name: &str,
    args: &[String],
    reps: u32,
    request: &ProviderRequest,
) -> Result<LiveSamples, String> {
    let transport = UreqTransport::new(TIMEOUT_CONNECT, TIMEOUT_READ);
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
                structured_output: false,
            };
            let provider = Llama::new(transport, config);
            for _ in 0..reps {
                measure(&provider, request, &mut ttft, &mut total, &mut usage)?;
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
                measure(&provider, request, &mut ttft, &mut total, &mut usage)?;
            }
        }
        other => return Err(format!("provider desconhecido: {other}")),
    }
    Ok(LiveSamples { ttft, total, usage })
}

/// JSON do live (TTFT/total com IC 95 %), ou `None` sem `--live`.
fn live_value(args: &[String], reps: u32) -> Option<Value> {
    if !args.iter().any(|arg| arg == "--live") {
        return None;
    }
    let model = value(args, "--model").unwrap_or("qwen");
    let provider = value(args, "--provider").unwrap_or("llama");
    match collect_live(provider, args, reps, &request(model)) {
        Ok(samples) => Some(live_json(provider, model, reps, &samples)),
        Err(error) => {
            println!("{{\"error\":\"{error}\"}}");
            None
        }
    }
}

/// JSON de uma medição live.
fn live_json(provider: &str, model: &str, reps: u32, samples: &LiveSamples) -> Value {
    let ttft_summary = Summary::from_samples(&samples.ttft);
    let total_summary = Summary::from_samples(&samples.total);
    let tmax = samples.ttft.iter().copied().max().unwrap_or(0);
    let wmax = samples.total.iter().copied().max().unwrap_or(0);
    json!({
        "provider": provider,
        "model": model,
        "reps": reps,
        "ttft_ms": {
            "p50": ttft_summary.p50 / 1_000_000,
            "p95": ttft_summary.p95 / 1_000_000,
            "max": tmax / 1_000_000,
            "mean": ttft_summary.mean / 1_000_000,
            "ci95": { "low": ttft_summary.ci95_low / 1_000_000, "high": ttft_summary.ci95_high / 1_000_000 },
        },
        "total_ms": {
            "p50": total_summary.p50 / 1_000_000,
            "p95": total_summary.p95 / 1_000_000,
            "max": wmax / 1_000_000,
            "mean": total_summary.mean / 1_000_000,
            "ci95": { "low": total_summary.ci95_low / 1_000_000, "high": total_summary.ci95_high / 1_000_000 },
        },
        "usage": usage_value(samples.usage),
    })
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
    let (summary, p99, max) = match aggregate(&offline.samples) {
        Ok(aggregate) => aggregate,
        Err(error) => {
            println!("{{\"error\":\"{error}\"}}");
            return;
        }
    };
    let sse = sse_value();
    let live = live_value(args, reps);
    let report = OfflineReport {
        reps,
        offline: &offline,
        summary,
        p99,
        max,
        sse: &sse,
        live: live.as_ref(),
    };
    let artifact = offline_value(&report);
    if let Err(error) = write_artifact(out, &artifact) {
        println!("{{\"error\":\"{error}\"}}");
        return;
    }
    println!(
        "{{\"artifact\":\"{out}\",\"reps\":{reps},\"offline_p95_nanos\":{}}}",
        summary.p95
    );
}

/// JSON do artefacto offline (com IC 95 %).
fn offline_value(report: &OfflineReport<'_>) -> Value {
    let summary = report.summary;
    json!({
        "machine": { "os": std::env::consts::OS, "arch": std::env::consts::ARCH },
        "offline": {
            "reps": report.reps,
            "corpus_bytes": report.offline.corpus_bytes,
            "deltas": DELTAS,
            "overhead_nanos": {
                "p50": summary.p50,
                "p95": summary.p95,
                "p99": report.p99,
                "max": report.max,
                "mean": summary.mean,
                "ci95": ci95_json(&summary),
            },
            "raw_nanos": report.offline.samples,
            "usage": usage_value(report.offline.usage),
        },
        "sse": report.sse,
        "live": report.live,
    })
}

/// Grava o artefacto pretty (falha legível).
fn write_artifact(path: &str, artifact: &Value) -> Result<(), String> {
    let pretty = serde_json::to_string_pretty(artifact)
        .map_err(|error| format!("serializando artefacto: {error}"))?;
    std::fs::write(path, format!("{pretty}\n")).map_err(|error| format!("gravando {path}: {error}"))
}

/// Ponto de entrada de `gate:provider`: trava a regressão contra o orçamento versionado.
pub(crate) fn gate(args: &[String]) -> Result<(), String> {
    let path = args.first().map_or(BUDGET, String::as_str);
    let text = std::fs::read_to_string(path).map_err(|err| format!("lendo {path}: {err}"))?;
    let file: BudgetFile =
        toml::from_str(&text).map_err(|err| format!("{path} inválido: {err}"))?;
    let offline = offline(GATE_REPS)?;
    let (summary, _, _) = aggregate(&offline.samples)?;
    let budget = file.provider.client_overhead_p95_nanos;
    let artifact =
        std::fs::read_to_string(ARTIFACT).map_err(|err| format!("lendo {ARTIFACT}: {err}"))?;
    serde_json::from_str::<Value>(&artifact)
        .map_err(|err| format!("{ARTIFACT} inválido: {err}"))?;
    if !summary.within_budget(budget) {
        return Err(format!(
            "gate:provider falhou: overhead IC95 superior={} ns > orçamento {budget} ns (p95={})",
            summary.ci95_high, summary.p95
        ));
    }
    println!(
        "gate:provider ok: overhead p95={} ns CI95=[{}, {}] (orçamento {budget} ns)",
        summary.p95, summary.ci95_low, summary.ci95_high
    );
    Ok(())
}

/// A/B do parser SSE (P-04) com a réplica congelada, como JSON.
fn sse_value() -> Value {
    let outcome = ab(&corpus(), 4096, SSE_REPS);
    // O ganho publica-se sobre o **mínimo** (o sinal com menos ruído de alocador/agendamento); as
    // medianas ficam no artefacto para quem quiser ver a dispersão.
    let saved = outcome
        .legacy_min_nanos
        .saturating_sub(outcome.production_min_nanos);
    json!({
        "events": outcome.events,
        "chunks": outcome.chunks,
        "production_nanos": outcome.production_nanos,
        "legacy_nanos": outcome.legacy_nanos,
        "production_min_nanos": outcome.production_min_nanos,
        "legacy_min_nanos": outcome.legacy_min_nanos,
        "gain_ratio": ratio(saved, outcome.legacy_min_nanos),
    })
}

/// Fração poupada (4 casas).
fn ratio(saved: u64, total: u64) -> f64 {
    if total == 0 {
        return 0.0;
    }
    let value = f64::from(u32::try_from(saved).unwrap_or(u32::MAX))
        / f64::from(u32::try_from(total).unwrap_or(u32::MAX));
    (value * 10_000.0).round() / 10_000.0
}

/// Lê o valor de `--chave`.
fn value<'a>(args: &'a [String], key: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == key)?;
    args.get(index.saturating_add(1)).map(String::as_str)
}
