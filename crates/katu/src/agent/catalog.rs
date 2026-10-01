//! Catálogo de tools enviado ao modelo (E06-T01/DF12): a **mesma** superfície fechada do registry.
//!
//! O catálogo deriva de [`katu_tools::schema::SCHEMAS`] — a fonte única já validada por
//! `xtask check-schemas` —, pelo que não há *drift* entre o que o modelo vê e o que o roteador
//! aceita. A ordem é a canónica do registry (estável entre execuções).
//!
//! A construção do JSON Schema vive em [`katu_tools::schema::tool_defs`] (Q-20): assim o gate
//! `xtask gate:prompt` mede exatamente o que o provider envia, sem uma segunda implementação.

use katu_core::provider::ToolDef;
use katu_tools::schema::tool_defs as schema_tool_defs;

/// Definições das 11 tools para o endpoint de modelo.
pub(super) fn tool_defs() -> Vec<ToolDef> {
    let _span = katu_core::trace_fn!("agent::catalog::tool_defs");

    schema_tool_defs()
}
