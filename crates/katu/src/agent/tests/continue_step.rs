//! Testes do passo extra pedido pela superfície (`S1/PI_GAINS`): `Command::Continue` one-shot.

use std::sync::Arc;

use super::{Ports, options, request, root};
use crate::agent::{Activity, ActivitySink, Termination, run_turn_with};
use crate::ports::{StdEnv, StdFs, StdProcess};
use crate::runtime::Runtime;
use katu_core::ports::{FixedClock, Timestamp};
use katu_core::provider::Provider;
use katu_providers::{FakeProvider, Turn};

/// Sink de teste que pede `n` passos extra (one-shot cada).
struct ContinueOnce {
    remaining: u32,
}

impl ActivitySink for ContinueOnce {
    fn activity(&mut self, _activity: Activity<'_>) {}

    fn continue_once(&mut self) -> bool {
        if self.remaining > 0 {
            self.remaining = self.remaining.saturating_sub(1);
            true
        } else {
            false
        }
    }
}

#[test]
fn a_continue_request_adds_one_step() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("continue")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "continua")?;
    let provider: Arc<dyn Provider> = Arc::new(FakeProvider::new(
        "fake",
        vec![Turn::text("primeiro"), Turn::text("segundo")],
    ));
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };
    let options = options(4);
    let mut sink = ContinueOnce { remaining: 1 };

    let report = run_turn_with(
        &mut runtime,
        request(&provider, ports, "continua", &options),
        &mut sink,
    )?;
    assert_eq!(report.steps, 2, "um pedido = um passo extra");
    assert_eq!(report.termination, Termination::Natural);
    assert!(
        report.text.contains("primeiro") && report.text.contains("segundo"),
        "{}",
        report.text
    );
    runtime.session().verify()?;

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

#[test]
fn without_a_request_the_turn_closes_after_one_step() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("continue-sem-pedido")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "continua")?;
    let provider: Arc<dyn Provider> = Arc::new(FakeProvider::new(
        "fake",
        vec![Turn::text("primeiro"), Turn::text("segundo")],
    ));
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };
    let options = options(4);
    let mut sink = ContinueOnce { remaining: 0 };

    let report = run_turn_with(
        &mut runtime,
        request(&provider, ports, "continua", &options),
        &mut sink,
    )?;
    assert_eq!(report.steps, 1);
    assert_eq!(report.termination, Termination::Natural);
    assert!(report.text.contains("primeiro"));
    assert!(!report.text.contains("segundo"), "{}", report.text);
    runtime.session().verify()?;

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

/// O passo extra não ultrapassa o teto de passos do turno.
#[test]
fn a_continue_request_never_exceeds_max_steps() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("continue-teto")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "continua")?;
    let provider: Arc<dyn Provider> = Arc::new(FakeProvider::new(
        "fake",
        vec![Turn::text("primeiro"), Turn::text("segundo")],
    ));
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };
    let options = options(1);
    let mut sink = ContinueOnce { remaining: 3 };

    let report = run_turn_with(
        &mut runtime,
        request(&provider, ports, "continua", &options),
        &mut sink,
    )?;
    assert_eq!(report.steps, 1, "o teto de 1 passo trava o continue");
    assert!(!report.text.contains("segundo"), "{}", report.text);
    runtime.session().verify()?;

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}
