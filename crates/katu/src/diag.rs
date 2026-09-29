//! Sink de diagnóstico para `stderr` (DF9/E19). Só existe com `--features profile`.

use katu_core::diag::{Kind, Record, Sink, Value, redact};

/// Escreve cada registo estruturado como uma linha em `stderr`.
///
/// Formato estável: `level=… kind=… event=… dur_ns=… fields=…` — agrega-se com `awk`/`jq`-like
/// sem tocar no código. Cada campo passa pela **redação** (E01-T07) antes de sair.
#[derive(Debug, Default)]
pub(crate) struct StderrSink;

impl Sink for StderrSink {
    #[allow(clippy::print_stderr, reason = "sink de diagnóstico: a borda é stderr")]
    fn record(&self, record: &Record<'_>) {
        let kind = match record.kind {
            Kind::Event => "event",
            Kind::SpanStart => "span.start",
            Kind::SpanEnd => "span.end",
        };
        let level = record.level;
        let event = record.event;
        let duration = record.duration_nanos;
        let fields: Vec<(&str, Value<'_>)> = record
            .fields
            .iter()
            .map(|(key, value)| (*key, redact::redact_value(key, *value)))
            .collect();
        eprintln!(
            "level={level:?} kind={kind} event={event} dur_ns={duration:?} fields={fields:?}"
        );
    }
}
