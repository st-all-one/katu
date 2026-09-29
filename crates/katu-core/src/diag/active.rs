//! Instrumentação ativa (`feature = "instrument"`), ligada em runtime.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use super::{Kind, Level, Record, Sink, Value};

static ENABLED: AtomicBool = AtomicBool::new(false);
static LEVEL: AtomicU8 = AtomicU8::new(level_to_u8(Level::Info));
static SINK: OnceLock<Arc<dyn Sink>> = OnceLock::new();

/// Codifica um nível num `u8` (sem `as`).
const fn level_to_u8(level: Level) -> u8 {
    match level {
        Level::Error => 0,
        Level::Warn => 1,
        Level::Info => 2,
        Level::Debug => 3,
        Level::Trace => 4,
    }
}

/// Descodifica um nível de um `u8`.
fn level_from_u8(value: u8) -> Level {
    match value {
        0 => Level::Error,
        1 => Level::Warn,
        2 => Level::Info,
        3 => Level::Debug,
        _ => Level::Trace,
    }
}

/// Instala o consumidor de diagnósticos (uma vez por processo) e liga a instrumentação.
pub fn install(sink: Arc<dyn Sink>) {
    if SINK.set(sink).is_ok() {
        set_enabled(true);
    }
}

/// Liga/desliga a instrumentação em runtime.
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "setter booleano único; a API é `set_enabled(bool)`"
)]
pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// `true` se a instrumentação está ligada.
#[must_use]
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Define o nível máximo recolhido.
pub fn set_level(level: Level) {
    LEVEL.store(level_to_u8(level), Ordering::Relaxed);
}

/// Nível máximo recolhido.
#[must_use]
pub fn current_level() -> Level {
    level_from_u8(LEVEL.load(Ordering::Relaxed))
}

/// Regista um evento estruturado, se a instrumentação estiver ligada e dentro do nível.
pub fn record(level: Level, event: &'static str, fields: &[(&'static str, Value<'_>)]) {
    if !enabled() || level > current_level() {
        return;
    }
    emit(&Record {
        level,
        event,
        kind: Kind::Event,
        duration_nanos: None,
        fields,
    });
}

/// Estado ativo de um [`Span`](super::Span).
pub(super) struct Active {
    level: Level,
    event: &'static str,
    start: Instant,
}

#[allow(
    clippy::disallowed_methods,
    reason = "diagnóstico: relógio monotónico é intencional (nunca entra no plano de dados)"
)]
pub(super) fn begin(
    level: Level,
    event: &'static str,
    fields: &[(&'static str, Value<'_>)],
) -> Option<Active> {
    if !enabled() || level > current_level() {
        return None;
    }
    emit(&Record {
        level,
        event,
        kind: Kind::SpanStart,
        duration_nanos: None,
        fields,
    });
    Some(Active {
        level,
        event,
        start: Instant::now(),
    })
}

impl Active {
    /// Fecha o span, registando a duração.
    pub(super) fn finish(self) {
        let nanos = self.start.elapsed().as_nanos();
        emit(&Record {
            level: self.level,
            event: self.event,
            kind: Kind::SpanEnd,
            duration_nanos: Some(nanos),
            fields: &[],
        });
    }
}

/// Entrega um registo ao sink instalado, se existir.
fn emit(record: &Record<'_>) {
    if let Some(sink) = SINK.get() {
        sink.record(record);
    }
}
