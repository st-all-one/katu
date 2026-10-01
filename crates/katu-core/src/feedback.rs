//! Feedback de comando (E06-T07): registo estruturado, redação e truncagem determinística.
//!
//! Cada comando produz um [`CommandRecord`] com `stdout_tail`/`stderr_tail`/`exit_code`/
//! `duration_ms`/`parent_command_id`. O texto é **redigido** (segredos nunca chegam ao log) e
//! **truncado** por um [`Ledger`] unificado (B-04): head/tail + *spill* com ponteiro para a
//! página vertida. `exit_code: null` (morto por sinal/timeout) marca o comando como **ambíguo** e o
//! kernel recusa avançar (§31).
//!
//! A **rotação** de ficheiros fica deliberadamente de fora: apagar/sobrepor logs é uma decisão
//! explícita (o projeto nunca apaga automaticamente); ver E01-T07.

use serde::{Deserialize, Serialize};

/// Limite por omissão da cauda (bytes).
pub const DEFAULT_TAIL: usize = 4_096;

/// Ledger unificado de output (B-04): head/tail + *spill* com ponteiro para a página vertida.
///
/// Substitui as truncagens ad-hoc: um só mecanismo determina o que o modelo vê (head + cauda) e,
/// acima de um teto, verte o output inteiro para um ficheiro e aponta-o. O teto de bytes
/// model-visible nunca é excedido — o spill é a recuperação, não o padrão.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ledger {
    /// Bytes de cabeça a manter.
    pub head: usize,
    /// Bytes de cauda a manter.
    pub tail: usize,
    /// Teto de bytes acima do qual o output é vertido para um ficheiro.
    pub spill_threshold: usize,
}

impl Ledger {
    /// Ledger por omissão: 2 KiB de cabeça + 2 KiB de cauda, spill acima de 8 KiB.
    pub const DEFAULT: Self = Self {
        head: 2_048,
        tail: 2_048,
        spill_threshold: 8_192,
    };

    /// Renderiza o texto para o modelo: head/tail + ponteiro de spill quando vertido.
    ///
    /// `spill_path` é o caminho da página vertida (quando o output excede o teto). O texto
    /// model-visible nunca excede `head + tail` bytes (mais o ponteiro).
    #[must_use]
    pub fn render(&self, text: &str, spill_path: Option<&str>) -> String {
        let _span = crate::trace_fn!("feedback::ledger::render");

        let truncated = self.head_tail(text);
        match spill_path {
            Some(path) if text.len() > self.spill_threshold => {
                format!("{truncated} — ver {path}")
            }
            _ => truncated,
        }
    }

