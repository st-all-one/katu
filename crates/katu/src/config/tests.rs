//! Testes do módulo de configuração (E20-T09).

use super::{KEYS, Kind, find, get_key, merge, parse_value, set_key, unknown_key, unset_key};

#[test]
fn nested_set_get_unset_round_trips() {
    let mut table = toml::Table::new();
    set_key(
        &mut table,
        "embeddings.url",
        toml::Value::String("http://127.0.0.1:8081/v1".to_owned()),
    );
    assert_eq!(
        get_key(&table, "embeddings.url"),
        Some(toml::Value::String("http://127.0.0.1:8081/v1".to_owned()))
    );
    assert!(unset_key(&mut table, "embeddings.url"));
    assert_eq!(get_key(&table, "embeddings.url"), None);
    assert!(!unset_key(&mut table, "embeddings.url"));
}

#[test]
fn project_wins_over_global() {
    let mut global = toml::Table::new();
    set_key(
        &mut global,
        "provider",
        toml::Value::String("llama".to_owned()),
    );
    set_key(&mut global, "model", toml::Value::String("qwen".to_owned()));
    let mut project = toml::Table::new();
    set_key(
        &mut project,
        "model",
        toml::Value::String("outro".to_owned()),
    );
    merge(&mut global, &project);
    assert_eq!(
        get_key(&global, "provider"),
        Some(toml::Value::String("llama".to_owned()))
    );
    assert_eq!(
        get_key(&global, "model"),
        Some(toml::Value::String("outro".to_owned()))
    );
}

#[test]
fn parse_value_is_typed() {
    assert!(parse_value(Kind::Bool, "true").is_ok());
    assert!(parse_value(Kind::Bool, "sim").is_err());
    assert!(parse_value(Kind::Integer, "42").is_ok());
    assert!(parse_value(Kind::Integer, "x").is_err());
}

#[test]
fn unknown_key_is_rejected_with_hint() {
    assert!(find("embeddings.url").is_some());
    assert!(find("embeddings.uri").is_none());
    let error = unknown_key("providerr");
    assert!(error.to_string().contains("provider"));
}

/// Nenhuma chave pode ser prefixo pontuado de outra.
///
/// Em TOML, `provider` escalar e `[provider]` tabela colidem, e `set_key` descartaria a escrita em
/// silêncio (bug encontrado na chave outrora chamada `provider.structured_output`).
#[test]
fn keys_have_no_scalar_table_collisions() {
    for outer in KEYS {
        for inner in KEYS {
            if outer.key == inner.key {
                continue;
            }
            assert!(
                !inner.key.starts_with(&format!("{}.", outer.key)),
                "colisão escalar/tabela: `{}` é prefixo de `{}`",
                outer.key,
                inner.key
            );
        }
    }
}

/// `structured_output` é uma chave de topo.
///
/// Antes `provider.structured_output`, impossível com `provider` escalar.
#[test]
fn structured_output_round_trips_and_the_old_key_is_gone() {
    assert!(find("structured_output").is_some());
    assert!(find("provider.structured_output").is_none());
    let mut table = toml::Table::new();
    set_key(
        &mut table,
        "provider",
        toml::Value::String("llama".to_owned()),
    );
    set_key(&mut table, "structured_output", toml::Value::Boolean(false));
    assert_eq!(
        get_key(&table, "structured_output"),
        Some(toml::Value::Boolean(false))
    );
    assert_eq!(
        get_key(&table, "provider"),
        Some(toml::Value::String("llama".to_owned()))
    );
}
