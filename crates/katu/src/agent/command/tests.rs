//! Testes do `drive` do CLI (`LIVE_FLOW` LF1/LF5) e do escopo kernel/superfície.

use katu_core::api::{ApprovalRequest, Command, Event, TurnSummary, channel};
use katu_core::error::ErrorKind;
use serde_json::Value;

use super::{drive, envelope, run_scoped};
use crate::report::Output;

fn summary() -> TurnSummary {
    TurnSummary {
        model: "m".to_string(),
        text: "olá".to_string(),
        steps: 2,
        calls: 1,
        cancelled: false,
        stop: "end_turn".to_string(),
        termination: "natural".to_string(),
        round_exit: 0,
        usage: None,
        session: Some("s_1".to_string()),
        state: None,
        selection: "full".to_string(),
    }
}

#[test]
fn run_scoped_returns_when_the_drive_finishes() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let done = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&done);
    run_scoped(
        move |bus| {
            while bus.recv().is_ok() {}
            flag.store(true, Ordering::Relaxed);
        },
        |handle| {
            let _sent = handle.send(Command::Submit("x".to_string()));
        },
    );
    assert!(
        done.load(Ordering::Relaxed),
        "o kernel termina quando o canal fecha (senão o CLI trava no fim do turno)"
    );
}

#[test]
fn drive_collects_the_turn_summary_until_done() {
    let (bus, handle) = channel();
    let responder = std::thread::spawn(move || {
        if let Ok(command) = bus.recv() {
            assert!(matches!(command, Command::Submit(goal) if goal == "olá"));
            bus.publish(Event::Turn(Box::new(summary())));
            bus.publish(Event::Done);
        }
    });
    let summary = drive(&handle, "olá", Output::Text).ok();
    assert_eq!(summary.map(|turn| turn.model), Some("m".to_string()));
    responder.join().ok();
}

#[test]
fn drive_surfaces_a_structured_failure() {
    let (bus, handle) = channel();
    let responder = std::thread::spawn(move || {
        if bus.recv().is_ok() {
            bus.publish(Event::Failure {
                kind: ErrorKind::InvalidInput,
                message: "mau".to_string(),
            });
            bus.publish(Event::Done);
        }
    });
    assert_eq!(
        drive(&handle, "olá", Output::Text),
        Err((ErrorKind::InvalidInput, "mau".to_string()))
    );
    responder.join().ok();
}

#[test]
fn drive_denies_approval_in_a_non_interactive_run() {
    let (bus, handle) = channel();
    let responder = std::thread::spawn(move || {
        if bus.recv().is_ok() {
            bus.publish(Event::ApprovalRequest(ApprovalRequest {
                tool: "bash".to_string(),
                rule: "r".to_string(),
                scope: "s".to_string(),
            }));
            // O cliente tem de responder, senão o kernel bloqueia no `ask`.
            assert!(matches!(bus.recv(), Ok(Command::Approval(None))));
            bus.publish(Event::Turn(Box::new(summary())));
            bus.publish(Event::Done);
        }
    });
    assert!(drive(&handle, "olá", Output::Text).is_ok());
    responder.join().ok();
}

#[test]
fn envelope_carries_the_round_exit_and_text() {
    let value = envelope(&summary());
    assert_eq!(value.get("round_exit").and_then(Value::as_u64), Some(0));
    assert_eq!(
        value.get("termination").and_then(Value::as_str),
        Some("natural")
    );
    assert_eq!(value.get("chars").and_then(Value::as_u64), Some(3));
    assert_eq!(
        value.get("context_selection").and_then(Value::as_str),
        Some("full")
    );
}
