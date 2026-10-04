//! Fecho do turno (L-Q3): mensagem visível e relatório final (extraído de `run.rs`).

use katu_core::diag::{Level, events};
use katu_core::provider::StopReason;

use super::Accum;
use crate::agent::{AgentError, TurnReport};
use crate::runtime::Runtime;

/// Fecha o turno e devolve o relatório (comum a todos os finais).
///
/// L-Q3: um fim **anormal** (vazio, teto ou loop) acrescenta uma mensagem do assistente visível ao
/// utilizador — o motivo continua no envelope de máquina, nunca em silêncio.
pub(super) fn finish(runtime: &mut Runtime<'_>, accum: Accum) -> Result<TurnReport, AgentError> {
    let _span = katu_core::fn_span!(Level::Debug, events::KERNEL_STOP, "run::finish");
    // L-Q2: o motivo de paragem do provider e a terminação ficam no diagnóstico (um facto, um id).
    katu_core::event!(
        Level::Debug,
        events::AGENT_STOP,
        "stop" => crate::agent::stop_label(&accum.stop),
        "termination" => accum.termination.as_str(),
        "steps" => accum.steps
    );
    let turn = runtime.turn();
    let mut text = accum.text;
    // L-Q3: fim anormal (vazio/teto/loop) tem mensagem própria; L-Q2: `length`/`content_filter`
    // sem outra terminação acrescenta o aviso correspondente. A mensagem é logada (visível).
    let note = accum.termination.message().or_else(|| match accum.stop {
        StopReason::Length => Some(
            "a resposta foi truncada pelo teto de tokens de saída; aumenta `--max-tokens` ou divide \
             o pedido"
                .to_string(),
        ),
        StopReason::ContentFilter => Some(
            "a resposta foi interrompida pelo filtro de conteúdo do provider".to_string(),
        ),
        _ => None,
    });
    if let Some(message) = note {
        runtime.record_assistant(&message)?;
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        text.push_str(&message);
    }
    runtime.record_turn_end(turn)?;
    Ok(TurnReport {
        model: accum.model,
        steps: accum.steps,
        text,
        calls: accum.calls,
        usage: accum.usage,
        cancelled: accum.cancelled,
        stop: accum.stop,
        termination: accum.termination,
    })
}
