//! Sink de diagnóstico para `stderr` (DF9/E19). Só existe com `--features profile`.

use katu_core::diag::{Kind, Record, Sink, Value, redact};

/// Escreve cada registo estruturado como uma linha em `stderr`.
///
/// Formato estável: `level=… kind=… [function=…] event=… dur_ns=… fields=…` — agrega-se com
/// `awk`/`jq`-like sem tocar no código. O campo `function` só aparece quando o span traz o rótulo
/// (`fn_span!`/`trace_fn!`, Q-09), pelo que os eventos pontuais mantêm a linha antiga. Cada campo
/// passa pela **redação** (E01-T07) antes de sair.
#[derive(Debug, Default)]
pub(crate) struct StderrSink;

impl Sink for StderrSink {
    #[allow(clippy::print_stderr, reason = "sink de diagnóstico: a borda é stderr")]
    fn record(&self, record: &Record<'_>) {
        eprintln!("{}", format_record(record));
    }
}

/// Formata um registo na linha estável do sink. Pura e testável (Q-09).
///
/// A atribuição **por função** chega pelo rótulo [`Record::function`]; só nesse caso se acrescenta
/// `function=…`, para não mudar as linhas já consumidas por *scripts*.
fn format_record(record: &Record<'_>) -> String {
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
    match record.function {
        Some(function) => format!(
            "level={level:?} kind={kind} function={function} event={event} dur_ns={duration:?} \
             fields={fields:?}"
        ),
        None => format!(
            "level={level:?} kind={kind} event={event} dur_ns={duration:?} fields={fields:?}"
        ),
    }
}

#[cfg(test)]
mod tests {
    use katu_core::diag::{Kind, Level, Record, Value};

    use super::format_record;

    /// Registo de span terminado, com ou sem rótulo de função.
    fn record<'a>(
        function: Option<&'static str>,
        fields: &'a [(&'static str, Value<'a>)],
    ) -> Record<'a> {
        Record {
            level: Level::Trace,
            event: "katu.fn",
            function,
            kind: Kind::SpanEnd,
            duration_nanos: Some(7),
            fields,
        }
    }

    #[test]
    fn a_function_span_exposes_its_label() {
        let line = format_record(&record(Some("kernel::log::append"), &[]));
        assert_eq!(
            line,
            "level=Trace kind=span.end function=kernel::log::append event=katu.fn dur_ns=Some(7) \
             fields=[]"
        );
    }

    #[test]
    fn an_event_without_a_label_keeps_the_stable_format() {
        let line = format_record(&record(None, &[]));
        assert_eq!(
            line,
            "level=Trace kind=span.end event=katu.fn dur_ns=Some(7) fields=[]"
        );
        assert!(!line.contains("function="), "{line}");
    }

    #[test]
    fn fields_are_redacted_before_printing() {
        let line = format_record(&record(None, &[("token", Value::Str("abc"))]));
        assert!(!line.contains("abc"), "{line}");
        assert!(line.contains("[redacted]"), "{line}");
    }
}
