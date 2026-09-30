//! Dialeto `OpenAI` **Responses API** (E12-T06): `POST /responses`, SSE por eventos.
//!
//! O stream identifica cada evento pelo campo `type` (`response.output_text.delta`,
//! `response.output_item.added`, `response.function_call_arguments.delta`, `response.completed`),
//! pelo que o parser SSE só precisa dos payloads `data:`. As tool calls só são emitidas
//! **completas** (em `response.output_item.done` ou no fecho).

use katu_core::diag::{Level, events};
use katu_core::provider::{ProviderError, ProviderOutcome, ProviderSink};

use crate::engine::Call;
use crate::transport::{HttpRequest, Transport};
use crate::wire;

mod chunk;
mod decode;
mod encode;

use decode::ResponsesDecoder;

/// Executa um turno no dialeto `responses`, emitindo deltas no `sink`.
///
/// # Errors
/// [`ProviderError`] em falha de transporte, HTTP não-2xx, decodificação ou cancelamento.
pub(crate) fn stream<T: Transport>(
    transport: &T,
    call: &Call<'_>,
    sink: &mut dyn ProviderSink,
) -> Result<ProviderOutcome, ProviderError> {
    let _span = katu_core::fn_span!(Level::Debug, events::PROVIDER_REQUEST, "responses::stream");
    let body = encode::encode_request(call.request, call.options)?;
    let http = HttpRequest::post(
        call.endpoint.url.clone(),
        body,
        call.endpoint.headers.clone(),
    );
    wire::stream(transport, &http, call.retry, ResponsesDecoder::new, sink)
}

#[cfg(test)]
mod tests;
