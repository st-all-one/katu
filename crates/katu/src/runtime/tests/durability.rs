//! A/B da durabilidade do log (P-01, ADR 0024). **Dev-only**: corre com `--ignored`.
//!
//! Mede, em **release e em disco real**, o custo de anexar eventos com:
//!
//! - `event` — uma barreira (`fsync`) por evento (default histórico);
//! - `turn` — uma barreira por turno (*group commit*).
//!
//! O conteúdo do log é **idêntico** nos dois modos (a política só muda *quando* se sincroniza): o
//! teste de CI (`the_two_modes_write_the_same_log`) afirma-o byte a byte.
//!
//! Corre-se com um diretório em **disco real** (não `tmpfs`, onde o `fsync` é grátis e a medição não
//! diria nada):
//!
//! ```sh
//! KATU_DURABILITY_DIR=$(pwd)/target/durability-bench \
//! KATU_DURABILITY_OUT=$(pwd)/bench/e18/durability/raw.json \
//!   cargo test -q --release -p katu --bin katu -- --ignored ab_durability
//! ```

use katu_core::kernel::Visibility;
use std::path::{Path, PathBuf};
use std::time::Instant;

use katu_core::error::ToolOutcome;
use katu_core::kernel::{CallId, Durability, Event, Session};
use katu_core::ports::Env;

use crate::ports::{StdEnv, StdFs};

/// Turnos por passagem e eventos por turno (o caminho real tem ~5).
const TURNS: u32 = 20;
/// Eventos anexados por turno (`TurnStart` + pedido + 2 tool calls + `TurnEnd`).
const EVENTS_PER_TURN: u32 = 5;
/// Repetições por modo.
const REPS: u32 = 3;

/// Ponto de entrada do bench.
#[test]
#[ignore = "bench dev-only: escreve o artefacto em KATU_DURABILITY_OUT"]
fn ab_durability() -> Result<(), Box<dyn std::error::Error>> {
    let dir = StdEnv
        .var("KATU_DURABILITY_DIR")
        .ok_or("KATU_DURABILITY_DIR ausente (tem de ser disco real, não tmpfs)")?;
    let out = StdEnv
        .var("KATU_DURABILITY_OUT")
        .ok_or("KATU_DURABILITY_OUT ausente")?;
    let root = PathBuf::from(&dir);
    let tmp = std::env::temp_dir();
    if root.starts_with(&tmp) {
        return Err(format!(
            "{} está sob {} (tmpfs): o `fsync` seria grátis e a medição não diria nada",
            root.display(),
            tmp.display()
        )
        .into());
    }
    let mut rows = Vec::new();
    for mode in [Durability::Event, Durability::Turn] {
        rows.push(measure(&root, mode)?);
    }
    let artifact = serde_json::json!({
        "schema": "katu.bench.durability.v1",
        "machine": { "os": std::env::consts::OS, "arch": std::env::consts::ARCH },
        "dir": dir,
        "turns": TURNS,
        "events_per_turn": EVENTS_PER_TURN,
        "reps": REPS,
        "rows": rows,
        "limits": [
            "mede `Session::apply` no disco real (fsync incluído); o conteúdo do log é idêntico nos dois modos",
            "o número depende do disco: um tmpfs ou um NVMe rápido mudam a razão",
        ],
    });
    std::fs::write(
        &out,
        format!("{}\n", serde_json::to_string_pretty(&artifact)?),
    )?;
    Ok(())
}

/// Mede um modo: `(nanos por passagem, nanos por evento, ficheiro do log)`.
#[allow(
    clippy::disallowed_methods,
    reason = "instrumento dev-only: o relógio monotónico é o objeto da medição"
)]
fn measure(root: &Path, mode: Durability) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut samples = Vec::new();
    let mut bytes = 0_u64;
    for rep in 0..REPS {
        let dir = root.join(format!("{}-{rep}", mode.as_str()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir)?;
        let fs = StdFs;
        let mut session = Session::create(&fs, &dir, 1_000, "bench")?;
        session.set_durability(mode);
        let start = Instant::now();
        for turn in 0..TURNS {
            apply_turn(&mut session, turn)?;
        }
        samples.push(u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX));
        bytes = u64::try_from(std::fs::read(session.log_path())?.len()).unwrap_or(0);
        std::fs::remove_dir_all(&dir).ok();
    }
    samples.sort_unstable();
    let median = samples.get(samples.len() / 2).copied().unwrap_or(0);
    let events = u64::from(TURNS).saturating_mul(u64::from(EVENTS_PER_TURN));
    Ok(serde_json::json!({
        "mode": mode.as_str(),
        "median_nanos": median,
        "per_event_nanos": median.checked_div(events).unwrap_or(0),
        "events": events,
        "log_bytes": bytes,
    }))
}

/// Aplica um turno com `EVENTS_PER_TURN` eventos (`TurnStart` + pedido + tool call/resultado + `TurnEnd`).
fn apply_turn(session: &mut Session<'_>, turn: u32) -> Result<(), Box<dyn std::error::Error>> {
    session.apply(&Event::TurnStart { turn })?;
    session.apply(&Event::UserMessage {
        text: format!("pedido {turn}"),
        visibility: Visibility::User,
    })?;
    let call = CallId::new(format!("c{turn}"));
    session.apply(&Event::ToolCall {
        call: call.clone(),
        tool: read_use()?,
    })?;
    session.apply(&Event::ToolResult {
        call,
        outcome: ToolOutcome::Ok,
        delta: None,
    })?;
    session.apply(&Event::TurnEnd { turn })?;
    Ok(())
}

/// Uso de tool mínimo (leitura de um caminho fixo).
fn read_use() -> Result<katu_policy::ToolUse, katu_policy::PolicyError> {
    let path = katu_policy::ResolvedPath::from_canonical("/work/nota.txt")?;
    Ok(katu_policy::ToolUse {
        name: katu_policy::ToolName::Read,
        args: katu_policy::ToolArgs::Read { path: path.clone() },
        resolved_paths: vec![path.clone()],
        argv: None,
        cwd: path,
    })
}

#[cfg(test)]
mod tests {
    use super::{apply_turn, read_use};
    use katu_core::kernel::{Durability, Session};
    use katu_core::ports::{Fs, MemFs};
    use std::path::Path;

    /// O modo **não** muda o conteúdo: só muda quando se sincroniza.
    #[test]
    fn the_two_modes_write_the_same_log() -> Result<(), Box<dyn std::error::Error>> {
        let mut logs = Vec::new();
        for mode in [Durability::Event, Durability::Turn] {
            let fs = MemFs::new();
            let root = Path::new("/bench");
            let mut session = Session::create(&fs, root, 1_000, "bench")?;
            session.set_durability(mode);
            for turn in 0..3 {
                apply_turn(&mut session, turn)?;
            }
            logs.push(fs.read(session.log_path())?);
        }
        assert_eq!(logs.first(), logs.get(1), "o log tem de ser idêntico");
        assert!(read_use().is_ok());
        Ok(())
    }
}
