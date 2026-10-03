//! Decodificação de tool calls **declaradas como texto** (compatibilidade com modelos sem tool
//! calls nativas).
//!
//! Alguns modelos servidos por `llama-server` — o Qwen2.5-Coder local é o caso concreto — não
//! emitem `tool_calls` nativos: escrevem a chamada no `content` (num bloco de código, dentro de
//! `<tool_call>` ou como objeto JSON isolado), apesar de o pedido levar o catálogo de tools. Sem
//! esta camada o loop vê **zero** chamadas, o turno termina e o JSON cru aparece ao utilizador
//! como resposta final (`calls = 0`).
//!
//! A decodificação é **conservadora** e determinística:
//!
//! 1. o nome tem de existir no catálogo fechado de tools do pedido (nunca se inventa uma tool);
//! 2. a forma tem de ser explícita (`<tool_call>`, bloco de código ou objeto JSON isolado);
//! 3. os argumentos têm de ser um objeto JSON (ou uma string que o contenha).
//!
//! Prosa que apenas **mencione** uma tool não é uma chamada: o texto restante volta intacto para o
//! log/transcript. O que se remove é só o bloco consumido.

use katu_core::diag::{Level, events};
use katu_core::kernel::CallId;
use katu_core::provider::ToolDef;
use serde_json::{Value, json};

/// Chamadas declaradas extraídas de um passo.
pub(super) struct Declared {
    /// Chamadas na ordem em que aparecem (nome ao modelo + argumentos).
    pub(super) calls: Vec<(CallId, String, Value)>,
    /// Texto restante, sem os blocos consumidos (a prosa que o modelo escreveu).
    pub(super) remaining: String,
}

/// Extrai as chamadas declaradas no texto de um passo, se houver.
///
/// Devolve `None` quando não há nenhuma chamada reconhecível — nesse caso o texto é a resposta
/// final e não se toca nele.
#[must_use]
pub(super) fn extract(text: &str, tools: &[ToolDef]) -> Option<Declared> {
    let _span = katu_core::trace_fn!("agent::turn::declared::extract");

    let mut calls: Vec<(CallId, String, Value)> = Vec::new();
    let mut spans: Vec<(usize, usize)> = Vec::new();

    // 1. Protocolo explícito do template (Qwen): `<tool_call>{...}</tool_call>`.
    for (start, end, body) in tagged_blocks(text, "<tool_call>", "</tool_call>") {
        if let Some((name, arguments)) = parse_call(body, tools) {
            calls.push(declared_call(calls.len(), name, arguments));
            spans.push((start, end));
        }
    }

    // 2. Bloco de código (```json … ``` ou ``` … ```).
    if calls.is_empty() {
        for (start, end, body) in fenced_blocks(text) {
            if let Some((name, arguments)) = parse_call(body, tools) {
                calls.push(declared_call(calls.len(), name, arguments));
                spans.push((start, end));
            }
        }
    }

    // 3. Objeto JSON isolado (o texto inteiro é a chamada).
    if calls.is_empty() {
        let trimmed = text.trim();
        if let Some((name, arguments)) = parse_call(trimmed, tools) {
            calls.push(declared_call(0, name, arguments));
            spans.push((0, text.len()));
        }
    }

    if calls.is_empty() {
        return None;
    }
    katu_core::event!(Level::Info, events::TOOL_CALL, "declared" => calls.len());
    let remaining = remove_spans(text, &spans).trim().to_string();
    Some(Declared { calls, remaining })
}

/// Constrói o par (id, nome, argumentos) de uma chamada declarada.
fn declared_call(index: usize, name: String, arguments: Value) -> (CallId, String, Value) {
    let _span = katu_core::trace_fn!("agent::turn::declared::declared_call");

    (CallId::new(format!("declared-{index}")), name, arguments)
}

/// Interpreta um corpo como uma chamada de tool (nome no catálogo + argumentos objeto).
fn parse_call(raw: &str, tools: &[ToolDef]) -> Option<(String, Value)> {
    let _span = katu_core::trace_fn!("agent::turn::declared::parse_call");

    let value: Value = serde_json::from_str(raw.trim()).ok()?;
    let (name, arguments) = call_of(&value)?;
    if !tools.iter().any(|tool| tool.name == name) {
        return None;
    }
    Some((name, arguments))
}

