//! Guard de loop do turno (Q-12/F7): observa as chamadas e corta o ciclo **antes** do teto.
//!
//! Uma chamada **exclusiva** (escreve, move, executa) marca o passo como **progresso**: o detector
//! reinicia. É o que distingue um ciclo patológico de um *polling* legítimo. O corte nunca é
//! silencioso: emite `agent.loop` e devolve [`Termination::Loop`].

use katu_core::diag::{Level, events};
use katu_core::kernel::{Call, CallId, Fingerprint, Guard};
use katu_tools::schema::concurrency_of;
use serde_json::Value;

use crate::agent::Termination;

/// Observa o passo no guard e **corta** o turno em ciclo (Q-12/F7).
///
/// As chamadas são observadas **antes** de correr, pelo que o ciclo não chega a gastar orçamento de
/// tools.
pub(super) fn cut_if_looping(
    guard: &mut Guard,
    calls: &[(CallId, String, Value)],
) -> Option<Termination> {
    let _span = katu_core::trace_fn!("agent::turn::looping::cut_if_looping");

    let alarm = guard.observe(&fingerprints(calls))?;
    katu_core::event!(
        Level::Warn,
        events::AGENT_LOOP,
        "step" => alarm.step,
        "kind" => alarm.kind.as_str(),
        "repeated" => alarm.repeated,
        "novelty_milli" => alarm.novelty_milli,
        "cusum_milli" => alarm.cusum_milli,
        "e_value_log_milli" => alarm.e_value_log_milli
    );
    Some(Termination::Loop {
        step: alarm.step,
        reason: alarm.reason(),
    })
}

/// Traduz as chamadas do passo em impressões para o guard (Q-12).
fn fingerprints(calls: &[(CallId, String, Value)]) -> Vec<Call> {
    let _span = katu_core::trace_fn!("agent::turn::looping::fingerprints");

    calls
        .iter()
        .map(|(_, name, arguments)| {
            let print = Fingerprint::of(name, arguments);
            if concurrency_of(name).is_shared() {
                Call::shared(print)
            } else {
                Call::exclusive(print)
            }
        })
        .collect()
}
