//! I/O do stream do provider fora da thread do turno (L-P1) e teto de inatividade (L-P2).
//!
//! O corpo do provider é bloqueante. Correndo numa thread própria e drenando o canal na thread do
//! turno, a UI é sondada a cada `STREAM_POLL_MS` **mesmo sem deltas**, pelo que `Esc`/`Ctrl-C` são
//! vistos de imediato e um stream parado expira em `idle_ms` em vez de pendurar.

use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use katu_core::diag::{Level, events};
use katu_core::provider::{
    Flow, Provider, ProviderError, ProviderEvent, ProviderOutcome, ProviderRequest, ProviderSink,
};

use super::sink::TurnSink;

/// Granularidade da sondagem do stream: a UI processa input a cada `STREAM_POLL_MS` mesmo sem
/// deltas, pelo que `T_cancel` não depende de chegar um chunk (L-P1).
pub(super) const STREAM_POLL_MS: u64 = 50;

/// Capacidade do canal do stream: *bounded* (backpressure). O painel é efémero; nada disto entra
/// no log, pelo que um evento descartado nunca corrompe o contexto do modelo.
const STREAM_CHANNEL: usize = 64;

/// Sink do I/O do provider: envia os eventos à thread do turno; `Break` quando o recetor fechou
/// (o turno já não precisa de mais eventos — cancelou ou fechou).
struct ChannelSink {
    tx: mpsc::SyncSender<ProviderEvent>,
}

impl ProviderSink for ChannelSink {
    fn on_event(&mut self, event: ProviderEvent) -> Flow {
        let _span = katu_core::trace_fn!("agent::turn::stream::ChannelSink::on_event");

        if self.tx.send(event).is_ok() {
            Flow::Continue
        } else {
            Flow::Break
        }
    }
}

/// Consome o stream do provider numa thread de I/O, drenando o canal na thread do turno.
///
/// L-P1: o corpo do provider (bloqueante) corre **fora** da thread da UI; a thread do turno drena
/// o canal e sonda o input a cada `STREAM_POLL_MS`. L-P2: sem progresso durante `idle_ms`, o stream
/// é um *stall* recuperável — devolve [`ProviderError::Timeout`] e o turno fecha (a thread de I/O
/// pendurada termina sozinha no teto do transporte).
#[allow(
    clippy::disallowed_methods,
    reason = "watchdog de inatividade em tempo real; não entra no log nem no estado (L-P2)"
)]
pub(super) fn drain_stream(
    provider: &Arc<dyn Provider>,
    request: &ProviderRequest,
    sink: &mut TurnSink<'_>,
    idle_ms: u64,
) -> Result<ProviderOutcome, ProviderError> {
    let _span = katu_core::trace_fn!("agent::turn::stream::drain_stream");

    let (tx, rx) = mpsc::sync_channel::<ProviderEvent>(STREAM_CHANNEL);
    let (done_tx, done_rx) = mpsc::channel::<Result<ProviderOutcome, ProviderError>>();
    let owned_request = request.clone();
    let owned_provider = Arc::clone(provider);
    // Sem `join`: um stream pendurado **não** pode prender a UI. A thread de I/O termina sozinha
    // quando o canal fecha (o turno saiu) ou quando o transporte expira.
    drop(
        std::thread::Builder::new()
            .name("katu-provider".to_string())
            .spawn(move || {
                let mut io_sink = ChannelSink { tx };
                let outcome = owned_provider.stream(&owned_request, &mut io_sink);
                drop(done_tx.send(outcome));
            }),
    );
    let mut last_progress = Instant::now();
    loop {
        if sink.activity.cancelled() {
            return Err(ProviderError::Cancelled);
        }
        match rx.recv_timeout(Duration::from_millis(STREAM_POLL_MS)) {
            Ok(event) => {
                last_progress = Instant::now();
                if matches!(sink.on_event(event), Flow::Break) {
                    return Err(ProviderError::Cancelled);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                sink.activity.tick();
                if sink.activity.cancelled() {
                    return Err(ProviderError::Cancelled);
                }
                if idle_ms > 0 && last_progress.elapsed() >= Duration::from_millis(idle_ms) {
                    katu_core::event!(Level::Warn, events::AGENT_STALL, "idle_ms" => idle_ms);
                    return Err(ProviderError::Timeout { millis: idle_ms });
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return done_rx.recv().unwrap_or_else(|_| {
                    Err(ProviderError::Transport(
                        "a thread de I/O do provider terminou sem resultado".to_string(),
                    ))
                });
            }
        }
    }
}
