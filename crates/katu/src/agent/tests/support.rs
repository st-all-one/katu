//! Apoio dos testes do loop de turnos: pedido, raiz temporária, guiões de tool e opções.

use std::path::PathBuf;

use katu_core::kernel::CallId;
use katu_core::ports::NO_PROGRESS;
use katu_core::provider::{ModelSpec, Provider, ProviderEvent};
use serde_json::json;

use super::{Ports, TurnOptions, TurnRequest};

/// Pedido de turno a partir dos componentes (o `ports` é `Copy`).
pub(crate) fn request<'a>(
    provider: &'a std::sync::Arc<dyn Provider>,
    ports: Ports<'a>,
    goal: &'a str,
    options: &'a TurnOptions,
) -> TurnRequest<'a> {
    TurnRequest {
        provider: std::sync::Arc::clone(provider),
        ports,
        goal,
        options,
        cancel: None,
        progress: &NO_PROGRESS,
    }
}

/// Raiz temporária única por teste.
pub(crate) fn root(label: &str) -> Result<PathBuf, std::io::Error> {
    let path = std::env::temp_dir().join(format!("katu-agent-{}-{label}", std::process::id()));
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

/// Ferramenta de escrita que o modelo pede no guião.
pub(crate) fn write_call() -> ProviderEvent {
    ProviderEvent::ToolCall {
        call: CallId::new("c1"),
        name: "write".to_string(),
        arguments: json!({"path": "new.txt", "content": "olá"}),
    }
}

/// Ferramenta de leitura que o modelo pede no guião.
pub(crate) fn read_call() -> ProviderEvent {
    ProviderEvent::ToolCall {
        call: CallId::new("c1"),
        name: "read".to_string(),
        arguments: json!({"path": "nota.txt", "view": "full"}),
    }
}

/// Opções mínimas de turno.
pub(crate) fn options(max_steps: u32) -> TurnOptions {
    TurnOptions {
        model: ModelSpec::new("fake"),
        system: None,
        max_tokens: 128,
        temperature: 0.0,
        max_steps,
        idle_ms: 0,
        step_model: None,
    }
}

/// Opções com teto de inatividade do stream (L-P2) explícito.
pub(crate) fn options_idle(max_steps: u32, idle_ms: u64) -> TurnOptions {
    TurnOptions {
        idle_ms,
        ..options(max_steps)
    }
}
