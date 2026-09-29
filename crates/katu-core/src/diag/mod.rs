//! Diagnóstico transversal: **log estruturado** + **métrica de tempo** (DF9/E19).
//!
//! Regras do projeto:
//!
//! - **Logs sempre estruturados**: um registo é um **identificador estável** (do catálogo
//!   [`events`]) + **campos tipados**; nunca texto livre interpolado.
//! - **Custo zero por defeito**: sem `feature = "instrument"`, [`span!`](crate::span) e
//!   [`event!`](crate::event) são *no-op* e o caminho ativo **não existe** no binário.
//! - **On-demand**: com a feature, liga-se em runtime (`KATU_INSTRUMENT=1`); desligado, é uma
//!   leitura atómica *relaxed* + ramo.
//!
//! A instrumentação é **diagnóstico**: nunca entra no log de sessão nem no contexto do modelo.
//!
//! ```
//! use katu_core::diag::{Level, events};
//!
//! fn exemplo(n: usize) {
//!     let _span = katu_core::span!(Level::Info, events::KATU_RUN, "n" => n);
//!     katu_core::event!(Level::Debug, events::POLICY_EVALUATE, "ok" => true);
//! }
//! ```

pub mod events;

/// Sink agregador de contagens e durações por evento (E19-T02).
#[cfg(feature = "instrument")]
pub mod aggregate;

/// Nível de diagnóstico, do mais grave (`Error`) ao mais verboso (`Trace`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Erro.
    Error,
    /// Aviso.
    Warn,
    /// Informação.
    Info,
    /// Detalhe de depuração.
    Debug,
    /// Rastreio fino.
    Trace,
}

/// Valor de um campo estruturado (sem alocação).
#[derive(Debug, Clone, Copy)]
pub enum Value<'a> {
    /// Texto.
    Str(&'a str),
    /// Inteiro com sinal.
    Int(i64),
    /// Inteiro sem sinal.
    Uint(u64),
    /// Booleano.
    Bool(bool),
}

impl<'a> From<&'a str> for Value<'a> {
    fn from(value: &'a str) -> Self {
        Self::Str(value)
    }
}

impl From<i32> for Value<'_> {
    fn from(value: i32) -> Self {
        Self::Int(i64::from(value))
    }
}

impl From<i64> for Value<'_> {
    fn from(value: i64) -> Self {
        Self::Int(value)
    }
}

impl From<u32> for Value<'_> {
    fn from(value: u32) -> Self {
        Self::Uint(u64::from(value))
    }
}

impl From<u64> for Value<'_> {
    fn from(value: u64) -> Self {
        Self::Uint(value)
    }
}

impl From<usize> for Value<'_> {
    fn from(value: usize) -> Self {
        Self::Uint(u64::try_from(value).unwrap_or(u64::MAX))
    }
}

impl From<bool> for Value<'_> {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

/// Tipo de registo de diagnóstico.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Evento pontual (log).
    Event,
    /// Início de um span.
    SpanStart,
    /// Fim de um span (tem `duração`).
    SpanEnd,
}

/// Registo estruturado entregue ao [`Sink`].
#[derive(Debug)]
pub struct Record<'a> {
    /// Nível do registo.
    pub level: Level,
    /// Identificador estável do evento (do catálogo [`events`]).
    pub event: &'static str,
    /// Tipo do registo.
    pub kind: Kind,
    /// Duração em nanossegundos (apenas em [`Kind::SpanEnd`]).
    pub duration_nanos: Option<u128>,
    /// Campos tipados; **todo** o dado variável vai aqui.
    pub fields: &'a [(&'static str, Value<'a>)],
}

/// Consumidor de diagnósticos (porta).
pub trait Sink: Send + Sync {
    /// Entrega um registo estruturado.
    fn record(&self, record: &Record<'_>);
}

/// Serializa testes que mexem no sink global (evita corridas entre testes).
#[cfg(all(test, feature = "instrument"))]
pub(crate) static INSTRUMENT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Guarda RAII de um span instrumentado.
///
/// Larga o span ao sair do escopo, registando a duração. Quando a instrumentação está compilada
/// fora, é um tipo zero-sized e nada acontece.
pub struct Span {
    #[cfg(feature = "instrument")]
    inner: Option<active::Active>,
}

impl Span {
    /// Abre um span com nível, identificador e campos.
    #[cfg(feature = "instrument")]
    #[must_use]
    pub fn start(level: Level, event: &'static str, fields: &[(&'static str, Value<'_>)]) -> Self {
        Self {
            inner: active::begin(level, event, fields),
        }
    }

    /// Span no-op; marca `level`/`event` como usados (instrumentação compilada fora).
    #[cfg(not(feature = "instrument"))]
    #[must_use]
    pub const fn noop(_level: Level, _event: &'static str) -> Self {
        Self {}
    }
}

#[cfg(feature = "instrument")]
impl Drop for Span {
    fn drop(&mut self) {
        if let Some(active) = self.inner.take() {
            active.finish();
        }
    }
}

#[cfg(feature = "instrument")]
mod active;

#[cfg(not(feature = "instrument"))]
mod disabled;

#[cfg(feature = "instrument")]
pub use active::{current_level, enabled, install, record, set_enabled, set_level};

#[cfg(feature = "instrument")]
pub use aggregate::{AggregatingSink, EventSummary};

#[cfg(not(feature = "instrument"))]
pub use disabled::{enabled, noop_event};

/// Abre um [`Span`] estruturado. `$event` vem do catálogo [`events`].
///
/// ```
/// use katu_core::diag::{Level, events};
/// let _span = katu_core::span!(Level::Info, events::KATU_RUN, "n" => 1_u64);
/// ```
#[cfg(feature = "instrument")]
#[macro_export]
macro_rules! span {
    ($level:expr, $event:expr $(, $key:literal => $value:expr)* $(,)?) => {
        $crate::diag::Span::start(
            $level,
            $event,
            &[$(( $key, $crate::diag::Value::from($value) )),*],
        )
    };
}

/// Abre um [`Span`] (no-op: instrumentação compilada fora). Os campos são ignorados.
#[cfg(not(feature = "instrument"))]
#[macro_export]
macro_rules! span {
    ($level:expr, $event:expr) => {
        $crate::diag::Span::noop($level, $event)
    };
    ($level:expr, $event:expr, $($rest:tt)*) => {
        $crate::diag::Span::noop($level, $event)
    };
}

/// Regista um **evento** estruturado (log pontual). `$event` vem do catálogo [`events`].
///
/// ```
/// use katu_core::diag::{Level, events};
/// katu_core::event!(Level::Debug, events::TOOL_OK, "ok" => true);
/// ```
#[cfg(feature = "instrument")]
#[macro_export]
macro_rules! event {
    ($level:expr, $event:expr $(, $key:literal => $value:expr)* $(,)?) => {
        $crate::diag::record(
            $level,
            $event,
            &[$(( $key, $crate::diag::Value::from($value) )),*],
        )
    };
}

/// Regista um evento (no-op: instrumentação compilada fora).
#[cfg(not(feature = "instrument"))]
#[macro_export]
macro_rules! event {
    ($level:expr, $event:expr) => {
        $crate::diag::noop_event($level, $event)
    };
    ($level:expr, $event:expr, $($rest:tt)*) => {
        $crate::diag::noop_event($level, $event)
    };
}

#[cfg(test)]
mod tests;
