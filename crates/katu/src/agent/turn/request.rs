//! Montagem do pedido ao provider (E09-T01/T07): contexto efetivo com orçamento.

use katu_core::context::Context;
use katu_core::provider::{ProviderRequest, ToolDef};

use crate::agent::{AgentError, TurnOptions};
use crate::runtime::Runtime;

/// Monta o pedido ao provider para um passo (histórico = projeção do log).
pub(super) fn build_request(
    runtime: &Runtime<'_>,
    options: &TurnOptions,
    tools: &[ToolDef],
) -> Result<ProviderRequest, AgentError> {
    let context = runtime.context()?;
    Ok(ProviderRequest {
        model: options.model.clone(),
        system: system_for(options, &context),
        messages: context.messages,
        tools: tools.to_vec(),
        max_tokens: Some(options.max_tokens),
        temperature: Some(options.temperature),
    })
}

/// Funde a instrução de sistema com o **prime** (DF12) e o **digest** da compactação (E09-T07).
/// O prime é determinístico e versionado (`PRIME_VERSION`); o digest só aparece com compactação.
fn system_for(options: &TurnOptions, context: &Context) -> Option<String> {
    let mut system = options.system.clone().unwrap_or_default();
    for part in [Some(context.prime.as_str()), context.summary.as_deref()]
        .into_iter()
        .flatten()
    {
        if !system.is_empty() {
            system.push_str("\n\n");
        }
        system.push_str(part);
    }
    (!system.is_empty()).then_some(system)
}
