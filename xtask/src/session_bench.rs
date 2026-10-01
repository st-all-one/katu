//! A/B da retomada de sessão (ADR 0008, Q-15). **Dev-only**: `cargo run -p xtask -- bench-resume`.
//!
//! Compara três políticas de snapshot à medida que a história cresce:
//!
//! - `replay` — sem snapshot nenhum (replay total do log): o custo cresce com a história;
//! - `phase` — snapshot só na **fronteira de fase** (comportamento até Q-15): a cauda é o que ficou
//!   desde a última transição, que cresce com o comprimento da fase;
//! - `bounded` — fronteira de fase **e** fim de turno quando a cauda passa de [`MAX_TAIL_BYTES`]
//!   (Q-15): o que a retomada relê tem teto.
//!
//! Escreve `bench/e18/resume/raw.json` (DF5: nenhum número publicado sem artefacto cru) e imprime a
//! tabela. Mede **tempo de parede** de propósito: é o objeto da medição.
#![allow(
    clippy::print_stdout,
    reason = "micro-bench dev-only: imprime a tabela"
)]

use std::path::{Path, PathBuf};
use std::time::Instant;

use katu_core::error::ToolOutcome;
use katu_core::kernel::{CallId, Event, MAX_TAIL_BYTES, Session, SessionId};
use katu_core::ports::{Fs, MemFs};
use katu_policy::{Phase, ResolvedPath, ToolArgs, ToolName, ToolUse};
use serde_json::json;

/// Cenários medidos: turnos e quantas tool calls por turno (para pesar o mapa de chamadas do estado).
const SCENARIOS: [(u32, u32); 3] = [(2_000, 0), (20_000, 0), (2_000, 5)];

/// Repetições por medição (mediana).
const REPS: u32 = 7;

/// Política de snapshot em vigor **no fim** da construção da sessão.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Sem snapshot (replay total): mede o custo da história.
    Replay,
    /// Um só snapshot, na fronteira de fase a 90% (comportamento anterior a Q-15).
    Phase,
    /// Fronteira de fase + teto de cauda (Q-15, produção).
    Bounded,
}

impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Replay => "replay",
            Self::Phase => "phase",
            Self::Bounded => "bounded",
        }
    }
}

/// Caso medido (agrupa os argumentos: o limite do projeto é 5 parâmetros por função).
#[derive(Clone, Copy)]
struct Case {
    created_ms: u64,
    turns: u32,
    calls_per_turn: u32,
    mode: Mode,
}

/// Resultado de uma medição.
struct Row {
    mode: &'static str,
    turns: u32,
    calls_per_turn: u32,
    resume_us: u64,
    log_bytes: u64,
    snapshots: u64,
    tail_bytes: u64,
}

/// Ponto de entrada do `xtask bench-resume`.
pub(crate) fn run() {
    if let Err(message) = measure() {
        println!("bench-resume erro: {message}");
    }
}

fn measure() -> Result<(), String> {
    println!(
        "{:<8} {:>7} {:>6} {:>12} {:>12} {:>10} {:>10}",
        "mode", "turns", "calls", "resume(us)", "log(bytes)", "snapshots", "tail(bytes)"
    );
    let mut rows = Vec::new();
    let mut case = 0_u64;
    for (turns, calls_per_turn) in SCENARIOS {
        for mode in [Mode::Replay, Mode::Phase, Mode::Bounded] {
            let fs = MemFs::new();
            let root = Path::new("/bench");
            // Cada caso tem a sua sessão: `Session::create` deriva o id do relógio, pelo que dois
            // casos com o mesmo instante partilhariam o log (medição contaminada).
            case = case.saturating_add(1);
            let subject = Case {
                created_ms: case,
                turns,
                calls_per_turn,
                mode,
            };
            let (id, snapshots, log_bytes) = build(&fs, root, subject)?;
            let (resume_us, tail_bytes) = resume_us(&fs, root, &id)?;
            println!(
                "{:<8} {turns:>7} {calls_per_turn:>6} {resume_us:>12} {log_bytes:>12} {snapshots:>10} {tail_bytes:>10}",
                mode.as_str()
            );
            rows.push(Row {
                mode: mode.as_str(),
                turns,
                calls_per_turn,
                resume_us,
                log_bytes,
                snapshots,
                tail_bytes,
            });
        }
    }
    let artifact = artifact(&rows);
    let out = "bench/e18/resume/raw.json";
    let pretty = serde_json::to_string_pretty(&artifact).map_err(|err| err.to_string())?;
    std::fs::write(out, format!("{pretty}\n")).map_err(|err| format!("gravando {out}: {err}"))?;
    println!("artefacto: {out}");
    Ok(())
}