/// Extrai `(nome, argumentos)` das formas aceites do objeto JSON.
///
/// Aceita `{"name":…, "arguments":…}` e a variante embrulhada `{"function":{"name":…, …}}`; os
/// `arguments` podem vir como objeto ou como string JSON (é o que o wire `OpenAI` usa).
fn call_of(value: &Value) -> Option<(String, Value)> {
    let _span = katu_core::trace_fn!("agent::turn::declared::call_of");

    let object = value.as_object()?;
    let (name, arguments) = if let Some(function) = object.get("function") {
        let function = function.as_object()?;
        (function.get("name")?.as_str()?, function.get("arguments"))
    } else {
        (object.get("name")?.as_str()?, object.get("arguments"))
    };
    let arguments = match arguments {
        None | Some(Value::Null) => json!({}),
        Some(Value::String(raw)) => serde_json::from_str(raw).ok()?,
        Some(other) => other.clone(),
    };
    if !arguments.is_object() {
        return None;
    }
    Some((name.to_string(), arguments))
}

/// Blocos delimitados por uma abertura e um fecho literais (ex.: `<tool_call>`).
fn tagged_blocks<'a>(text: &'a str, open: &str, close: &str) -> Vec<(usize, usize, &'a str)> {
    let _span = katu_core::trace_fn!("agent::turn::declared::tagged_blocks");

    let mut blocks = Vec::new();
    let mut search = 0;
    while let Some(relative) = text.get(search..).and_then(|rest| rest.find(open)) {
        let start = search.saturating_add(relative);
        let body_start = start.saturating_add(open.len());
        let Some(rest) = text.get(body_start..) else {
            break;
        };
        let Some(relative) = rest.find(close) else {
            break;
        };
        let body_end = body_start.saturating_add(relative);
        let end = body_end.saturating_add(close.len());
        if let Some(body) = text.get(body_start..body_end) {
            blocks.push((start, end, body));
        }
        search = end;
    }
    blocks
}

/// Blocos de código markdown (``` … ```); a primeira linha é a linguagem e é descartada.
fn fenced_blocks(text: &str) -> Vec<(usize, usize, &str)> {
    let _span = katu_core::trace_fn!("agent::turn::declared::fenced_blocks");

    let mut blocks = Vec::new();
    let mut search = 0;
    while let Some(relative) = text.get(search..).and_then(|rest| rest.find("```")) {
        let start = search.saturating_add(relative);
        let after = start.saturating_add(3);
        let Some(rest) = text.get(after..) else {
            break;
        };
        // A linha de abertura (linguagem) termina no primeiro `\n`; sem ela não há corpo.
        let Some(newline) = rest.find('\n') else {
            break;
        };
        let body_start = after.saturating_add(newline).saturating_add(1);
        let Some(body) = text.get(body_start..) else {
            break;
        };
        let Some(relative) = body.find("```") else {
            break;
        };
        let body_end = body_start.saturating_add(relative);
        let end = body_end.saturating_add(3);
        if let Some(inner) = text.get(body_start..body_end) {
            blocks.push((start, end, inner));
        }
        search = end;
    }
    blocks
}

