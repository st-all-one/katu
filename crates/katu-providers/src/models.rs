//! Leitura do catálogo **do endpoint** (E12-T02): `dynamic_models` ao vivo.
//!
//! O formato varia por gateway: `OpenAI`/Anthropic/`llama.cpp` usam `{"data":[{"id":…}]}`; o Google
//! usa `{"models":[{"name":"models/…"}]}`. A leitura é **defensiva** (nunca falha por forma
//! inesperada: devolve o que reconhecer) e **determinística** (ordena e deduplica).

use serde_json::Value;

/// Extrai os ids de modelo de um corpo JSON, em ordem determinística.
///
/// Reconhece `data[].id`, `data[].name`, `models[].name` (prefixo `models/` removido) e um array de
/// topo. Um corpo que não seja JSON devolve vazio (o chamador cai no catálogo estático).
#[must_use]
pub(crate) fn parse_models(body: &str) -> Vec<String> {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return Vec::new();
    };
    let mut models = Vec::new();
    if let Some(entries) = value.get("data").and_then(Value::as_array) {
        collect(entries, &mut models);
    }
    if let Some(entries) = value.get("models").and_then(Value::as_array) {
        collect(entries, &mut models);
    }
    if let Some(entries) = value.as_array() {
        collect(entries, &mut models);
    }
    models.sort();
    models.dedup();
    models
}

/// Recolhe `id`/`name` (ou o próprio texto) de cada entrada.
fn collect(entries: &[Value], out: &mut Vec<String>) {
    for entry in entries {
        if let Some(text) = entry.as_str() {
            out.push(strip_prefix(text));
        } else if let Some(id) = entry.get("id").and_then(Value::as_str) {
            out.push(id.to_string());
        } else if let Some(name) = entry.get("name").and_then(Value::as_str) {
            out.push(strip_prefix(name));
        }
    }
}

/// Remove o prefixo `models/` do Google.
fn strip_prefix(name: &str) -> String {
    name.strip_prefix("models/").unwrap_or(name).to_string()
}

#[cfg(test)]
mod tests {
    use super::parse_models;
    use proptest::prelude::*;

    #[test]
    fn reads_openai_style_and_deduplicates() {
        let body = r#"{"object":"list","data":[{"id":"b"},{"id":"a"},{"id":"a"}]}"#;
        assert_eq!(parse_models(body), vec!["a", "b"]);
    }

    #[test]
    fn reads_google_style_and_strips_the_prefix() {
        let body = r#"{"models":[{"name":"models/gemini-1.5-pro"},
            {"name":"models/gemini-1.5-flash"}]}"#;
        assert_eq!(
            parse_models(body),
            vec!["gemini-1.5-flash", "gemini-1.5-pro"]
        );
    }

    #[test]
    fn reads_a_top_level_array_and_ignores_garbage() {
        assert_eq!(parse_models(r#"["m2","m1"]"#), vec!["m1", "m2"]);
        assert!(parse_models("not json").is_empty());
        assert!(parse_models("{}").is_empty());
    }

    proptest! {
        #[test]
        fn parsing_is_total_and_deterministic(body in any::<String>()) {
            let first = parse_models(&body);
            prop_assert_eq!(&first, &parse_models(&body));
            let mut sorted = first.clone();
            sorted.sort();
            sorted.dedup();
            prop_assert_eq!(first, sorted);
        }
    }
}
