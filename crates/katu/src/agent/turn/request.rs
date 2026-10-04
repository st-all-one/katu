//! Montagem do pedido ao provider (E09-T01/T07): contexto efetivo com orçamento.

use katu_core::context::Context;
use katu_core::provider::{ModelSpec, ProviderRequest, ToolDef};

use crate::agent::{AgentError, TurnOptions};
use crate::runtime::Runtime;

/// Monta o pedido ao provider para um passo (histórico = projeção do log).
///
/// O `model` vem resolvido do passo (`Q2/PI_GAINS`): pode mudar entre passos do mesmo turno.
pub(super) fn build_request(
    runtime: &Runtime<'_>,
    options: &TurnOptions,
    tools: &[ToolDef],
    model: &ModelSpec,
) -> Result<ProviderRequest, AgentError> {
    let _span = katu_core::trace_fn!("agent::turn::request::build_request");

    let context = runtime.context()?;
    let system = system_for(
        options,
        &context,
        runtime.instructions(),
        &runtime.skills_catalog(),
    );
    Ok(ProviderRequest {
        model: model.clone(),
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
    let _span = katu_core::trace_fn!("agent::turn::request::system_for");

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

#[cfg(test)]
mod tests {
    use katu_core::context::Context;
    use katu_core::provider::ModelSpec;

    use super::system_for;
    use crate::agent::TurnOptions;

    fn options() -> TurnOptions {
        TurnOptions {
            model: ModelSpec::new("m"),
            system: Some("instrução".to_string()),
            max_tokens: 16,
            temperature: 0.0,
            max_steps: 1,
            idle_ms: 0,
            step_model: None,
        }
    }

    fn context() -> Context {
        Context {
            prime: "prime".to_string(),
            summary: None,
            messages: Vec::new(),
            raw_tokens: 0,
            tokens: 0,
        }
    }

    #[test]
    fn agents_md_leads_the_system_prompt() {
        let system = system_for(&options(), &context(), Some("regra máxima"), "skills: rust")
            .unwrap_or_default();
        assert!(system.starts_with("# AGENTS.md"), "{system}");
        let agents = system.find("regra máxima");
        let instruction = system.find("instrução");
        assert!(agents < instruction, "AGENTS.md vem primeiro: {system}");
        assert!(system.contains("skills: rust"), "{system}");
    }
}
