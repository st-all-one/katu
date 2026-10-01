//! Transcrição durável (E10-T05): projeção **pura** do log para linhas legíveis.
//!
//! O durável é o próprio log (`Session`); esta projeção é reconstruível a qualquer momento. A borda
//! escreve-a em `.katu/transcript.md` e serve a **vista read-only** da TUI a partir do ficheiro —
//! nunca do painel efémero.

use katu_core::error::ToolOutcome;
use katu_core::kernel::Message;
use katu_policy::{ResolvedPath, ToolUse};

use super::{Runtime, RuntimeError};

impl Runtime<'_> {
    /// Projeta o log durável numa transcrição legível (E10-T05).
    ///
    /// # Errors
    /// [`RuntimeError::Session`] se o log não puder ser lido.
    pub(crate) fn transcript(&self) -> Result<Vec<String>, RuntimeError> {
        let _span = katu_core::trace_fn!("runtime::transcript::transcript");

        Ok(render(&self.session.messages()?))
    }
}

/// Renderiza as mensagens visíveis ao modelo numa lista de linhas (determinístico).
fn render(messages: &[Message]) -> Vec<String> {
    let _span = katu_core::trace_fn!("runtime::transcript::render");

    let mut lines = vec!["# katu — transcrição durável".to_string(), String::new()];
    for message in messages {
        match message {
            Message::User { text } => {
                lines.push("**utilizador**".to_string());
                lines.push(text.clone());
            }
            Message::Assistant { text } => {
                lines.push("**assistente**".to_string());
                lines.push(text.clone());
            }
            Message::ToolCall { call, tool } => {
                lines.push(format!(
                    "**tool `{}`** ({})",
                    tool.name.as_str(),
                    call.as_str()
                ));
                lines.push(tool_summary(tool));
            }
            Message::ToolResult {
                call,
                outcome,
                delta,
            } => {
                lines.push(format!("**resultado** ({})", call.as_str()));
                lines.push(outcome_label(outcome));
                if let Some(delta) = delta {
                    lines.push(delta.clone());
                }
            }
            _ => {}
        }
        lines.push(String::new());
    }
    lines
}

/// Resumo de uma tool call: `argv` quando há, senão caminhos resolvidos, senão o `cwd`.
fn tool_summary(tool: &ToolUse) -> String {
    let _span = katu_core::trace_fn!("runtime::transcript::tool_summary");

    if let Some(argv) = &tool.argv {
        return format!("`{}`", argv.as_slice().join(" "));
    }
    if !tool.resolved_paths.is_empty() {
        let paths: Vec<&str> = tool
            .resolved_paths
            .iter()
            .map(ResolvedPath::as_str)
            .collect();
        return format!("`{}`", paths.join(", "));
    }
    format!("`{}`", tool.cwd.as_str())
}

/// Rótulo curto do efeito, com a regra quando a recusa é acionável (DF10).
fn outcome_label(outcome: &ToolOutcome) -> String {
    let _span = katu_core::trace_fn!("runtime::transcript::outcome_label");

    match outcome {
        ToolOutcome::Ok => "ok".to_string(),
        ToolOutcome::Partial => "parcial".to_string(),
        ToolOutcome::Denied { rule_id, .. } => format!("negado ({})", rule_id.as_str()),
        ToolOutcome::Timeout => "tempo esgotado".to_string(),
        ToolOutcome::Unavailable { control, .. } => {
            format!("indisponível (falta {})", control.as_str())
        }
        _ => "desconhecido".to_string(),
    }
}
