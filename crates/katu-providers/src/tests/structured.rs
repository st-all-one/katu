//! Testes da decodificação estruturada (B1/W8-1): schema, bytes, malformado e fail-open.

use katu_core::provider::{
    CollectSink, Flow, ModelSpec, Provider, ProviderError, ProviderRequest, StopReason, ToolDef,
};
use serde_json::{Value, json};

use crate::declarative::{Declarative, ProviderSpec};
use crate::openai::{EncodeOptions, encode_request, tool_call_schema};
use crate::transport::{ChunkSink, HttpMeta, HttpRequest, Transport, TransportError};
use crate::wire::parse_arguments;

/// Pedido mínimo de teste.
fn request(model: &str) -> ProviderRequest {
    ProviderRequest {
        model: ModelSpec::new(model),
        system: Some("seja breve".to_string()),
        messages: Vec::new(),
        tools: Vec::new(),
        max_tokens: Some(64),
        temperature: None,
    }
}

/// Tool mínima com `path` obrigatório (espelha `katu_tools::schema`).
fn read_tool() -> ToolDef {
    ToolDef {
        name: "read".to_string(),
        description: "Use when reading. Do not use for writing.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": { "path": { "type": "string" } },
            "required": ["path"],
            "additionalProperties": false,
        }),
    }
}

#[test]
fn structured_output_is_off_by_default() -> Result<(), Box<dyn std::error::Error>> {
    let mut request = request("m");
    request.tools = vec![read_tool()];
    let body = encode_request(&request, &EncodeOptions::default())?;
    let value: Value = serde_json::from_str(&body)?;
    assert!(
        value.get("response_format").is_none(),
        "desligado não muda os bytes"
    );
    Ok(())
}

#[test]
fn structured_output_adds_a_schema_derived_from_the_tools() -> Result<(), Box<dyn std::error::Error>>
{
    let mut request = request("m");
    request.tools = vec![read_tool()];
    let off = encode_request(&request, &EncodeOptions::default())?;
    let off_value: Value = serde_json::from_str(&off)?;
    let options = EncodeOptions {
        structured_output: true,
        ..EncodeOptions::default()
    };
    let on = encode_request(&request, &options)?;
    let on_value: Value = serde_json::from_str(&on)?;
    assert_eq!(
        on_value.pointer("/response_format/type"),
        Some(&json!("json_schema"))
    );
    assert_eq!(
        on_value.pointer("/response_format/json_schema/name"),
        Some(&json!("katu_tool_call"))
    );
    let variants = on_value
        .pointer("/response_format/json_schema/schema/oneOf")
        .and_then(Value::as_array)
        .ok_or("sem oneOf")?;
    assert_eq!(variants.len(), 1);
    assert_eq!(
        variants
            .first()
            .and_then(|v| v.pointer("/properties/name/const")),
        Some(&json!("read"))
    );
    assert_eq!(
        variants
            .first()
            .and_then(|v| v.pointer("/properties/arguments/required")),
        Some(&json!(["path"]))
    );
    // O único delta face ao desligado é o campo `response_format`.
    let mut stripped = on_value.clone();
    if let Some(object) = stripped.as_object_mut() {
        object.remove("response_format");
    }
    assert_eq!(stripped, off_value);
    Ok(())
}

#[test]
fn structured_output_without_tools_is_a_no_op() -> Result<(), Box<dyn std::error::Error>> {
    let options = EncodeOptions {
        structured_output: true,
        ..EncodeOptions::default()
    };
    let body = encode_request(&request("m"), &options)?;
    let value: Value = serde_json::from_str(&body)?;
    assert!(value.get("response_format").is_none());
    Ok(())
}

#[test]
fn the_schema_excludes_malformed_arguments() -> Result<(), Box<dyn std::error::Error>> {
    // O fixture malformado (truncado) falha hoje a decodificação.
    assert!(matches!(
        parse_arguments(r#"{"path": }"#),
        Err(ProviderError::Decode(_))
    ));
    // O esquema derivado exige `path`: a gramática não pode gerar o fixture.
    let schema = tool_call_schema(&[read_tool()]);
    let variant = schema.pointer("/schema/oneOf/0").ok_or("sem variante")?;
    assert_eq!(
        variant.pointer("/properties/arguments/required"),
        Some(&json!(["path"]))
    );
    assert_eq!(variant.get("additionalProperties"), Some(&json!(false)));
    // O fixture válido (o que a gramática pode gerar) decodifica.
    let valid = parse_arguments(r#"{"path": "/work/a.rs"}"#)?;
    assert_eq!(valid.get("path"), Some(&json!("/work/a.rs")));
    Ok(())
}

/// Stream mínimo de texto (fim natural) para o teste de fail-open.
const OK_STREAM: &str = concat!(
    "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Ol\"}}]}\n\n",
    "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
    "data: [DONE]\n\n",
);

/// Transporte que devolve `400` ao primeiro `POST` e o stream `200` ao segundo, guardando os corpos.
struct FlakyTransport {
    bodies: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl Transport for FlakyTransport {
    fn send(
        &self,
        request: &HttpRequest,
        sink: &mut ChunkSink<'_>,
    ) -> Result<HttpMeta, TransportError> {
        let body = request
            .body
            .as_ref()
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
            .unwrap_or_default();
        let mut bodies = self
            .bodies
            .lock()
            .map_err(|_| TransportError::Protocol("lock envenenado".to_string()))?;
        bodies.push(body);
        let first = bodies.len() == 1;
        drop(bodies);
        let (status, payload): (u16, &[u8]) = if first {
            (400, br#"{"error":"response_format unsupported"}"#)
        } else {
            (200, OK_STREAM.as_bytes())
        };
        let mut bytes = 0_u64;
        for fragment in payload.chunks(4096) {
            bytes = bytes.saturating_add(u64::try_from(fragment.len()).unwrap_or(u64::MAX));
            if matches!(sink(fragment), Flow::Break) {
                break;
            }
        }
        Ok(HttpMeta {
            status,
            headers: Vec::new(),
            bytes,
        })
    }
}

#[test]
fn a_rejected_structured_request_falls_back_to_the_current_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let bodies = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let transport = FlakyTransport {
        bodies: std::sync::Arc::clone(&bodies),
    };
    let spec = ProviderSpec::from_json(
        r#"{
            "name": "flaky",
            "engine": "openai",
            "base_url": "https://example.invalid/v1",
            "structured_output": true,
            "models": [{ "name": "m" }]
        }"#,
    )?;
    let provider = Declarative::new(transport, spec);
    let mut request = request("m");
    request.tools = vec![read_tool()];
    let mut sink = CollectSink::default();
    let outcome = provider.stream(&request, &mut sink)?;
    assert_eq!(sink.text, "Ol");
    assert_eq!(outcome.stop, StopReason::EndTurn);
    let bodies = bodies.lock().map_err(|_| "lock envenenado")?.clone();
    assert_eq!(bodies.len(), 2, "uma tentativa com e outra sem o campo");
    let first = bodies.first().ok_or("sem 1.ª tentativa")?;
    let second = bodies.get(1).ok_or("sem 2.ª tentativa")?;
    assert!(first.contains("response_format"), "1.ª leva o schema");
    assert!(
        !second.contains("response_format"),
        "a repetição volta aos bytes atuais"
    );
    Ok(())
}
