//! Catálogo de tools enviado ao modelo (E06-T01/DF12): a **mesma** superfície fechada do registry.
//!
//! O catálogo deriva de [`katu_tools::schema::SCHEMAS`] — a fonte única já validada por
//! `xtask check-schemas` —, pelo que não há *drift* entre o que o modelo vê e o que o roteador
//! aceita. A ordem é a canónica do registry (estável entre execuções).

use katu_core::provider::ToolDef;
use katu_tools::schema::{ParamKind, ParamSpec, SCHEMAS, ToolSchema};
use serde_json::{Map, Value, json};

/// Definições das 11 tools para o endpoint de modelo.
pub(super) fn tool_defs() -> Vec<ToolDef> {
    let _span = katu_core::trace_fn!("agent::catalog::tool_defs");

    SCHEMAS.iter().map(def).collect()
}

/// Converte um `ToolSchema` no `ToolDef` do provider (JSON Schema dos parâmetros).
fn def(schema: &ToolSchema<'_>) -> ToolDef {
    let _span = katu_core::trace_fn!("agent::catalog::def");

    let mut properties = Map::new();
    let mut required: Vec<Value> = Vec::new();
    for param in schema.params {
        properties.insert(param.name.to_string(), property(param));
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
/// JSON Schema de um parâmetro, incluindo a descrição que **ensina** o modelo (E06-T02).
///
/// A descrição das specs era validada pelo linter mas descartada antes de chegar ao endpoint;
/// incluí-la reduz o uso errado do campo (o modelo decide pelo nome + descrição).
fn property(param: &ParamSpec<'_>) -> Value {
    let _span = katu_core::trace_fn!("agent::catalog::property");

    let mut schema = match param.kind {
        ParamKind::Text | ParamKind::Path => json!({ "type": "string" }),
        ParamKind::Integer => json!({ "type": "integer" }),
        ParamKind::Boolean => json!({ "type": "boolean" }),
        ParamKind::ListText => json!({ "type": "array", "items": { "type": "string" } }),
        ParamKind::Enum(values) => json!({ "type": "string", "enum": values }),
        ParamKind::Id(pattern) => json!({ "type": "string", "pattern": pattern }),
    };
    if let Value::Object(map) = &mut schema {
        map.insert(
            "description".to_string(),
            Value::String(param.description.to_string()),
        );
    }
    schema
}

#[cfg(test)]
mod tests {
    use super::tool_defs;

    #[test]
    fn every_parameter_carries_its_description() {
        for tool in tool_defs() {
            let properties = tool
                .parameters
                .get("properties")
                .and_then(serde_json::Value::as_object);
            assert!(
                properties.is_some(),
                "tool `{}` sem `properties`",
                tool.name
            );
            let Some(properties) = properties else {
                continue;
            };
            assert!(
                !properties.is_empty(),
                "tool `{}` sem parâmetros",
                tool.name
            );
            for (name, schema) in properties {
                let description = schema
                    .get("description")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                assert!(
                    !description.is_empty(),
                    "parâmetro `{name}` da tool `{}` sem descrição",
                    tool.name
                );
            }
        }
    }
}
