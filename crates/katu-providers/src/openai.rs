//! Dialeto **`OpenAI` `chat/completions`** (E12-T06): o caminho quente do built-in.
//!
//! Cobre `zen/v1/chat/completions`, `zen/go/v1/chat/completions` e o `llama-server`
//! (`/v1/chat/completions`), que partilham o mesmo dialeto. O pedido pede `stream: true` com
//! `include_usage`; a resposta é consumida **incrementalmente** (ver [`decode`]) e as tool calls
//! só são emitidas **completas** (acumuladas por índice).
//!
//! O transporte é injetado ([`Transport`]): os testes servem bytes canónicos sem rede. O retry e
//! a captura de erro vivem no [`crate::wire`] comum aos dialetos.

use katu_core::provider::{ProviderError, ProviderOutcome, ProviderSink};

use super::engine::Call;
use super::transport::{HttpRequest, Transport};
use super::wire;

mod chunk;
mod decode;
mod encode;

pub(crate) use decode::ChatDecoder;
pub(crate) use encode::{
    EncodeOptions, encode_request, model_tool_name, thinking_effort, tool_arguments,
};

/// Destino de um dialeto: URL + cabeçalhos fixos.
#[derive(Debug, Clone)]
pub struct Endpoint {
    /// URL absoluta do endpoint.
    pub url: String,
    /// Cabeçalhos na ordem de envio.
    pub headers: Vec<(String, String)>,
}

/// Executa um turno no dialeto chat/completions, emitindo deltas no `sink`.
///
/// # Errors
/// [`ProviderError`] em falha de transporte, HTTP não-2xx, decodificação ou cancelamento.
pub(crate) fn stream_chat<T: Transport>(
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
    wire::stream(transport, &http, call.retry, ChatDecoder::new, sink)
}

#[cfg(test)]
mod tests;
