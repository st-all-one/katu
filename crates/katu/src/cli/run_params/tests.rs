//! Testes de precedência de `run`/`tui` (E20-T17): flags > `--params` > config > default.

use katu_core::provider::Thinking;

use super::{Flags, RunParams};
use crate::defaults::Defaults;
use crate::report::Output;

/// Flags com só o pensamento definido.
fn thinking_flag(value: Option<&str>) -> Flags {
    Flags {
        thinking: value.map(str::to_string),
        ..Flags::default()
    }
}

/// `--params` com só o pensamento definido.
fn thinking_params(value: Option<&str>) -> RunParams {
    RunParams {
        thinking: value.map(str::to_string),
        ..RunParams::default()
    }
}

/// Config com só o pensamento definido.
fn thinking_config(value: &str) -> Defaults {
    Defaults {
        thinking: Some(value.to_string()),
        ..Defaults::default()
    }
}

#[test]
fn flag_beats_params_and_config() -> Result<(), Box<dyn std::error::Error>> {
    let config = thinking_flag(Some("high")).assemble(
        "g".to_string(),
        thinking_params(Some("medium")),
        &thinking_config("low"),
    )?;
    assert_eq!(config.thinking, Some(Thinking::High));
    Ok(())
}

#[test]
fn params_beat_config() -> Result<(), Box<dyn std::error::Error>> {
    let config = Flags::default().assemble(
        "g".to_string(),
        thinking_params(Some("medium")),
        &thinking_config("low"),
    )?;
    assert_eq!(config.thinking, Some(Thinking::Medium));
    Ok(())
}

#[test]
fn config_applies_when_no_override() -> Result<(), Box<dyn std::error::Error>> {
    let config = Flags::default().assemble(
        "g".to_string(),
        RunParams::default(),
        &thinking_config("low"),
    )?;
    assert_eq!(config.thinking, Some(Thinking::Low));
    Ok(())
}

#[test]
fn invalid_thinking_is_refused() {
    let refused = Flags::default().assemble(
        "g".to_string(),
        thinking_params(Some("turbo")),
        &Defaults::default(),
    );
    assert!(refused.is_err());
}

#[test]
fn output_defaults_to_text_and_stream_json_resolves() -> Result<(), Box<dyn std::error::Error>> {
    let plain =
        Flags::default().assemble("g".to_string(), RunParams::default(), &Defaults::default())?;
    assert_eq!(plain.output, Output::Text);
    let stream = Flags {
        output: Some("stream-json".to_string()),
        ..Flags::default()
    }
    .assemble("g".to_string(), RunParams::default(), &Defaults::default())?;
    assert_eq!(stream.output, Output::StreamJson);
    Ok(())
}

#[test]
fn json_conflicts_with_a_different_output() {
    let flags = Flags {
        json: true,
        output: Some("text".to_string()),
        ..Flags::default()
    };
    assert!(
        flags
            .assemble("g".to_string(), RunParams::default(), &Defaults::default())
            .is_err()
    );
}

#[test]
fn an_unknown_output_is_rejected() {
    assert!(Output::parse("bogus").is_err());
}