/// Reconstroi o texto sem os intervalos consumidos (ordenados, não sobrepostos).
fn remove_spans(text: &str, spans: &[(usize, usize)]) -> String {
    let _span = katu_core::trace_fn!("agent::turn::declared::remove_spans");

    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for &(start, end) in spans {
        if start < cursor || end > text.len() {
            continue;
        }
        if let Some(before) = text.get(cursor..start) {
            out.push_str(before);
        }
        cursor = end;
    }
    if let Some(tail) = text.get(cursor..) {
        out.push_str(tail);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Declared, extract};
    use katu_core::provider::ToolDef;
    use serde_json::{Value, json};

    type TestError = Box<dyn std::error::Error>;
    type BorrowedCall<'a> = (&'a str, &'a Value);
    type OwnedCall = (String, Value);

    fn tools() -> Vec<ToolDef> {
        vec![
            ToolDef {
                name: "read".to_string(),
                description: "lê".to_string(),
                parameters: json!({"type": "object"}),
            },
            ToolDef {
                name: "bash".to_string(),
                description: "corre".to_string(),
                parameters: json!({"type": "object"}),
            },
        ]
    }

    fn parse(text: &str) -> Result<Declared, TestError> {
        extract(text, &tools()).ok_or_else(|| "sem chamada".into())
    }

    fn first(declared: &Declared) -> Result<BorrowedCall<'_>, TestError> {
        let call = declared.calls.first().ok_or("sem chamada")?;
        Ok((call.1.as_str(), &call.2))
    }

    fn one(text: &str) -> Result<OwnedCall, TestError> {
        let declared = parse(text)?;
        assert_eq!(declared.calls.len(), 1);
        let call = declared.calls.first().ok_or("sem chamada")?;
        Ok((call.1.clone(), call.2.clone()))
    }

    #[test]
    fn a_fenced_json_call_is_decoded_and_removed() -> Result<(), TestError> {
        let text = "```json\n{\"name\":\"read\",\"arguments\":{\"path\":\"Cargo.toml\"}}\n```";
        let declared = parse(text)?;
        let (name, arguments) = first(&declared)?;
        assert_eq!(name, "read");
        assert_eq!(arguments, &json!({"path": "Cargo.toml"}));
        assert!(declared.remaining.is_empty(), "{}", declared.remaining);
        Ok(())
    }

    #[test]
    fn prose_around_the_call_is_preserved() -> Result<(), TestError> {
        let text = "Vou ler o ficheiro.\n```json\n{\"name\":\"read\",\"arguments\":{\"path\":\"a\"}}\n```\nJá volto.";
        let declared = parse(text)?;
        assert_eq!(declared.remaining, "Vou ler o ficheiro.\n\nJá volto.");
        Ok(())
    }

    #[test]
    fn the_qwen_tool_call_tag_is_decoded() -> Result<(), TestError> {
        let text =
            "<tool_call>\n{\"name\":\"bash\",\"arguments\":{\"argv\":[\"ls\"]}}\n</tool_call>";
        let (name, arguments) = one(text)?;
        assert_eq!(name, "bash");
        assert_eq!(arguments, json!({"argv": ["ls"]}));
        Ok(())
    }

    #[test]
    fn a_bare_json_object_is_decoded() -> Result<(), TestError> {
        let (name, _) = one("{\"name\":\"read\",\"arguments\":{}}")?;
        assert_eq!(name, "read");
        Ok(())
    }

    #[test]
    fn arguments_as_a_json_string_are_decoded() -> Result<(), TestError> {
        let text = "{\"name\":\"read\",\"arguments\":\"{\\\"path\\\":\\\"x\\\"}\"}";
        let (_, arguments) = one(text)?;
        assert_eq!(arguments, json!({"path": "x"}));
        Ok(())
    }

    #[test]
    fn the_function_wrapper_is_accepted() -> Result<(), TestError> {
        let text = "{\"function\":{\"name\":\"read\",\"arguments\":{\"path\":\"y\"}}}";
        let (name, arguments) = one(text)?;
        assert_eq!(name, "read");
        assert_eq!(arguments, json!({"path": "y"}));
        Ok(())
    }

    #[test]
    fn a_name_outside_the_catalog_is_not_a_call() {
        assert!(
            extract(
                "{\"name\":\"delete_everything\",\"arguments\":{}}",
                &tools()
            )
            .is_none()
        );
    }

    #[test]
    fn malformed_json_is_not_a_call() {
        assert!(extract("```json\n{\"name\":\"read\",", &tools()).is_none());
    }

    #[test]
    fn prose_that_mentions_a_tool_is_not_a_call() {
        assert!(extract("Usa a tool read para ver o ficheiro.", &tools()).is_none());
    }

    #[test]
    fn multiple_tagged_calls_are_decoded_in_order() -> Result<(), TestError> {
        let text = "<tool_call>{\"name\":\"read\",\"arguments\":{\"path\":\"a\"}}</tool_call>\n\
                    <tool_call>{\"name\":\"read\",\"arguments\":{\"path\":\"b\"}}</tool_call>";
        let declared = parse(text)?;
        assert_eq!(declared.calls.len(), 2);
        let first = declared.calls.first().ok_or("primeira")?;
        let second = declared.calls.get(1).ok_or("segunda")?;
        assert_eq!(first.2, json!({"path": "a"}));
        assert_eq!(second.2, json!({"path": "b"}));
        assert_eq!(first.0.as_str(), "declared-0");
        assert_eq!(second.0.as_str(), "declared-1");
        Ok(())
    }

    #[test]
    fn a_plain_code_block_without_language_is_decoded() -> Result<(), TestError> {
        let text = "```\n{\"name\":\"read\",\"arguments\":{\"path\":\"z\"}}\n```";
        let (name, arguments) = one(text)?;
        assert_eq!(name, "read");
        assert_eq!(arguments, json!({"path": "z"}));
        Ok(())
    }
}
