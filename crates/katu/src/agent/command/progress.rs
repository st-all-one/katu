//! Progresso ao vivo do turno para a borda CLI (`KERNEL_SURFACE` F2; E10-T05; `LIVE_FLOW` LF1/LF5).
//!
//! O kernel já publica o fluxo efémero (`Live`: raciocínio, tool calls, execução) para qualquer
//! superfície. O CLI é um cliente como a TUI, mas não tem painel; sem isto, `drive` descartava tudo
//! e o utilizador só via o resultado final. Aqui o fluxo é consumido conforme o [`Output`]:
//!
//! - `text`: linhas humanas em `stderr` (só quando `stderr` é um terminal; `stdout` fica só com os
//!   dados — regra do `report.rs`);
//! - `stream-json`: um objecto JSON por linha em `stdout` (JSONL), para consumidores programáticos;
//! - `json`: nada (só o envelope final).
//!
//! É a lição do goose ([`GOOSE_LOOP`](../../../wiki/_ref/brainstorm/goose-rs/GOOSE_LOOP.md) §8: a
//! interface é um consumidor do stream de eventos; `stream-json` é o formato de máquina).

use std::io::{self, IsTerminal, Write};

use katu_core::api::{Event, Live};
use serde_json::{Value, json};

use crate::report::Output;

/// Bloco de deltas em curso (separa raciocínio de resposta na saída humana).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Nenhum delta aberto (o próximo abre bloco).
    None,
    /// A fluir raciocínio.
    Thinking,
    /// A fluir texto do assistente.
    Text,
}

/// Consumidor do fluxo do turno na borda CLI.
pub(crate) struct Progress {
    output: Output,
    /// Só para `text`: escrever em `stderr` (detectado no arranque; V5).
    enabled: bool,
    mode: Mode,
}

impl Progress {
    /// Constrói o consumidor para o formato pedido.
    pub(crate) fn new(output: Output) -> Self {
        let _span = katu_core::trace_fn!("agent::command::progress::new");

        let enabled = output == Output::Text && io::stderr().is_terminal();
        Self {
            output,
            enabled,
            mode: Mode::None,
        }
    }

    /// Consome um evento, escrevendo o progresso se aplicável.
    ///
    /// Um erro de escrita (ex.: `EPIPE`) é **ignorado**: o progresso nunca falha o turno.
    pub(crate) fn show(&mut self, event: &Event) {
        let _span = katu_core::trace_fn!("agent::command::progress::show");

        match self.output {
            Output::Text => {
                if !self.enabled {
                    return;
                }
                if let Some(text) = self.render(event) {
                    write_stderr(&text);
                }
            }
            Output::StreamJson => {
                if let Some(value) = stream_line(event) {
                    let mut line = value.to_string();
                    line.push('\n');
                    write_stdout(&line);
                }
            }
            Output::Json => {}
        }
    }

    /// Traduz um evento em texto humano de progresso; `None` se não é progresso (puro, testável).
    pub(crate) fn render(&mut self, event: &Event) -> Option<String> {
        let _span = katu_core::trace_fn!("agent::command::progress::render");

        match event {
            Event::Live(Live::Thinking(delta)) => Some(self.delta(delta, Mode::Thinking)),
            Event::Live(Live::Text(delta)) => Some(self.delta(delta, Mode::Text)),
            Event::Live(Live::Tool { name, args }) => {
                self.mode = Mode::None;
                Some(format!("\n→ {name} {args}\n"))
            }
            Event::Live(Live::ToolDone { name, summary }) => {
                self.mode = Mode::None;
                Some(tool_done_line(name, summary))
            }
            Event::Live(Live::ToolOutput { name, chunk }) => {
                self.mode = Mode::None;
                tool_output_line(name, chunk)
            }
            Event::Live(Live::Refused { rule, evidence }) => {
                self.mode = Mode::None;
                Some(format!("\n⛔ {rule}: {evidence}\n"))
            }
            Event::Live(Live::Unavailable { control }) => {
                self.mode = Mode::None;
                Some(format!("\n⚠ falta {control}\n"))
            }
            Event::Info(text) | Event::Error(text) => {
                self.mode = Mode::None;
                Some(format!("\n{text}\n"))
            }
            Event::Phase(phase) => {
                self.mode = Mode::None;
                Some(format!("[{phase}]\n"))
            }
            Event::NextAction(action) => {
                self.mode = Mode::None;
                Some(format!("[próximo: {action}]\n"))
            }
            Event::Usage(text) => {
                self.mode = Mode::None;
                Some(format!("{text}\n"))
            }
            _ => None,
        }
    }

    /// Formata um delta, abrindo o bloco quando o modo muda (raciocínio vs resposta).
    fn delta(&mut self, delta: &str, mode: Mode) -> String {
        let _span = katu_core::trace_fn!("agent::command::progress::delta");

        if self.mode == mode {
            return delta.to_string();
        }
        self.mode = mode;
        match mode {
            Mode::Thinking => format!("\n· {delta}"),
            Mode::Text | Mode::None => format!("\n{delta}"),
        }
    }
}

/// Linha humana de uma tool concluída, com o resumo quando existe (LF4).
fn tool_done_line(name: &str, summary: &str) -> String {
    let _span = katu_core::trace_fn!("agent::command::progress::tool_done_line");

    if summary.is_empty() {
        format!("✓ {name}\n")
    } else {
        format!("✓ {name}: {summary}\n")
    }
}