/// Cria a sessão do cenário e devolve `(id, snapshots em disco, bytes do log)`.
///
/// A política é imposta **no fim**: a `Bounded` deixa o snapshot da produção; a `Phase` restaura o
/// snapshot tirado a 90% (um só); a `Replay` apaga-o. É o que permite comparar as três no mesmo log.
fn build(fs: &MemFs, root: &Path, subject: Case) -> Result<(SessionId, u64, u64), String> {
    let Case {
        created_ms,
        turns,
        calls_per_turn,
        mode,
    } = subject;
    let mut session = Session::create(fs, root, created_ms, "bench").map_err(stringify)?;
    let id = session.id().cloned().ok_or("sessão sem id")?;
    let path = snapshot_path(&session);
    let mark = turns
        .checked_mul(9)
        .unwrap_or(0)
        .checked_div(10)
        .unwrap_or(0);
    let mut phase_snapshot: Option<Vec<u8>> = None;
    let mut mechanism = 0_u64;
    let mut previous_tail = 0_u64;
    for turn in 0..turns {
        if mode == Mode::Phase && turn == mark {
            // A fronteira de fase do ADR 0008: um snapshot a 90% da história.
            phase_boundary(&mut session)?;
            phase_snapshot = fs.read(&path).ok();
        }
        one_turn(&mut session, turn, calls_per_turn)?;
        // O snapshot **reinicia** a cauda: uma descida é a marca de um snapshot gravado (Q-15).
        let tail = session.tail_bytes();
        if tail < previous_tail {
            mechanism = mechanism.saturating_add(1);
        }
        previous_tail = tail;
    }
    match mode {
        Mode::Bounded => {}
        Mode::Phase => {
            let bytes = phase_snapshot.ok_or("sem snapshot de fase")?;
            fs.write_atomic(&path, &bytes).map_err(stringify)?;
        }
        Mode::Replay => {
            if fs.exists(&path) {
                fs.remove(&path).map_err(stringify)?;
            }
        }
    }
    let snapshots = match mode {
        Mode::Bounded => mechanism,
        Mode::Phase => 1,
        Mode::Replay => 0,
    };
    let log_bytes =
        u64::try_from(fs.read(session.log_path()).map_err(stringify)?.len()).unwrap_or(0);
    Ok((id, snapshots, log_bytes))
}

/// Aplica um turno sintético (mensagem + `calls` chamadas concluídas).
fn one_turn(session: &mut Session<'_>, turn: u32, calls: u32) -> Result<(), String> {
    session
        .apply(&Event::TurnStart { turn })
        .map_err(stringify)?;
    session
        .apply(&Event::UserMessage {
            text: format!("pedido {turn}: densidade e retomada"),
        })
        .map_err(stringify)?;
    for index in 0..calls {
        let call = CallId::new(format!("c{index}"));
        session
            .apply(&Event::ToolCall {
                call: call.clone(),
                tool: read_use()?,
            })
            .map_err(stringify)?;
        session
            .apply(&Event::ToolResult {
                call,
                outcome: ToolOutcome::Ok,
                delta: Some("ok".to_string()),
            })
            .map_err(stringify)?;
    }
    session.apply(&Event::TurnEnd { turn }).map_err(stringify)
}

/// Caminho do snapshot de uma sessão (o ficheiro é irmão do log).
fn snapshot_path(session: &Session<'_>) -> PathBuf {
    session.log_path().parent().map_or_else(
        || PathBuf::from("snapshot.v1.json"),
        |dir| dir.join("snapshot.v1.json"),
    )
}

/// Uma transição de fase válida (com o waiver da pré-condição).
fn phase_boundary(session: &mut Session<'_>) -> Result<(), String> {
    session
        .apply(&Event::Waiver {
            transition: Phase::KnowledgeConsulted,
            reason: "bench".to_string(),
        })
        .map_err(stringify)?;
    session
        .apply(&Event::PhaseTransition {
            to: Phase::KnowledgeConsulted,
            outcome: None,
        })
        .map_err(stringify)?;
    Ok(())
}

/// Uso de tool mínimo (leitura de um caminho fixo).
fn read_use() -> Result<ToolUse, String> {
    let path = ResolvedPath::from_canonical("/bench/nota.txt").map_err(stringify)?;
    Ok(ToolUse {
        name: ToolName::Read,
        args: ToolArgs::Read { path: path.clone() },
        resolved_paths: vec![path.clone()],
        argv: None,
        cwd: path,
    })
}

/// Mede a retomada (mediana de [`REPS`]) e devolve `(micros, cauda)`.
#[allow(
    clippy::disallowed_methods,
    reason = "bench dev-only: medição de tempo de parede com `Instant`"
)]
fn resume_us(fs: &MemFs, root: &Path, id: &SessionId) -> Result<(u64, u64), String> {
    let mut samples = Vec::with_capacity(usize::try_from(REPS).unwrap_or(0));
    let mut tail = 0_u64;
    for _ in 0..REPS {
        let start = Instant::now();
        let session = Session::resume(fs, root, id).map_err(stringify)?;
        let elapsed = start.elapsed().as_micros();
        tail = session.tail_bytes();
        samples.push(u64::try_from(elapsed).unwrap_or(u64::MAX));
        std::hint::black_box(session.state().turn);
    }
    samples.sort_unstable();
    Ok((samples.get(samples.len() / 2).copied().unwrap_or(0), tail))
}

/// Artefacto cru da medição (DF5).
fn artifact(rows: &[Row]) -> serde_json::Value {
    let pick = |mode: &str| -> Vec<serde_json::Value> {
        rows.iter()
            .filter(|row| row.mode == mode)
            .map(|row| {
                json!({
                    "turns": row.turns,
                    "calls_per_turn": row.calls_per_turn,
                    "resume_us": row.resume_us,
                    "log_bytes": row.log_bytes,
                    "snapshots": row.snapshots,
                    "tail_bytes": row.tail_bytes,
                })
            })
            .collect()
    };
    json!({
        "schema": "katu.bench.resume.v1",
        "machine": { "os": std::env::consts::OS, "arch": std::env::consts::ARCH },
        "reps": REPS,
        "max_tail_bytes": MAX_TAIL_BYTES,
        "replay": pick("replay"),
        "phase": pick("phase"),
        "bounded": pick("bounded"),
        "limits": [
            "MemFs não tem fsync: o custo de gravar o snapshot (tmp→sync_all→rename) não está aqui; \
             o E18 mediu ~3,7 ms por escrita atómica em disco real (7 escritas = 25,8 ms/turno)",
            "as sessões são sintéticas (turnos + tool calls Ok), sem conteúdo de tools: mede-se a \
             retomada, não o trabalho do modelo",
        ],
    })
}

fn stringify(error: impl std::fmt::Display) -> String {
    error.to_string()
}
