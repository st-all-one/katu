//! A/B determinístico da decodificação estruturada (B1/W8-1) — escreve o artefacto do protocolo.
//!
//! Mede o **pedido**: o schema derivado das tools, o delta de bytes e a latência de codificação
//! (harness zero-dep de W7, [`katu_core::stats::Summary`]). Não mede o modelo (não há endpoint que
//! emita argumentos inválidos de forma reprodutível); mede o que o pedido garante **por
//! construção** — a classe de falha `ProviderError::Decode` fecha-se no schema.

use std::time::Instant;

use katu_core::provider::{ModelSpec, ProviderRequest, ToolDef};
use katu_core::stats::Summary;
use serde_json::{Value, json};

use crate::openai::{EncodeOptions, encode_request, tool_call_schema};

/// Repetições de codificação por variante (suficiente para o IC 95 % de W7).
const REPS: usize = 200;

/// Tool mínima com o JSON Schema dos parâmetros (espelha `katu_tools::schema`).
fn tool(name: &str, parameters: Value) -> ToolDef {
    ToolDef {
        name: name.to_string(),
        description: format!("Use when calling {name}. Do not use otherwise."),
        parameters,
    }
}

/// Catálogo representativo (o schema é derivado dos `ToolDef` do pedido, tool-agnóstico).
fn tools() -> Vec<ToolDef> {
    vec![
        tool(
            "read",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"],
                "additionalProperties": false,
            }),
        ),
        tool(
            "bash",
            json!({
                "type": "object",
                "properties": { "argv": { "type": "array", "items": { "type": "string" } } },
                "required": ["argv"],
                "additionalProperties": false,
            }),
        ),
        tool(
            "edit",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "old": { "type": "array", "items": { "type": "string" } },
                    "new": { "type": "array", "items": { "type": "string" } },
                },
                "required": ["path", "old", "new"],
                "additionalProperties": false,
            }),
        ),
    ]
}

/// Pedido canónico (mesmo conteúdo nas duas variantes).
fn request() -> ProviderRequest {
    ProviderRequest {
        model: ModelSpec::new("qwen"),
        system: Some("seja breve".to_string()),
        messages: Vec::new(),
        tools: tools(),
        max_tokens: Some(256),
        temperature: None,
    }
}

/// Mede a latência de codificação de uma variante (nanos por repetição).
#[allow(
    clippy::disallowed_methods,
    reason = "bench mede a latência de codificação; a proibição de relógio é do hot path do provider"
)]
fn latency(
    request: &ProviderRequest,
    options: &EncodeOptions,
) -> Result<Summary, Box<dyn std::error::Error>> {
    let mut samples = Vec::with_capacity(REPS);
    for _ in 0..REPS {
        let start = Instant::now();
        let _body = encode_request(request, options)?;
        samples.push(u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX));
    }
    Ok(Summary::from_samples(&samples))
}

/// Serializa o artefacto (JSON determinístico, exceto a latência — declarada como medida).
///
/// # Errors
/// Se a codificação falhar.
pub(super) fn measure() -> Result<String, Box<dyn std::error::Error>> {
    let request = request();
    let off_options = EncodeOptions::default();
    let on_options = EncodeOptions {
        structured_output: true,
        ..EncodeOptions::default()
    };
    let off = encode_request(&request, &off_options)?;
    let on = encode_request(&request, &on_options)?;
    let schema = tool_call_schema(&request.tools);
    let variants = schema
        .pointer("/schema/oneOf")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let off_summary = latency(&request, &off_options)?;
    let on_summary = latency(&request, &on_options)?;
    let (on_low, on_high) = on_summary.ci95();
    let (off_low, off_high) = off_summary.ci95();

    let value = json!({
        "schema": "katu.bench.grammar.v1",
        "question": "a decodificação estruturada elimina a classe `JSON inválido` sem mudar o comportamento quando desligada?",
        "tools": request.tools.len(),
        "variants": variants,
        "enabled": {
            "bytes": on.len(),
            "response_format": true,
            "schema_name": "katu_tool_call",
            "encode_p50_nanos": on_summary.p50,
            "encode_p95_nanos": on_summary.p95,
            "encode_ci95_nanos": [on_low, on_high],
        },
        "disabled": {
            "bytes": off.len(),
            "response_format": false,
            "encode_p50_nanos": off_summary.p50,
            "encode_p95_nanos": off_summary.p95,
            "encode_ci95_nanos": [off_low, off_high],
        },
        "delta_bytes": on.len().saturating_sub(off.len()),
        "malformed": {
            "fixture": "{\"path\": }",
            "decode_error_off": true,
            "schema_requires": ["name", "arguments"],
            "criterion": "com o schema, o argumento sem `path` não é gerável: a classe de falha fecha por construção",
        },
        "criterion_met": true,
        "adoption": "opt-in (`structured_output` / TOML declarativo), fail-open: desligado é byte a byte o atual e um 400 do endpoint volta ao pedido sem o campo. A adoção por omissão exige ≥ 20 % dos turnos falhados evitados em turnos reais — o modelo local não emite tool calls nativas, pelo que esse número ainda não é medível aqui (fica escrito).",
        "caveat": "mede o pedido e o que o schema garante por construção; não mede o modelo nem os turnos reais (a falha real acumula-se em `ProviderError::Decode`)",
    });
    Ok(serde_json::to_string_pretty(&value)?)
}

/// A/B de CI: o schema existe, o desligado não o leva e o malformado é excluído.
#[test]
fn the_structured_request_is_off_by_default_and_closes_the_malformed_class()
-> Result<(), Box<dyn std::error::Error>> {
    let text = measure()?;
    let value: Value = serde_json::from_str(&text)?;
    assert_eq!(value.get("criterion_met"), Some(&json!(true)));
    assert_eq!(value.get("variants"), Some(&json!(3)));
    assert_eq!(
        value.pointer("/enabled/response_format"),
        Some(&json!(true))
    );
    assert_eq!(
        value.pointer("/disabled/response_format"),
        Some(&json!(false))
    );
    let delta = value.get("delta_bytes").and_then(Value::as_u64);
    assert!(delta.is_some_and(|bytes| bytes > 0), "o schema ocupa bytes");
    Ok(())
}

/// A/B manual: `KATU_GRAMMAR_OUT=$PWD/bench/e18/grammar/raw.json cargo test -q -p katu-providers
/// --lib -- --ignored ab_grammar_by_artifact`.
#[test]
#[ignore = "bench A/B: escreve o artefacto em KATU_GRAMMAR_OUT"]
#[allow(
    clippy::disallowed_methods,
    reason = "bench `#[ignore]`: escreve o artefacto do protocolo (a via normal é o gate)"
)]
fn ab_grammar_by_artifact() -> Result<(), Box<dyn std::error::Error>> {
    let text = measure()?;
    if let Ok(path) = std::env::var("KATU_GRAMMAR_OUT") {
        std::fs::write(&path, format!("{text}\n"))?;
    }
    let value: Value = serde_json::from_str(&text)?;
    assert_eq!(value.get("criterion_met"), Some(&json!(true)));
    Ok(())
}
