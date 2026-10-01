//! Testes do snapshot de estado (ADR 0008, Q-15).

use std::collections::BTreeMap;
use std::path::Path;

use super::{SNAPSHOT_SCHEMA_VERSION, StateSnapshot, load, save};
use crate::kernel::State;
use crate::kernel::budget::Budget;
use crate::ports::{Fs, MemFs};

fn snapshot(seq: u64, state: State) -> StateSnapshot {
    StateSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        seq,
        offset: 0,
        budget: Budget::ZERO,
        per_tool: BTreeMap::new(),
        history: Vec::new(),
        state,
        hash: 0,
    }
}

#[test]
fn round_trips_and_defaults_to_absent() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/s");
    assert_eq!(load(&fs, dir), None);
    let written = save(&fs, dir, snapshot(7, State::initial()))?;
    assert_ne!(written.hash, 0, "o hash é derivado do estado ao gravar");
    assert_eq!(load(&fs, dir), Some(written));
    Ok(())
}

#[test]
fn a_snapshot_that_does_not_match_its_state_is_discarded() -> Result<(), Box<dyn std::error::Error>>
{
    let fs = MemFs::new();
    let dir = Path::new("/s");
    let written = save(&fs, dir, snapshot(7, State::initial()))?;
    // Adultera só o hash (o estado continua a desserializar): o snapshot tem de ser descartado.
    let path = super::snapshot_path(dir);
    let text = String::from_utf8(fs.read(&path)?)?;
    let tampered = text.replace(
        &format!("\"hash\":{}", written.hash),
        &format!("\"hash\":{}", written.hash.wrapping_add(1)),
    );
    assert_ne!(text, tampered, "a adulteração tem de mudar o ficheiro");
    fs.write_atomic(&path, tampered.as_bytes())?;
    assert_eq!(load(&fs, dir), None, "hash divergente ⇒ replay total");
    Ok(())
}

#[test]
fn an_old_schema_is_ignored() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/s");
    let mut old = snapshot(7, State::initial());
    old.schema_version = SNAPSHOT_SCHEMA_VERSION.saturating_sub(1);
    let path = super::snapshot_path(dir);
    fs.write_atomic(&path, serde_json::to_vec(&old)?.as_slice())?;
    assert_eq!(load(&fs, dir), None);
    Ok(())
}

#[test]
fn the_state_hash_is_stable_and_content_sensitive() -> Result<(), Box<dyn std::error::Error>> {
    let first = StateSnapshot::state_hash(&State::initial())?;
    assert_eq!(StateSnapshot::state_hash(&State::initial())?, first);
    let mut other = State::initial();
    other.turn = 3;
    assert_ne!(StateSnapshot::state_hash(&other)?, first);
    Ok(())
}
