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
    let system = system_for(
        options,
        &context,
        runtime.instructions(),
        &runtime.skills_catalog(),
    );
    Ok(ProviderRequest {
        model: options.model.clone(),
        system,
        messages: context.messages,
        tools: tools.to_vec(),
        max_tokens: Some(options.max_tokens),
        temperature: Some(options.temperature),
    })
}

/// Funde o `AGENTS.md` (fonte de verdade máxima, E20-T13), a instrução de sistema, o **prime**
/// (DF12), o **digest** (E09-T07) e o catálogo de skills (E20-T13).
fn system_for(
    options: &TurnOptions,
    context: &Context,
    instructions: Option<&str>,
    skills: &str,
) -> Option<String> {
    let mut system = String::new();
    if let Some(instructions) = instructions {
        system.push_str("# AGENTS.md (fonte de verdade do projeto)\n");
        system.push_str(instructions);
    }
    for part in [
        options.system.as_deref(),
        Some(context.prime.as_str()),
        context.summary.as_deref(),
        (!skills.is_empty()).then_some(skills),
    ]
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
