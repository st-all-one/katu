//! Harness de medição do MVK (E05-T06/E15-T02): casos **positivo, negativo e no-op**.
//!
//! Corre o **caminho real** (sessão + gate de memória + política), recolhe contadores e durações
//! (sink agregador, E19-T02) e grava um **artefacto cru** em `bench/mvk/raw.json`. Os números
//! publicados em `bench/published.toml` citam este artefacto (DF5) — nenhum número sem base.
//!
//! Uso: `cargo run -p katu --features profile --example measure_mvk [saída.json]`

use std::error::Error;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use katu_core::diag::{Level, aggregate::AggregatingSink, events, install, set_level};
use katu_core::error::ToolOutcome;
use katu_core::kernel::{CallId, Event, MemoryWriteRequest, Session};
use katu_core::memory::{FakeMemory, NoteType, PreWriteReq};
use katu_core::ports::MemFs;
use katu_policy::{ResolvedPath, RuleSet, ToolArgs, ToolName, ToolUse};
use katu_tools::write::WriteNoteTool;
use serde_json::json;

/// Regras reais do protocolo, versionadas no repositório.
const MEMORY_POLICY: &str = include_str!("../../../policy/memory.toml");
/// Artefacto por omissão.
const DEFAULT_OUT: &str = "bench/mvk/raw.json";
/// Repetições para os percentis de tempo.
const REPS: usize = 500;

type HarnessResult<T> = Result<T, Box<dyn Error>>;

/// Cenário do protocolo de medição.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scenario {
    /// Recall antes da escrita; a escrita é permitida.
    Positive,
    /// Escrita sem recall; deve ser negada.
    Negative,
    /// Só leitura; nenhuma escrita tentada.
    NoOp,
}

impl Scenario {
    /// Se o cenário executa `memory_recall` antes.
    const fn recalls(self) -> bool {
        !matches!(self, Self::Negative)
    }

    /// Se o cenário tenta escrever.
    const fn writes(self) -> bool {
        !matches!(self, Self::NoOp)
    }
}

/// Tipo de recusa observado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutcomeKind {
    /// Sem recusa.
    None,
    /// Negação fechada (`Denied`).
    Denied,
    /// Controlo em falta (`Unavailable`).
    Soft,
}

impl OutcomeKind {
    /// Se houve recusa.
    const fn refused(self) -> bool {
        !matches!(self, Self::None)
    }
}

/// Contadores de uma passagem.
#[derive(Debug, Clone, Copy)]
struct Counts {
    /// Commits efetivos na porta de memória.
    commits: usize,
    /// Tipo de recusa.
    outcome: OutcomeKind,
}

fn rules() -> HarnessResult<RuleSet> {
    Ok(RuleSet::from_toml(MEMORY_POLICY)?)
}

fn request() -> PreWriteReq {
    PreWriteReq {
        statement: "cache usa LRU".to_string(),
        note_type: NoteType::Decision,
        anchor: None,
        body: String::new(),
    }
}

fn recall_use(cwd: &ResolvedPath) -> ToolUse {
    ToolUse {
        name: ToolName::MemoryRecall,
        args: ToolArgs::Other,
        resolved_paths: Vec::new(),
        argv: None,
        cwd: cwd.clone(),
    }
}

/// Corre uma passagem do cenário pelo caminho real e devolve os contadores.
fn run(scenario: Scenario) -> HarnessResult<Counts> {
    let fs = MemFs::new();
    let mut session = Session::open(&fs, Path::new("/sessions"))?;
    session.apply(&Event::TurnStart { turn: 1 })?;
    let memory = FakeMemory::default();
    let cwd = ResolvedPath::from_canonical("/work")?;

    if scenario.recalls() {
        session.apply(&Event::ToolCall {
            call: CallId::new("r1"),
            tool: recall_use(&cwd),
        })?;
        session.apply(&Event::ToolResult {
            call: CallId::new("r1"),
            outcome: ToolOutcome::Ok,
            delta: None,
        })?;
    }
    if !scenario.writes() {
        return Ok(Counts {
            commits: 0,
            outcome: OutcomeKind::None,
        });
    }

    let req = request();
    let tool = WriteNoteTool {
        memory: &memory,
        req: req.clone(),
    };
    let dispatch = session.memory_write(
        CallId::new("w1"),
        MemoryWriteRequest {
            cwd: &cwd,
            req: &req,
            memory: &memory,
            rules: &rules()?,
            now_millis: 0,
            tool: &tool,
        },
    )?;
    let outcome = match dispatch.outcome() {
        ToolOutcome::Denied { .. } => OutcomeKind::Denied,
        ToolOutcome::Unavailable { .. } => OutcomeKind::Soft,
        _ => OutcomeKind::None,
    };
    Ok(Counts {
        commits: memory.recorded(),
        outcome,
    })
}

fn main() -> HarnessResult<()> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_OUT.to_string());

    let sink = Arc::new(AggregatingSink::default());
    install(sink.clone());
    set_level(Level::Trace);

    let positive = run(Scenario::Positive)?;
    let negative = run(Scenario::Negative)?;
    let noop = run(Scenario::NoOp)?;
    for _ in 0..REPS {
        run(Scenario::Positive)?;
    }

    let timings = sink.snapshot();
    let write_timing = timings.iter().find(|s| s.event == events::MEMORY_WRITE);
    let scenario_json = |counts: Counts| {
        json!({
            "commits": counts.commits,
            "refused": counts.outcome.refused(),
            "denied": counts.outcome == OutcomeKind::Denied,
        })
    };
    let artifact = json!({
        "schema": 1,
        "machine": { "os": std::env::consts::OS, "arch": std::env::consts::ARCH },
        "reps": REPS,
        "scenarios": {
            "positive": scenario_json(positive),
            "negative": scenario_json(negative),
            "noop": scenario_json(noop),
        },
        "timings": timings.iter().map(|s| json!({
            "event": s.event,
            "function": s.function,
            "count": s.count,
            "total_nanos": s.total_nanos,
            "min_nanos": s.min_nanos,
            "p50_nanos": s.p50_nanos,
            "p95_nanos": s.p95_nanos,
            "p99_nanos": s.p99_nanos,
            "max_nanos": s.max_nanos,
            "ci95": { "low_nanos": s.ci95_low_nanos, "high_nanos": s.ci95_high_nanos },
        })).collect::<Vec<_>>(),
    });

    if let Some(parent) = Path::new(&out).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&out, serde_json::to_vec_pretty(&artifact)?)?;

    let mut stdout = std::io::stdout();
    writeln!(
        stdout,
        "measure_mvk: positivo={} negativo={}(denied={}) no-op={}; memory.write p50={:?} p95={:?} -> {out}",
        positive.commits,
        negative.commits,
        negative.outcome == OutcomeKind::Denied,
        noop.commits,
        write_timing.map(|s| s.p50_nanos),
        write_timing.map(|s| s.p95_nanos),
    )?;
    Ok(())
}
