//! Deteção determinística de eco do delta de uma tool na resposta final (L-Q4).
//!
//! Um modelo fraco devolve o `ToolResult.delta` (TOON/JSON cru) como resposta final. Compara-se o
//! texto do passo com os `delta` recentes do turno com uma normalização determinística
//! (`trim` + colapso de whitespace) e igualdade/prefixo longo. Um eco dispara um *nudge* e um
//! retry (1×) — nunca é aceite como resposta.
//!
//! É **conservador**: uma resposta legítima que cite um excerto (sem o repetir como prefixo) não
//! dispara. Não substitui a decodificação de `declared` (tool calls declaradas como texto).

use katu_core::diag::{Level, events};
use katu_core::kernel::Message;

use crate::agent::AgentError;
use crate::runtime::Runtime;

/// Comprimento mínimo (caracteres) de um delta/texto para poder ser eco.
///
/// Abaixo disto a comparação seria ruído (respostas curtas partilham prefixos triviais).
const MIN_ECHO_CHARS: usize = 24;

/// `true` se `text` ecoa um dos `deltas` recentes (normalizados).
pub(super) fn echoes_recent_delta(runtime: &Runtime<'_>, text: &str) -> Result<bool, AgentError> {
    let _span = katu_core::trace_fn!("agent::turn::echo::echoes_recent_delta");

    let messages = runtime.session.messages()?;
    let deltas: Vec<&str> = messages
        .iter()
        .filter_map(|message| match message {
            Message::ToolResult { delta, .. } => delta.as_deref(),
            _ => None,
        })
        .collect();
    Ok(is_echo(text, &deltas))
}

/// `true` se `text` é igual a um delta ou partilha com ele um prefixo longo (em qualquer sentido).
fn is_echo(text: &str, deltas: &[&str]) -> bool {
    let _span = katu_core::fn_span!(
        Level::Debug,
        events::AGENT_ECHO,
        "agent::turn::echo::is_echo",
        "echo" => false
    );
    let needle = normalize(text);
    if needle.chars().count() < MIN_ECHO_CHARS {
        return false;
    }
    deltas.iter().any(|delta| {
        let hay = normalize(delta);
        hay.chars().count() >= MIN_ECHO_CHARS
            && (needle == hay
                || needle.starts_with(&hay)
                || (hay.len() > needle.len() && hay.starts_with(&needle)))
    })
}

/// Normaliza para comparação: `trim` + colapso de whitespace em espaços simples.
fn normalize(text: &str) -> String {
    let _span = katu_core::trace_fn!("agent::turn::echo::normalize");

    let mut out = String::with_capacity(text.len());
    let mut last_space = false;
    for character in text.trim().chars() {
        if character.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(character);
            last_space = false;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{is_echo, normalize};

    const DELTA: &str = "{\"kind\":\"read\",\"path\":\"src/main.rs\",\"bytes\":1234,\"lines\":42}";

    #[test]
    fn an_echo_of_the_delta_is_detected() {
        assert!(is_echo(DELTA, &[DELTA]));
        // Whitespace de embrulho (indentação) continua a ser o mesmo conteúdo.
        let spaced = format!("\n  {DELTA}\t");
        assert!(is_echo(&spaced, &[DELTA]));
    }

    #[test]
    fn a_legitimate_answer_that_quotes_an_excerpt_does_not_fire() {
        let answer = "Li o ficheiro. O conteúdo começa por `fn main()` e o módulo tem 42 linhas.";
        assert!(!is_echo(answer, &[DELTA]));
    }

    #[test]
    fn short_texts_never_match() {
        assert!(!is_echo("ok", &["ok"]));
    }

    #[test]
    fn a_long_prefix_in_either_direction_is_an_echo() {
        let longer = format!("{DELTA} e mais texto que o modelo colou");
        assert!(is_echo(&longer, &[DELTA]));
        assert!(is_echo(DELTA, &[&longer]));
    }

    #[test]
    fn normalize_collapses_whitespace() {
        assert_eq!(normalize("  a \n b\tc "), "a b c");
    }
}
