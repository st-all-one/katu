//! JSON Schema das tools para o endpoint de modelo (E06-T01/DF12/Q-06/Q-20).
//!
//! Deriva de [`SCHEMAS`] — a fonte única já validada por `xtask check-schemas` —, pelo que não há
//! *drift* entre o que o modelo vê e o que o roteador aceita. Vive em `katu-tools` (e não no
//! binário) para que o **gate** `xtask gate:prompt` meça exatamente o que o provider envia
//! (Q-20): a apresentação das tools é um facto desta crate.
//!
//! A descrição de cada parâmetro **ensina** o modelo (Q-06): era validada pelo linter e descartada
//! antes de chegar ao endpoint, o que fazia o modelo adivinhar o sentido dos campos.

use katu_core::provider::ToolDef;
use serde_json::{Map, Value, json};

use super::{ParamKind, ParamSpec, SCHEMAS, ToolSchema};

/// Definições das tools para o endpoint de modelo (ordem canónica do registry).
#[must_use]
pub fn tool_defs() -> Vec<ToolDef> {
    let _span = katu_core::trace_fn!("schema::tool_defs");

    SCHEMAS.iter().map(def).collect()
}

/// Converte um `ToolSchema` no `ToolDef` do provider (JSON Schema dos parâmetros).
fn def(schema: &ToolSchema<'_>) -> ToolDef {
    let _span = katu_core::trace_fn!("schema::def");

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

/// JSON Schema de um parâmetro, incluindo a descrição que **ensina** o modelo (E06-T02/Q-06).
fn property(param: &ParamSpec<'_>) -> Value {
    let _span = katu_core::trace_fn!("schema::property");

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

/// Forma **wire** de uma tool (`OpenAI`): `{"type":"function","function":{…}}`.
///
/// Espelha [`katu_providers::openai`] — mas os providers são uma camada acima e o gate do prompt
/// não pode depender deles; a forma é o contrato do endpoint, estável e testada do lado do
/// provider (`openai/encode.rs`).
#[must_use]
pub fn wire_json(tool: &ToolDef) -> Value {
    let _span = katu_core::trace_fn!("schema::wire_json");

    json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.parameters,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{tool_defs, wire_json};

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

    #[test]
    fn wire_json_matches_the_openai_shape() {
        for tool in tool_defs() {
            let wire = wire_json(&tool);
            assert_eq!(
                wire.get("type").and_then(serde_json::Value::as_str),
                Some("function")
            );
            let function = wire.get("function").and_then(serde_json::Value::as_object);
            assert!(function.is_some_and(|map| map.contains_key("parameters")));
        }
    }
}
