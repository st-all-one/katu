//! Dialeto **Anthropic Messages** (E12-T06): `POST /messages`, SSE com `event:`/`data:`.
//!
//! Os blocos chegam por índice (`content_block_start`/`content_block_delta`/`content_block_stop`);
//! as tool calls só são emitidas **completas** (no `content_block_stop` ou no fecho). A
//! contabilização vem repartida (`message_start` para a entrada, `message_delta` para a saída).

use katu_core::provider::{ProviderError, ProviderOutcome, ProviderSink};

use crate::engine::Call;
use crate::transport::{HttpRequest, Transport};
use crate::wire;

mod chunk;
mod decode;
mod encode;

use decode::MessagesDecoder;

/// Executa um turno no dialeto Anthropic Messages, emitindo deltas no `sink`.
///
/// # Errors
/// [`ProviderError`] em falha de transporte, HTTP não-2xx, decodificação ou cancelamento.
pub(crate) fn stream<T: Transport>(
    transport: &T,
    call: &Call<'_>,
    sink: &mut dyn ProviderSink,
) -> Result<ProviderOutcome, ProviderError> {
    let body = encode::encode_request(call.request, call.options)?;
    let http = HttpRequest::post(
        call.endpoint.url.clone(),
        body,
        call.endpoint.headers.clone(),
    );
    wire::stream(transport, &http, call.retry, MessagesDecoder::new, sink)
}

#[cfg(test)]
mod tests;
