#![allow(
    clippy::print_stdout,
    reason = "xtask dev-only: relatório de medição do render da TUI"
)]
//! `bench-render` / `gate:render` (E15-T01, E10-T03): mede o custo de render por quadro e trava a
//! regressão contra o orçamento versionado (DF5).
//!
//! Constrói um [`App`] sintético determinístico (conversa cheia + painel de atividade) e desenha
//! `reps` quadros num [`TestBackend`] (sem terminal real). O artefacto cru é `bench/render/frame.json`
//! e o orçamento versionado é `bench/render/budget.toml` — a regressão acima do orçamento **falha**.
//! Nada aqui toca o caminho de produção.

use std::time::{Duration, Instant};

use katu_tui::{App, Live, Update, render};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::stats::{aggregate, ci95_json};

/// Repetições do gate (rápido e estável).
const GATE_REPS: u32 = 400;
/// Aquecimento antes de medir (caches de layout do `ratatui`).
const WARMUP: u32 = 40;
/// Artefacto cru commitado.
const ARTIFACT: &str = "bench/render/frame.json";
/// Orçamento versionado.
const BUDGET: &str = "bench/render/budget.toml";
/// Tamanho do terminal sintético (colunas × linhas).
const WIDTH: u16 = 120;
/// Altura do terminal sintético.
const HEIGHT: u16 = 40;
/// Entradas de conversa no cenário (no teto de E10-T03).
const ENTRIES: usize = 200;

/// Orçamento versionado (E15-T01).
#[derive(Debug, Deserialize)]
struct BudgetFile {
    render: Budget,
}

/// Limites de render, em `nanos`.
#[derive(Debug, Deserialize)]
struct Budget {
    frame_p95_nanos: u64,
}

/// Ponto de entrada de `bench-render`: mede, grava o artefacto e imprime o resumo.
pub(crate) fn run(args: &[String]) {
    let reps = value(args, "--reps")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(GATE_REPS);
    let out = value(args, "--out").unwrap_or(ARTIFACT);
    let samples = match frame_samples(reps) {
        Ok(samples) => samples,
        Err(error) => {
            println!("{{\"error\":\"{error}\"}}");
            return;
        }
    };
    let (summary, p99, max) = match aggregate(&samples) {
        Ok(aggregate) => aggregate,
        Err(error) => {
            println!("{{\"error\":\"{error}\"}}");
            return;
        }
    };
    let artifact = json!({
        "schema": 1,
        "machine": { "os": std::env::consts::OS, "arch": std::env::consts::ARCH },
        "backend": "TestBackend",
        "size": { "width": WIDTH, "height": HEIGHT },
        "entries": ENTRIES,
        "reps": reps,
        "frame_nanos": {
            "p50": summary.p50,
            "p95": summary.p95,
            "p99": p99,
            "max": max,
            "mean": summary.mean,
            "ci95": ci95_json(&summary),
        },
        "raw_nanos": samples,
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
    println!(
        "{{\"artifact\":\"{out}\",\"reps\":{reps},\"frame_p95_nanos\":{}}}",
        summary.p95
    );
}

/// Ponto de entrada de `gate:render`: trava a regressão contra o orçamento versionado.
pub(crate) fn gate(args: &[String]) -> Result<(), String> {
    let path = args.first().map_or(BUDGET, String::as_str);
    let text = std::fs::read_to_string(path).map_err(|err| format!("lendo {path}: {err}"))?;
    let file: BudgetFile =
        toml::from_str(&text).map_err(|err| format!("{path} inválido: {err}"))?;
    let samples = frame_samples(GATE_REPS)?;
    let (summary, _, _) = aggregate(&samples)?;
    let budget = file.render.frame_p95_nanos;
    let artifact =
        std::fs::read_to_string(ARTIFACT).map_err(|err| format!("lendo {ARTIFACT}: {err}"))?;
    serde_json::from_str::<Value>(&artifact)
        .map_err(|err| format!("{ARTIFACT} inválido: {err}"))?;
    if !summary.within_budget(budget) {
        return Err(format!(
            "gate:render falhou: quadro IC95 superior={} ns > orçamento {budget} ns (p95={})",
            summary.ci95_high, summary.p95
        ));
    }
    println!(
        "gate:render ok: quadro p95={} ns CI95=[{}, {}] (orçamento {budget} ns)",
        summary.p95, summary.ci95_low, summary.ci95_high
    );
    Ok(())
}

/// Mede `reps` quadros completos num backend de teste (determinístico, sem terminal).
#[allow(
    clippy::disallowed_methods,
    reason = "instrumento dev-only: o relógio monotónico é o objeto da medição"
)]
fn frame_samples(reps: u32) -> Result<Vec<u64>, String> {
    let app = scenario();
    let backend = TestBackend::new(WIDTH, HEIGHT);
    let mut terminal = Terminal::new(backend).map_err(|error| error.to_string())?;
    for _ in 0..WARMUP {
        terminal
            .draw(|frame| render(frame, &app))
            .map_err(|error| error.to_string())?;
    }
    let mut samples = Vec::with_capacity(usize::try_from(reps).unwrap_or(0));
    for _ in 0..reps {
        let start = Instant::now();
        terminal
            .draw(|frame| render(frame, &app))
            .map_err(|error| error.to_string())?;
        samples.push(nanos(start.elapsed()));
    }
    Ok(samples)
}

/// Cenário sintético determinístico: conversa cheia, fase e painel de atividade.
fn scenario() -> App {
    let mut app = App::new();
    app.apply_update(Update::Models(vec![
        "qwen3".to_string(),
        "deepseek-v4.1-flash".to_string(),
    ]));
    app.apply_update(Update::Phase("implemented".to_string()));
    for index in 0..ENTRIES {
        match index % 4 {
            0 => app.apply_update(Update::Assistant(format!(
                "resposta {index} com texto suficiente para ocupar largura no painel"
            ))),
            1 => app.apply_update(Update::Tool(format!("read src/module_{index}.rs"))),
            2 => app.apply_update(Update::Info(format!("nota {index}"))),
            _ => app.apply_update(Update::Error(format!(
                "recusa {index}: contain-sensitive-read"
            ))),
        }
    }
    app.apply_update(Update::Live(Live::Tool {
        name: "grep".to_string(),
        args: "{}".to_string(),
    }));
    app.apply_update(Update::Live(Live::Text("a responder ao vivo ".repeat(8))));
    app
}

/// Nanos de uma `Duration` (saturando).
fn nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

/// Lê o valor de `--chave`.
fn value<'a>(args: &'a [String], key: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == key)?;
    args.get(index.saturating_add(1)).map(String::as_str)
}