/// Linha humana de um fragmento de output (`P1/PI_GAINS`): só a última linha não vazia.
fn tool_output_line(name: &str, chunk: &str) -> Option<String> {
    let _span = katu_core::trace_fn!("agent::command::progress::tool_output_line");

    let line = chunk.lines().rev().find(|line| !line.trim().is_empty())?;
    Some(format!("… {name}: {}\n", line.trim_end()))
}

/// Objecto JSONL de um evento para `--output stream-json`; `None` se não é progresso (LF5).
fn stream_line(event: &Event) -> Option<Value> {
    let _span = katu_core::trace_fn!("agent::command::progress::stream_line");

    let value = match event {
        Event::Live(Live::Thinking(delta)) => json!({ "type": "thinking", "delta": delta }),
        Event::Live(Live::Text(delta)) => json!({ "type": "text", "delta": delta }),
        Event::Live(Live::Tool { name, args }) => {
            json!({ "type": "tool", "name": name, "args": args })
        }
        Event::Live(Live::ToolDone { name, summary }) => {
            json!({ "type": "tool_done", "name": name, "summary": summary })
        }
        Event::Live(Live::ToolOutput { name, chunk }) => {
            json!({ "type": "tool_output", "name": name, "chunk": chunk })
        }
        Event::Live(Live::Refused { rule, evidence }) => {
            json!({ "type": "refused", "rule": rule, "evidence": evidence })
        }
        Event::Live(Live::Unavailable { control }) => {
            json!({ "type": "unavailable", "control": control })
        }
        Event::Info(text) => json!({ "type": "info", "text": text }),
        Event::Error(text) => json!({ "type": "error", "text": text }),
        Event::Phase(phase) => json!({ "type": "phase", "phase": phase }),
        Event::Assistant(text) => json!({ "type": "assistant", "text": text }),
        Event::Usage(text) => json!({ "type": "usage", "text": text }),
        Event::NextAction(action) => json!({ "type": "next_action", "action": action }),
        _ => return None,
    };
    Some(value)
}

/// Escreve em `stderr`, ignorando `EPIPE` (o consumidor fechou).
fn write_stderr(text: &str) {
    let _span = katu_core::trace_fn!("agent::command::progress::write_stderr");

    let stderr = io::stderr();
    let mut lock = stderr.lock();
    drop(lock.write_all(text.as_bytes()));
    drop(lock.flush());
}

/// Escreve em `stdout`, ignorando `EPIPE`.
fn write_stdout(text: &str) {
    let _span = katu_core::trace_fn!("agent::command::progress::write_stdout");

    let stdout = io::stdout();
    let mut lock = stdout.lock();
    drop(lock.write_all(text.as_bytes()));
    drop(lock.flush());
}

#[cfg(test)]
mod tests {
    use katu_core::api::{Event, Live};
    use serde_json::json;

    use super::{Progress, stream_line};
    use crate::report::Output;

    #[test]
    fn thinking_and_text_open_their_own_blocks() {
        let mut progress = Progress::new(Output::Text);
        assert_eq!(
            progress.render(&Event::Live(Live::Thinking("a".to_string()))),
            Some("\n· a".to_string())
        );
        assert_eq!(
            progress.render(&Event::Live(Live::Thinking("b".to_string()))),
            Some("b".to_string())
        );
        assert_eq!(
            progress.render(&Event::Live(Live::Text("c".to_string()))),
            Some("\nc".to_string())
        );
    }

    #[test]
    fn a_tool_output_shows_only_the_last_non_empty_line() {
        let mut progress = Progress::new(Output::Text);
        assert_eq!(
            progress.render(&Event::Live(Live::ToolOutput {
                name: "bash".to_string(),
                chunk: "um\ndois\n".to_string(),
            })),
            Some("… bash: dois\n".to_string())
        );
        assert_eq!(
            progress.render(&Event::Live(Live::ToolOutput {
                name: "bash".to_string(),
                chunk: "\n\n".to_string(),
            })),
            None,
            "sem linha não vazia, nada é mostrado"
        );
        assert_eq!(
            stream_line(&Event::Live(Live::ToolOutput {
                name: "bash".to_string(),
                chunk: "x".to_string(),
            })),
            Some(json!({ "type": "tool_output", "name": "bash", "chunk": "x" }))
        );
    }

    #[test]
    fn a_tool_call_and_result_are_their_own_lines() {
        let mut progress = Progress::new(Output::Text);
        assert_eq!(
            progress.render(&Event::Live(Live::Tool {
                name: "bash".to_string(),
                args: "{}".to_string(),
            })),
            Some("\n→ bash {}\n".to_string())
        );
        assert_eq!(
            progress.render(&Event::Live(Live::ToolDone {
                name: "bash".to_string(),
                summary: "ok".to_string(),
            })),
            Some("✓ bash: ok\n".to_string())
        );
    }

    #[test]
    fn the_final_answer_and_terminal_events_are_not_progress() {
        let mut progress = Progress::new(Output::Text);
        assert!(
            progress
                .render(&Event::Assistant("final".to_string()))
                .is_none()
        );
        assert!(progress.render(&Event::Done).is_none());
    }

    #[test]
    fn stream_json_emits_one_object_per_event() {
        assert_eq!(
            stream_line(&Event::Live(Live::Thinking("x".to_string()))),
            Some(json!({ "type": "thinking", "delta": "x" }))
        );
        assert_eq!(
            stream_line(&Event::Live(Live::ToolDone {
                name: "read".to_string(),
                summary: "1 linha".to_string(),
            })),
            Some(json!({ "type": "tool_done", "name": "read", "summary": "1 linha" }))
        );
        assert!(stream_line(&Event::Done).is_none());
    }
}
