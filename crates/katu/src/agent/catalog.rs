//! Catálogo de tools enviado ao modelo (E06-T01/DF12): a **mesma** superfície fechada do registry.
//!
//! O catálogo deriva de [`katu_tools::schema::SCHEMAS`] — a fonte única já validada por
//! `xtask check-schemas` —, pelo que não há *drift* entre o que o modelo vê e o que o roteador
//! aceita. A ordem é a canónica do registry (estável entre execuções).

use katu_core::provider::ToolDef;
use katu_tools::schema::{ParamKind, SCHEMAS, ToolSchema};
use serde_json::{Map, Value, json};

/// Definições das 11 tools para o endpoint de modelo.
pub(super) fn tool_defs() -> Vec<ToolDef> {
    SCHEMAS.iter().map(def).collect()
}

/// Converte um `ToolSchema` no `ToolDef` do provider (JSON Schema dos parâmetros).
fn def(schema: &ToolSchema<'_>) -> ToolDef {
    let mut properties = Map::new();
    let mut required: Vec<Value> = Vec::new();
    for param in schema.params {
        properties.insert(param.name.to_string(), property(param.kind));
        if param.required {
            required.push(Value::String(param.name.to_string()));
        }
    }
    let parameters = json!({
        "type": "object",
        "properties": Value::Object(properties),
        "required": required,
        "additionalProperties": false,
    });
    ToolDef {
        name: schema.name.to_string(),
        description: schema.description.to_string(),
        parameters,
    }
}

/// JSON Schema de um parâmetro.
fn property(kind: ParamKind<'_>) -> Value {
    match kind {
        ParamKind::Text | ParamKind::Path => json!({ "type": "string" }),
        ParamKind::Integer => json!({ "type": "integer" }),
        ParamKind::Boolean => json!({ "type": "boolean" }),
        ParamKind::ListText => json!({ "type": "array", "items": { "type": "string" } }),
        ParamKind::Enum(values) => json!({ "type": "string", "enum": values }),
        ParamKind::Id(pattern) => json!({ "type": "string", "pattern": pattern }),
    }
}
