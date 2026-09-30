//! Dialeto **Google Gemini** (`models/<id>:streamGenerateContent`).
//!
//! Consome o stream SSE incremental: `parts` de texto/raciocínio (`thought: true`) e
//! `functionCall` (completo, sem acumulação). A contabilização vem em `usageMetadata`. O endpoint
//! e a autenticação (`x-goog-api-key`) são construídos pelo despacho comum ([`crate::engine`]).

use katu_core::provider::{ProviderError, ProviderOutcome, ProviderSink};

use super::engine::Call;
use super::transport::{HttpRequest, Transport};
use super::wire;

mod chunk;
mod decode;
mod encode;

pub(crate) use decode::GoogleDecoder;
pub(crate) use encode::encode_request;

/// Executa um turno no dialeto Google, emitindo deltas no `sink`.
///
/// # Errors
/// [`ProviderError`] em falha de transporte, HTTP não-2xx, decodificação ou cancelamento.
pub(crate) fn stream<T: Transport>(
    transport: &T,
    call: &Call<'_>,
    sink: &mut dyn ProviderSink,
) -> Result<ProviderOutcome, ProviderError> {
    let body = encode_request(call.request, call.options)?;
    let http = HttpRequest::post(
        call.endpoint.url.clone(),
        body,
        call.endpoint.headers.clone(),
    );
    wire::stream(transport, &http, call.retry, GoogleDecoder::new, sink)
}

#[cfg(test)]
mod tests;
