//! E09-T01/T07: o pedido ao provider usa o contexto orçamentado e o digest de compactação.

use std::sync::{Mutex, PoisonError};

use katu_core::context::{CompactionMode, SelectionPolicy};
use katu_core::ports::{FixedClock, Timestamp};
use katu_core::provider::{
    Flow, Provider, ProviderError, ProviderEvent, ProviderOutcome, ProviderRequest, ProviderSink,
    StopReason,
};

use super::{options, request, root};
use crate::agent::{Ports, run_turn};
use crate::ports::{StdEnv, StdFs, StdProcess};
use crate::runtime::Runtime;

/// Provider que grava o pedido recebido (sem rede) e responde com texto.
#[derive(Default)]
struct Recorder {
    requests: Mutex<Vec<ProviderRequest>>,
}

impl Provider for Recorder {
    fn id(&self) -> &'static str {
        "recorder"
    }

    fn stream(
        &self,
        request: &ProviderRequest,
        sink: &mut dyn ProviderSink,
    ) -> Result<ProviderOutcome, ProviderError> {
        self.requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(request.clone());
        if matches!(
            sink.on_event(ProviderEvent::Text("ok".to_string())),
            Flow::Break
        ) {
            return Err(ProviderError::Cancelled);
        }
        Ok(ProviderOutcome {
            usage: None,
            stop: StopReason::EndTurn,
        })
    }
}

#[test]
fn the_request_carries_the_prime_and_only_compacts_when_enabled()
-> Result<(), Box<dyn std::error::Error>> {
    let root = root("context-compact")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "g")?;
    // Prefixo grande o suficiente para sair do orçamento cru (`raw_min` default: 65 536 tokens).
    runtime.record_user(&"a".repeat(300_000))?;

    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };

    let recorder = std::sync::Arc::new(Recorder::default());
    let provider: std::sync::Arc<dyn Provider> = recorder.clone();
    let report = run_turn(&mut runtime, request(&provider, ports, "g", &options(2)))?;
    assert_eq!(report.text, "ok");
    {
        let requests = recorder
            .requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let first = requests.first().ok_or("houve um pedido")?;
        let system = first.system.as_deref().unwrap_or("");
        assert!(system.contains("katu prime"), "o prime viaja no sistema");
        assert!(
            !system.contains('\u{1e}'),
            "sem compactação não há digest no sistema"
        );
    }

    // Liga a compactação e corre de novo: o digest passa a viajar no sistema.
    // A política `suffix` isola o mecanismo de compactação do default `utility` (que pode suprimir
    // o digest por baixa divergência).
    runtime.set_selection(SelectionPolicy::Suffix);
    runtime.set_compaction(CompactionMode::Enabled);
    let recorder = std::sync::Arc::new(Recorder::default());
    let provider: std::sync::Arc<dyn Provider> = recorder.clone();
    let report = run_turn(
        &mut runtime,
        request(&provider, ports, "segundo", &options(2)),
    )?;
    assert_eq!(report.text, "ok");
    let requests = recorder
        .requests
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let second = requests.first().ok_or("houve um pedido")?;
    let system = second.system.as_deref().unwrap_or("");
    assert!(system.contains("katu prime"));
    assert!(system.contains('\u{1e}'), "o digest viaja no sistema");

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}
