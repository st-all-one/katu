use super::{CHECKPOINT_SCHEMA_VERSION, Checkpoint, CheckpointError, load, save, validate};
use crate::kernel::event::{CallId, Event};
use crate::kernel::state::State;
use crate::kernel::step::step;
use crate::ports::{Fs, MemFs};
use katu_policy::{Phase, ResolvedPath, ToolArgs, ToolName, ToolUse};
use serde_json::{Value, json};
use std::path::Path;

fn write_tool() -> Result<ToolUse, katu_policy::PolicyError> {
    let path = ResolvedPath::from_canonical("/work/src/main.rs")?;
    Ok(ToolUse {
        name: ToolName::Write,
        args: ToolArgs::Write {
            path: path.clone(),
            bytes: 1,
        },
        resolved_paths: vec![path.clone()],
        argv: None,
        cwd: path,
    })
}

fn valid_value() -> Value {
    json!({
        "schema_version": CHECKPOINT_SCHEMA_VERSION,
        "goal": "provar a tese",
        "state": "Planned turn=1",
        "pending": ["c1"],
        "next_action": "implementar",
        "findings": [],
        "phase": "planned",
    })
}

#[test]
fn valid_value_parses() -> Result<(), CheckpointError> {
    let checkpoint = validate(&valid_value())?;
    assert_eq!(checkpoint.schema_version, CHECKPOINT_SCHEMA_VERSION);
    assert_eq!(checkpoint.phase, Phase::Planned);
    assert_eq!(checkpoint.pending, vec!["c1".to_string()]);
    Ok(())
}

#[test]
fn missing_field_is_invalid() {
    let mut value = valid_value();
    if let Some(object) = value.as_object_mut() {
        object.remove("goal");
    }
    assert!(matches!(validate(&value), Err(CheckpointError::Invalid(_))));
}

#[test]
fn unknown_field_is_rejected() {
    let mut value = valid_value();
    if let Some(object) = value.as_object_mut() {
        object.insert("surpresa".to_string(), json!(true));
    }
    assert!(matches!(validate(&value), Err(CheckpointError::Invalid(_))));
}

#[test]
fn wrong_schema_version_is_rejected() {
    let mut value = valid_value();
    if let Some(object) = value.as_object_mut() {
        object.insert("schema_version".to_string(), json!(99));
    }
    assert!(matches!(
        validate(&value),
        Err(CheckpointError::SchemaVersion { found: 99, .. })
    ));
}

#[test]
fn multiple_issues_are_aggregated_with_exact_paths() -> Result<(), Box<dyn std::error::Error>> {
    let value = json!({
        "schema_version": CHECKPOINT_SCHEMA_VERSION,
        "state": "s",
        "pending": 7,
        "next_action": "n",
        "findings": [],
        "phase": "bogus",
        "surpresa": true,
    });
    let Err(CheckpointError::Invalid(issues)) = validate(&value) else {
        return Err("esperava Invalid agregado".into());
    };
    let paths: Vec<&str> = issues
        .as_slice()
        .iter()
        .map(|issue| issue.path.as_str())
        .collect();
    for expected in ["goal", "pending", "phase", "surpresa"] {
        assert!(paths.contains(&expected), "falta {expected} em {paths:?}");
    }
    assert!(issues.len() >= 4);
    Ok(())
}

#[test]
fn absent_and_corrupt_are_distinct() -> Result<(), CheckpointError> {
    let fs = MemFs::new();
    let dir = Path::new("/sessions");
    assert_eq!(load(&fs, dir)?, None);

    fs.write_atomic(&super::checkpoint_path(dir), b"{nao json")?;
    assert!(matches!(load(&fs, dir), Err(CheckpointError::Parse(_))));
    Ok(())
}

#[test]
fn round_trip_write_and_read() -> Result<(), Box<dyn std::error::Error>> {
    let fs = MemFs::new();
    let dir = Path::new("/sessions");
    let checkpoint = Checkpoint {
        schema_version: CHECKPOINT_SCHEMA_VERSION,
        goal: "g".to_string(),
        state: "s".to_string(),
        pending: Vec::new(),
        next_action: "n".to_string(),
        findings: vec!["f".to_string()],
        phase: Phase::Verified,
    };
    save(&fs, dir, &checkpoint)?;
    assert_eq!(load(&fs, dir)?, Some(checkpoint));
    Ok(())
}

#[test]
fn from_state_lists_pending_calls() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = State::initial();
    state = step(&state, &Event::TurnStart { turn: 1 })?;
    state = step(
        &state,
        &Event::ToolCall {
            call: CallId::new("c1"),
            tool: write_tool()?,
        },
    )?;
    let checkpoint = Checkpoint::from_state(&state, "objetivo", "próxima");
    assert_eq!(checkpoint.phase, Phase::Task);
    assert_eq!(checkpoint.pending, vec!["c1".to_string()]);
    assert_eq!(checkpoint.goal, "objetivo");
    Ok(())
}
