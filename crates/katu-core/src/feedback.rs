//! Feedback de comando (E06-T07): registo estruturado, redação e truncagem determinística.
//!
//! Cada comando produz um [`CommandRecord`] com `stdout_tail`/`stderr_tail`/`exit_code`/
//! `duration_ms`/`parent_command_id`. O texto é **redigido** (segredos nunca chegam ao log) e
//! **truncado pela cauda** (determinístico). `exit_code: null` (morto por sinal/timeout) marca o
//! comando como **ambíguo** e o kernel recusa avançar (§31).
//!
//! A **rotação** de ficheiros fica deliberadamente de fora: apagar/sobrepor logs é uma decisão
//! explícita (o projeto nunca apaga automaticamente); ver E01-T07.

use serde::{Deserialize, Serialize};

/// Limite por omissão da cauda (bytes).
pub const DEFAULT_TAIL: usize = 4_096;

/// Fragmentos que marcam uma chave como sensível (redação no write).
const SECRET_MARKERS: &[&str] = &[
    "KEY",
    "SECRET",
    "TOKEN",
    "PASSWORD",
    "PASSWD",
    "CREDENTIAL",
    "AUTHORIZATION",
];

/// Registo estruturado de um comando.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandRecord {
    /// Identificador content-addressed do comando.
    pub id: String,
    /// Argumentos (o primeiro é o programa).
    pub argv: Vec<String>,
    /// Diretório de trabalho.
    pub cwd: String,
    /// Código de saída (`None` se morto por sinal/timeout).
    pub exit_code: Option<i32>,
    /// Sinal que matou o processo.
    pub signal: Option<i32>,
    /// `true` se o timeout disparou.
    pub timed_out: bool,
    /// Duração de *wall-clock* em milissegundos.
    pub duration_ms: u64,
    /// Cauda de `stdout` (redigida + truncada).
    pub stdout_tail: String,
    /// Cauda de `stderr` (redigida + truncada).
    pub stderr_tail: String,
    /// Comando pai, se aninhado.
    pub parent_command_id: Option<String>,
}

/// Estado compacto do último comando (pré-condição do kernel).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandStatus {
    /// Código de saída (`None` = ambíguo).
    pub exit_code: Option<i32>,
    /// `true` se o timeout disparou.
    pub timed_out: bool,
}

impl CommandStatus {
    /// `true` se o comando é **ambíguo** (`exit_code: null`) — bloqueia avançar (§31).
    #[must_use]
    pub const fn is_ambiguous(self) -> bool {
        self.exit_code.is_none()
    }
}

impl CommandRecord {
    /// Estado compacto derivado do registo.
    #[must_use]
    pub const fn status(&self) -> CommandStatus {
        CommandStatus {
            exit_code: self.exit_code,
            timed_out: self.timed_out,
        }
    }
}

/// Guarda os **últimos** `max` bytes, num limite de caractere válido.
#[must_use]
pub fn tail(text: &str, max: usize) -> String {
    let _span = crate::trace_fn!("feedback::tail");

    if text.len() <= max {
        return text.to_string();
    }
    let mut start = text.len().saturating_sub(max);
    while start < text.len() && !text.is_char_boundary(start) {
        start = start.saturating_add(1);
    }
    format!("…{}", text.get(start..).unwrap_or_default())
}

/// Redige segredos do texto, linha a linha (chaves `*KEY*`/`*TOKEN*`/`Authorization`, …).
#[must_use]
pub fn redact(text: &str) -> String {
    let _span = crate::trace_fn!("feedback::redact");

    text.lines().map(redact_line).collect::<Vec<_>>().join("\n")
}

fn redact_line(line: &str) -> String {
    let _span = crate::trace_fn!("feedback::redact_line");

    for separator in [':', '='] {
        if let Some((left, _)) = line.split_once(separator) {
            let key = left.trim().to_ascii_uppercase();
            if SECRET_MARKERS.iter().any(|marker| key.contains(marker)) {
                let trimmed = left.trim_end();
                return format!("{trimmed}{separator} [redacted]");
            }
        }
    }
    line.to_string()
}

#[cfg(test)]
mod tests {
    use super::{CommandStatus, redact, tail};

    #[test]
    fn tail_keeps_the_end() {
        let text = "0123456789";
        assert_eq!(tail(text, 4), "…6789");
        assert_eq!(tail(text, 100), text);
    }

    #[test]
    fn tail_respects_char_boundaries() {
        let text = "aé"; // 3 bytes; cortar a 2 não parte o 'é'
        let out = tail(text, 2);
        assert!(out.ends_with('é'), "{out}");
    }

    #[test]
    fn redact_hides_secret_values() {
        let text = "MY_SECRET=abc123\nAuthorization: Bearer xyz\nPATH=/bin";
        let out = redact(text);
        assert!(!out.contains("abc123"), "{out}");
        assert!(!out.contains("xyz"), "{out}");
        assert!(out.contains("PATH=/bin"), "{out}");
    }

    #[test]
    fn ambiguous_status_when_no_exit_code() {
        assert!(
            CommandStatus {
                exit_code: None,
                timed_out: true,
            }
            .is_ambiguous()
        );
        assert!(
            !CommandStatus {
                exit_code: Some(0),
                timed_out: false,
            }
            .is_ambiguous()
        );
    }
}
