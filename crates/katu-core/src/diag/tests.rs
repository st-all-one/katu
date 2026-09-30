//! Testes do diagnóstico (E19): caminho ativo e custo-zero por omissão.

#[cfg(feature = "instrument")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "instrument")]
use crate::containment::{ContainmentStatus, announce};
#[cfg(feature = "instrument")]
use crate::diag::{
    INSTRUMENT_LOCK, Kind, Level, Record, Sink, events, filter_allows, install, set_enabled,
    set_filter, set_level,
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
    // O sink é global e os testes correm em paralelo: outras threads podem emitir (sobretudo
    // `katu.fn`, agora que quase toda a função abre um span). Exigimos que a **nossa** sequência
    // apareça **em ordem** (subsequência), tolerando ruído intercalado.
    let got: Vec<(&str, Kind)> = records
        .iter()
        .map(|(event, kind)| (*event, *kind))
        .collect();
    // `announce` abre um span de função (`trace_fn!` → `katu.fn`) em torno de `contain.mode`.
    let expected = [
        (events::KATU_RUN, Kind::SpanStart),
        (events::TOOL_OK, Kind::Event),
        (events::KATU_FN, Kind::SpanStart),
        (events::CONTAIN_MODE, Kind::Event),
        (events::KATU_RUN, Kind::SpanEnd),
    ];
    assert!(is_subsequence(&expected, &got), "ordem inesperada: {got:?}");
    set_enabled(false);
}

/// `true` se `needle` ocorre em `haystack` pela mesma ordem (sem exigir contiguidade).
#[cfg(feature = "instrument")]
fn is_subsequence(needle: &[(&str, Kind)], haystack: &[(&str, Kind)]) -> bool {
    let mut it = haystack.iter();
    needle
        .iter()
        .all(|item| it.any(|candidate| candidate == item))
}

#[cfg(not(feature = "instrument"))]
#[test]
fn instrumentation_is_off_by_default() {
    use std::hint::black_box;

    assert!(!black_box(enabled()));
    let _span = crate::span!(Level::Info, events::KATU_RUN, "k" => black_box(1_u64));
}

#[cfg(feature = "instrument")]
#[test]
fn filter_limits_events_by_subsystem() {
    let _guard = INSTRUMENT_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_filter("provider.");
    assert!(filter_allows("provider.ttft"));
    assert!(!filter_allows("policy.deny"));
    set_filter("");
    assert!(filter_allows("policy.deny"));
}
