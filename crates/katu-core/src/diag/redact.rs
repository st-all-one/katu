//! Redação de segredos no caminho de diagnóstico (E01-T07).
//!
//! Regra: um campo cuja **chave** é sensível (`token`, `authorization`, `secret`, corpos…) ou cujo
//! **valor** traz um marcador de segredo (`[secrets]`, `Bearer …`, `Authorization:`, chaves
//! PEM/`sk-…`) nunca é emitido em claro — é substituído por [`REDACTED`]. O resto passa intacto.
//!
//! Isto é uma **allowlist por omissão**: só campos estruturados com chave conhecida e valor sem
//! marcador são públicos. A decisão é pura e testável; o sink apenas a aplica.

use super::Value;

/// Marcador emitido no lugar de um valor redigido.
pub const REDACTED: &str = "[redacted]";

/// Chaves (em minúsculas) cujo valor textual é sempre redigido.
const SENSITIVE_KEYS: &[&str] = &[
    "secret",
    "token",
    "password",
    "passwd",
    "authorization",
    "auth",
    "cookie",
    "credential",
    "api_key",
    "apikey",
    "private_key",
    "body",
    "content",
];

/// Marcadores de segredo detetados **dentro** de um valor textual.
const SECRET_MARKERS: &[&str] = &[
    "[secrets]",
    "-----begin",
    "bearer ",
    "authorization:",
    "sk-",
    "ghp_",
    "xoxb-",
];

/// `true` se a chave do campo é sensível (comparação por substring, minúsculas).
#[must_use]
pub fn is_sensitive_key(key: &str) -> bool {
    let lowered = key.to_ascii_lowercase();
    SENSITIVE_KEYS.iter().any(|needle| lowered.contains(needle))
}

/// `true` se o valor textual contém um marcador de segredo.
#[must_use]
pub fn is_sensitive_value(value: &str) -> bool {
    let lowered = value.to_ascii_lowercase();
    SECRET_MARKERS.iter().any(|needle| lowered.contains(needle))
}

/// Aplica a redação a um campo: mantém o valor, salvo se a chave ou o valor forem sensíveis.
#[must_use]
pub fn redact_value<'a>(key: &str, value: Value<'a>) -> Value<'a> {
    if is_sensitive_key(key) {
        return match value {
            Value::Str(_) => Value::Str(REDACTED),
            other => other,
        };
    }
    match value {
        Value::Str(text) if is_sensitive_value(text) => Value::Str(REDACTED),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::{REDACTED, is_sensitive_key, is_sensitive_value, redact_value};
    use crate::diag::Value;

    #[test]
    fn flags_sensitive_keys() {
        assert!(is_sensitive_key("Authorization"));
        assert!(is_sensitive_key("api_key"));
        assert!(is_sensitive_key("body"));
        assert!(!is_sensitive_key("path"));
        assert!(!is_sensitive_key("event"));
    }

    #[test]
    fn redacts_sensitive_keys_and_values() {
        assert_eq!(
            redact_value("token", Value::Str("abc")),
            Value::Str(REDACTED)
        );
        assert_eq!(
            redact_value("note", Value::Str("Bearer abc")),
            Value::Str(REDACTED)
        );
        assert_eq!(
            redact_value("note", Value::Str("público")),
            Value::Str("público")
        );
        // Campos não-textuais nunca são tocados.
        assert_eq!(redact_value("token", Value::Uint(7)), Value::Uint(7));
    }

    #[test]
    fn detects_value_markers() {
        assert!(is_sensitive_value("Authorization: Bearer x"));
        assert!(is_sensitive_value("-----BEGIN PRIVATE KEY-----"));
        assert!(!is_sensitive_value("caminho/normal.rs"));
    }
}