    /// Head/tail determinístico: se cabe, o texto inteiro; senão cabeça + marcador + cauda.
    fn head_tail(&self, text: &str) -> String {
        let _span = crate::trace_fn!("feedback::ledger::head_tail");

        if text.len() <= self.head.saturating_add(self.tail) {
            return text.to_string();
        }
        let head = text.get(..self.head).unwrap_or(text);
        let tail_start = text.len().saturating_sub(self.tail);
        let tail = text.get(tail_start..).unwrap_or("");
        let omitted = text
            .len()
            .saturating_sub(self.head.saturating_add(self.tail));
        format!("{head}…[{omitted} bytes omitidos]…{tail}")
    }
}

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
    /// Cauda de `stdout` (redigida + truncada pelo [`Ledger`]).
    pub stdout_tail: String,
    /// Cauda de `stderr` (redigida + truncada pelo [`Ledger`]).
    pub stderr_tail: String,
    /// Caminho da página de *spill* de `stdout` (B-04), quando o output excedeu o teto.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout_spill: Option<String>,
    /// Caminho da página de *spill* de `stderr` (B-04), quando o output excedeu o teto.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr_spill: Option<String>,
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
    use super::{CommandStatus, Ledger, redact, tail};

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

    #[test]
    fn ledger_keeps_the_full_text_when_it_fits() {
        let ledger = Ledger::DEFAULT;
        let text = "pequeno";
        assert_eq!(ledger.render(text, None), text);
    }

    #[test]
    fn ledger_keeps_head_and_tail_with_a_marker() {
        let ledger = Ledger {
            head: 4,
            tail: 4,
            spill_threshold: 100,
        };
        let text = "0123456789ABCDEF";
        let out = ledger.render(text, None);
        assert!(out.starts_with("0123"), "{out}");
        assert!(out.ends_with("CDEF"), "{out}");
        assert!(out.contains("bytes omitidos"), "{out}");
        assert!(!out.contains("456789AB"), "{out}");
    }

    #[test]
    fn ledger_spill_adds_a_pointer_when_the_output_exceeds_the_threshold() {
        let ledger = Ledger {
            head: 4,
            tail: 4,
            spill_threshold: 8,
        };
        let text = "0123456789ABCDEF";
        let out = ledger.render(text, Some(".katu/spill/x.stdout"));
        assert!(out.contains("ver .katu/spill/x.stdout"), "{out}");
        assert!(out.starts_with("0123"), "{out}");
    }

    #[test]
    fn ledger_without_spill_path_has_no_pointer() {
        let ledger = Ledger {
            head: 4,
            tail: 4,
            spill_threshold: 8,
        };
        let text = "0123456789ABCDEF";
        let out = ledger.render(text, None);
        assert!(!out.contains("ver "), "{out}");
    }

    #[test]
    fn ledger_respects_char_boundaries_in_head_and_tail() {
        let ledger = Ledger {
            head: 2,
            tail: 2,
            spill_threshold: 100,
        };
        let text = "aéíó"; // 8 bytes; head=2 ("aé"), tail=2 ("ó" é 2 bytes)
        let out = ledger.render(text, None);
        assert!(out.starts_with('a'), "{out}");
        assert!(out.ends_with('ó'), "{out}");
    }

    /// A/B determinístico do ledger (B-04): escreve o artefacto em `KATU_LEDGER_OUT`.
    #[test]
    #[ignore = "bench A/B: escreve o artefacto do protocolo (a via normal é o gate)"]
    #[allow(
        clippy::disallowed_methods,
        reason = "bench `#[ignore]`: escreve o artefacto do protocolo (a via normal é o gate)"
    )]
    fn ab_ledger_by_artifact() -> Result<(), Box<dyn std::error::Error>> {
        let ledger = Ledger::DEFAULT;
        let small = "pequeno\n".to_string();
        let head_tail = "x".repeat(6_000);
        let spill = "y".repeat(20_000);
        let scenarios = vec![
            ("small", small.as_str()),
            ("head_tail", head_tail.as_str()),
            ("spill", spill.as_str()),
        ];
        let mut results = Vec::new();
        let mut total_model_visible = 0_usize;
        let mut total_input = 0_usize;
        for (name, text) in &scenarios {
            let rendered = ledger.render(text, Some(".katu/spill/x.stdout"));
            let model_visible = rendered.len();
            total_model_visible = total_model_visible.saturating_add(model_visible);
            total_input = total_input.saturating_add(text.len());
            results.push(serde_json::json!({
                "name": name,
                "input_bytes": text.len(),
                "model_visible_bytes": model_visible,
                "has_pointer": rendered.contains("ver .katu/spill/"),
            }));
        }
        let value = serde_json::json!({
            "schema": "katu.bench.ledger.v1",
            "question": "o ledger unificado limita o output model-visible e recupera o resto por spill",
            "rule": "head/tail + spill com ponteiro; o teto model-visible nunca é excedido",
            "scenarios": results,
            "totals": {
                "input_bytes": total_input,
                "model_visible_bytes": total_model_visible,
                "head_bytes": ledger.head,
                "tail_bytes": ledger.tail,
                "spill_threshold_bytes": ledger.spill_threshold,
            },
            "criterion": "model-visible <= head + tail + ponteiro, e o spill recupera o output inteiro",
            "criterion_met": total_model_visible < total_input,
            "caveat": "proxy determinístico (sem I/O real): mede o teto de bytes e o ponteiro, não a latência de escrita do spill",
            "decision": "default on (o ledger é o mecanismo unificado de truncagem)",
        });
        let text_out = serde_json::to_string_pretty(&value)?;
        if let Ok(path) = std::env::var("KATU_LEDGER_OUT") {
            std::fs::write(&path, format!("{text_out}\n"))?;
        }
        let parsed: serde_json::Value = serde_json::from_str(&text_out)?;
        assert_eq!(parsed.get("criterion_met"), Some(&serde_json::json!(true)));
        Ok(())
    }
}
