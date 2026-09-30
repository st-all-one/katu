//! Macro [`fn_span!`](crate::fn_span): span com o rótulo estável da função (E19-T03).
//!
//! Igual a [`span!`](crate::span) mas transporta o nome `"módulo::função"` até ao fim do span, para
//! que o [`Sink`](super::Sink) atribua o **tempo atómico à função** certa (o
//! [`AggregatingSink`](super::AggregatingSink) agrupa por `(event, função)`).

/// Abre um [`Span`](crate::diag::Span) **de função**: igual a [`span!`](crate::span) mas com o
/// rótulo estável da função (`"módulo::função"`), para mapeamento de tempo atómico por função
/// (E19-T03).
///
/// ```
/// use katu_core::diag::{Level, events};
/// let _span = katu_core::fn_span!(Level::Trace, events::KATU_RUN, "exemplo::run");
/// ```
#[cfg(feature = "instrument")]
#[macro_export]
macro_rules! fn_span {
    ($level:expr, $event:expr, $function:literal $(, $key:literal => $value:expr)* $(,)?) => {
        $crate::diag::Span::start_function(
            $level,
            $event,
            $function,
            &[$(( $key, $crate::diag::Value::from($value) )),*],
        )
    };
}

/// Instrumenta uma função com nível `Trace` e o identificador genérico `katu.fn` (E19-T03).
///
/// Conveniência para cobrir funções **em massa** sem exigir imports no chamador: o rótulo
/// (`"módulo::função"`) é o único argumento e o `Sink` agrupa o tempo atómico por `(evento, função)`.
/// `no-op` quando a `feature = "instrument"` está desligada (custo literalmente zero).
///
/// ```
/// fn f() {
///     let _span = katu_core::trace_fn!("exemplo::f");
/// }
/// ```
#[macro_export]
macro_rules! trace_fn {
    ($function:literal) => {
        $crate::fn_span!(
            $crate::diag::Level::Trace,
            $crate::diag::events::KATU_FN,
            $function
        )
    };
}

/// Abre um [`Span`](crate::diag::Span) de função (no-op: instrumentação compilada fora). Os campos
/// são ignorados.
#[cfg(not(feature = "instrument"))]
#[macro_export]
macro_rules! fn_span {
    ($level:expr, $event:expr, $function:literal) => {
        $crate::diag::Span::noop($level, $event)
    };
    ($level:expr, $event:expr, $function:literal, $($rest:tt)*) => {
        $crate::diag::Span::noop($level, $event)
    };
}
