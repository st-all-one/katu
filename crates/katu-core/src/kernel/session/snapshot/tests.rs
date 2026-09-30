//! Testes do snapshot de estado (ADR 0008).

use std::path::Path;

use super::{SNAPSHOT_SCHEMA_VERSION, StateSnapshot, load, save};
use crate::kernel::State;
use crate::kernel::budget::Budget;
use crate::ports::MemFs;

#[test]
fn round_trips_and_defaults_to_absent() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/s");
    assert_eq!(load(&fs, dir), None);
    let snapshot = StateSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        seq: 7,
        offset: 0,
        budget: Budget::ZERO,
        per_tool: std::collections::BTreeMap::new(),
        history: Vec::new(),
        state: State::initial(),
    };
    save(&fs, dir, &snapshot)?;
    assert_eq!(load(&fs, dir), Some(snapshot));
    Ok(())
}
