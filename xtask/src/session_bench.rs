//! A/B da retomada de sessão (ADR 0008). **Dev-only**: `cargo run -p xtask -- bench-resume`.
//!
//! Compara a retomada por **snapshot+cauda** (leitura ranged, sem reler o prefixo) com o
//! **replay total** do log, a 6k/60k eventos. É o que decide manter o offset no snapshot (DF5).
#![allow(
    clippy::print_stdout,
    reason = "micro-bench dev-only: imprime a tabela"
)]

use std::path::Path;
use std::time::Instant;

use katu_core::kernel::{Event, Session, SessionId};
use katu_core::ports::MemFs;
use katu_policy::Phase;

/// Número de turnos medidos (cada turno = 3 eventos).
const TURNS: [u32; 2] = [2_000, 20_000];

/// Se a sessão leva snapshot intermédio (cauda) ou é log puro (replay total).
#[derive(Clone, Copy)]
enum Mode {
    /// Snapshot a 90% + cauda.
    Walked,
    /// Sem snapshot (replay total).
    Plain,
}

/// Corre o A/B e imprime a tabela.
pub(crate) fn run() {
    if let Err(message) = measure() {
        println!("bench-resume erro: {message}");
    }
}

fn measure() -> Result<(), String> {
    println!(
        "{:<8} {:>12} {:>12} {:>8}",
        "turns", "resume(us)", "full(us)", "gain"
    );
    for turns in TURNS {
        let fs = MemFs::new();
        let root = Path::new("/bench");
        let walked = build(&fs, root, turns, Mode::Walked)?;
        let plain = build(&fs, root, turns, Mode::Plain)?;
        let resume_us = micros(5, || {
            Session::resume(&fs, root, &walked).map_or(0, |session| {
                usize::try_from(session.state().turn).unwrap_or(0)
            })
        });
        let full_us = micros(5, || {
            Session::resume(&fs, root, &plain).map_or(0, |session| {
                usize::try_from(session.state().turn).unwrap_or(0)
            })
        });
        let gain = full_us.checked_div(resume_us.max(1)).unwrap_or(0);
        println!("{turns:<8} {resume_us:>12} {full_us:>12} {gain:>7}x");
    }
    Ok(())
}

/// Cria uma sessão com `turns` turnos; em [`Mode::Walked`] grava o estado a 90% e deixa cauda.
fn build(fs: &MemFs, root: &Path, turns: u32, mode: Mode) -> Result<SessionId, String> {
    let snapshot = matches!(mode, Mode::Walked);
    let created_ms = if snapshot { 1 } else { 2 };
    let mut session = Session::create(fs, root, created_ms, "bench").map_err(stringify)?;
    let id = session.id().cloned().ok_or("sessão sem id")?;
    let mark = turns
        .checked_mul(9)
        .unwrap_or(0)
        .checked_div(10)
        .unwrap_or(0);
    for turn in 0..turns {
        if snapshot && turn == mark {
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
        }
        session
            .apply(&Event::TurnStart { turn })
            .map_err(stringify)?;
        session
            .apply(&Event::UserMessage {
                text: format!("pedido {turn}: densidade e retomada"),
            })
            .map_err(stringify)?;
        session.apply(&Event::TurnEnd { turn }).map_err(stringify)?;
    }
    Ok(id)
}

/// Mede `iterations` execuções e devolve microssegundos por operação.
#[allow(
    clippy::disallowed_methods,
    reason = "bench dev-only: medição de tempo de parede com `Instant`"
)]
fn micros(iterations: u32, mut run: impl FnMut() -> usize) -> u64 {
    let start = Instant::now();
    for _ in 0..iterations {
        std::hint::black_box(run());
    }
    let elapsed = start.elapsed().as_micros();
    u64::try_from(elapsed)
        .unwrap_or(u64::MAX)
        .checked_div(u64::from(iterations))
        .unwrap_or(0)
}

fn stringify(error: impl std::fmt::Display) -> String {
    error.to_string()
}
