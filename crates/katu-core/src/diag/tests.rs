//! Testes do diagnóstico (E19): caminho ativo e custo-zero por omissão.

#[cfg(feature = "instrument")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "instrument")]
use crate::containment::{ContainmentStatus, announce};
#[cfg(feature = "instrument")]
use crate::diag::{
    INSTRUMENT_LOCK, Kind, Level, Record, Sink, events, install, set_enabled, set_level,
};
#[cfg(not(feature = "instrument"))]
use crate::diag::{Level, enabled, events};

/// Sink de teste que guarda `(evento, tipo)` por ordem.
#[cfg(feature = "instrument")]
#[derive(Default)]
struct Recorder {
    records: Mutex<Vec<(&'static str, Kind)>>,
}

#[cfg(feature = "instrument")]
impl Sink for Recorder {
    fn record(&self, record: &Record<'_>) {
        self.records
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((record.event, record.kind));
    }
}

#[cfg(feature = "instrument")]
#[test]
fn records_span_and_event() {
    let _guard = INSTRUMENT_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let recorder = Arc::new(Recorder::default());
    install(recorder.clone());
    set_level(Level::Trace);
    set_enabled(true);
    {
        let _span = crate::span!(Level::Info, events::KATU_RUN, "n" => 1_u64);
        crate::event!(Level::Debug, events::TOOL_OK, "ok" => true);
        announce(ContainmentStatus::mvp());
    }
    let records = recorder
        .records
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert_eq!(records.len(), 4);
    assert_eq!(records.first().map(|r| r.1), Some(Kind::SpanStart));
    assert_eq!(records.get(1).map(|r| r.1), Some(Kind::Event));
    assert_eq!(records.get(2).map(|r| r.0), Some(events::CONTAIN_MODE));
    assert_eq!(records.get(3).map(|r| r.1), Some(Kind::SpanEnd));
    set_enabled(false);
}

#[cfg(not(feature = "instrument"))]
#[test]
fn instrumentation_is_off_by_default() {
    use std::hint::black_box;

    assert!(!black_box(enabled()));
    let _span = crate::span!(Level::Info, events::KATU_RUN, "k" => black_box(1_u64));
}
