//! L-P1/L-P2: o stream do provider corre fora da thread do turno; cancelar e o teto de inatividade
//! não dependem de chegar um delta.
//!
//! Prova-se que: (a) um stream **parado** é cancelado no *tick* (≤ `STREAM_POLL_MS`), sem deltas;
//! (b) sem cancelamento, um stream parado expira no teto de inatividade (L-P2) em vez de pendurar.

use std::sync::Arc;
use std::time::{Duration, Instant};

use katu_core::ports::{FixedClock, Timestamp};
use katu_core::provider::{
    Provider, ProviderError, ProviderOutcome, ProviderRequest, ProviderSink, StopReason,
};

use super::{Ports, options, options_idle, request, root};
use crate::agent::{Activity, ActivitySink, AgentError, run_turn_with};
use crate::ports::{StdEnv, StdFs, StdProcess};
use crate::runtime::Runtime;

/// Provider que **nunca** emite nada: fica bloqueado até o teste acabar.
struct SilentProvider {
    millis: u64,
}

impl Provider for SilentProvider {
    fn id(&self) -> &'static str {
        "silent"
    }

    fn stream(
        &self,
        _request: &ProviderRequest,
        _sink: &mut dyn ProviderSink,
    ) -> Result<ProviderOutcome, ProviderError> {
        std::thread::sleep(Duration::from_millis(self.millis));
        Ok(ProviderOutcome {
            usage: None,
            stop: StopReason::EndTurn,
        })
    }
}

/// Observador que cancela no primeiro `tick` (o loop chama-o quando o stream cala).
#[derive(Default)]
struct TickCanceller {
    cancel: bool,
}

impl ActivitySink for TickCanceller {
    fn activity(&mut self, _activity: Activity<'_>) {}

    fn tick(&mut self) {
        self.cancel = true;
    }

    fn cancelled(&self) -> bool {
        self.cancel
    }
}

/// Observador inerte (só conta o fim).
#[derive(Default)]
struct Silent;

impl ActivitySink for Silent {
    fn activity(&mut self, _activity: Activity<'_>) {}
}

#[test]
#[allow(
    clippy::disallowed_methods,
    reason = "o teste mede o tempo real do cancelamento (L-P1); o relógio do kernel é fixo"
)]
fn cancel_without_deltas_is_seen_on_the_tick() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("cancel-tick")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "g")?;
    let provider: Arc<dyn Provider> = Arc::new(SilentProvider { millis: 2_000 });
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };
    let mut sink = TickCanceller::default();

    let started = Instant::now();
    let report = run_turn_with(
        &mut runtime,
        request(&provider, ports, "g", &options(4)),
        &mut sink,
    )?;
    let elapsed = started.elapsed();

    assert!(report.cancelled, "o turno é cancelado sem deltas");
    assert_eq!(report.calls, 0, "nenhuma tool corre");
    assert!(
        elapsed < Duration::from_millis(1_500),
        "o cancelamento não espera o stream: {elapsed:?}"
    );
    runtime.session().verify()?;

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

#[test]
#[allow(
    clippy::disallowed_methods,
    reason = "o teste mede o tempo real do *stall* (L-P2); o relógio do kernel é fixo"
)]
fn a_stalled_stream_times_out_instead_of_hanging() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("stall")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "g")?;
    let provider: Arc<dyn Provider> = Arc::new(SilentProvider { millis: 2_000 });
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };
    let mut sink = Silent;

    let started = Instant::now();
    let result = run_turn_with(
        &mut runtime,
        request(&provider, ports, "g", &options_idle(4, 120)),
        &mut sink,
    );
    let elapsed = started.elapsed();

    assert!(
        matches!(
            result,
            Err(AgentError::Provider(ProviderError::Timeout { .. }))
        ),
        "um stream parado expira no teto de inatividade"
    );
    assert!(
        elapsed < Duration::from_millis(1_500),
        "não pendura até ao fim do provider: {elapsed:?}"
    );
    // O log fecha limpo mesmo no *stall* (L-Q1/I1).
    runtime.session().verify()?;
    assert!(!runtime.session().state().turn_open);

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}
