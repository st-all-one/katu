//! Envelope de máquina e escrita em `stdout`/`stderr` (E01-T04).
//!
//! Regra fixa: **stdout = dados, stderr = logs**. O envelope `--json` é a superfície estável de
//! máquina; um *pipe* fechado (`katu … | head`) é **sucesso**, não erro.

use std::io::{self, Write};

use katu_core::error::{Error, ErrorKind};
use serde::Serialize;
use serde_json::Value;

/// Resultado de um comando, pronto a serializar.
#[derive(Debug, Serialize)]
pub(crate) struct Report {
    /// Se o comando teve sucesso.
    pub(crate) success: bool,
    /// Nome estável do comando.
    pub(crate) command: &'static str,
    /// Corpo do erro, quando há falha.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<ErrorBody>,
    /// Dados do comando, quando há sucesso.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) data: Option<Value>,
    /// Código de saída associado (não serializado).
    #[serde(skip)]
    pub(crate) exit: u8,
}

/// Corpo de erro estável.
#[derive(Debug, Serialize)]
pub(crate) struct ErrorBody {
    /// Categoria estável (`not_found`, `io`, …).
    pub(crate) kind: &'static str,
    /// Mensagem legível.
    pub(crate) message: String,
}

impl Report {
    /// Relatório de sucesso.
    pub(crate) fn ok(command: &'static str, data: Option<Value>) -> Self {
        let _span = katu_core::trace_fn!("report::ok");

        Self {
            success: true,
            command,
            error: None,
            data,
            exit: 0,
        }
    }

    /// Relatório de falha (código de saída derivado da categoria).
    pub(crate) fn failed(command: &'static str, error: &Error) -> Self {
        let _span = katu_core::trace_fn!("report::failed");

        Self {
            success: false,
            command,
            error: Some(ErrorBody {
                kind: error.kind().as_str(),
                message: error.to_string(),
            }),
            data: None,
            exit: error.kind().exit_code(),
        }
    }

    /// Relatório de falha a partir de partes já estruturadas (reconstrução na fronteira do kernel).
    pub(crate) fn failed_parts(command: &'static str, kind: ErrorKind, message: &str) -> Self {
        let _span = katu_core::trace_fn!("report::failed_parts");

        Self {
            success: false,
            command,
            error: Some(ErrorBody {
                kind: kind.as_str(),
                message: message.to_string(),
            }),
            data: None,
            exit: kind.exit_code(),
        }
    }
}

/// Formato de saída do comando (`LIVE_FLOW` LF5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Output {
    /// Texto humano (default): progresso em `stderr`, dados em `stdout`.
    #[default]
    Text,
    /// Envelope JSON único em `stdout` (o `--json`).
    Json,
    /// Stream JSONL: um evento por linha em `stdout`, seguido do envelope (`--output stream-json`).
    StreamJson,
}

impl Output {
    /// Interpreta `--output` (recusa valores desconhecidos).
    ///
    /// # Errors
    /// [`Error::invalid_input`] se o formato não for `text`/`json`/`stream-json`.
    pub(crate) fn parse(raw: &str) -> Result<Self, Error> {
        let _span = katu_core::trace_fn!("report::output_parse");

        match raw {
            "text" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            "stream-json" => Ok(Self::StreamJson),
            other => Err(Error::invalid_input(format!(
                "output inválido: `{other}` (text/json/stream-json)"
            ))),
        }
    }
}

/// Emite o relatório e devolve o código de saída do processo.
#[allow(
    clippy::fn_params_excessive_bools,
    reason = "o flag `--json` é um booleano explícito da borda"
)]
pub(crate) fn emit(report: &Report, json: bool) -> u8 {
    let _span = katu_core::trace_fn!("report::emit");

    let written = if json {
        emit_json(report)
    } else {
        emit_human(report)
    };
    match written {
        Ok(()) => report.exit,
        Err(error) => error.kind().exit_code(),
    }
}

/// Envelope JSON numa linha, em `stdout`.
fn emit_json(report: &Report) -> Result<(), Error> {
    let _span = katu_core::trace_fn!("report::emit_json");

    let mut bytes = serde_json::to_vec(report)
        .map_err(|err| Error::internal(format!("serializando envelope: {err}")))?;
    bytes.push(b'\n');
    write_stdout(&bytes)
}

/// Saída humana: erro em `stderr`, dados em `stdout`.
fn emit_human(report: &Report) -> Result<(), Error> {
    let _span = katu_core::trace_fn!("report::emit_human");

    if let Some(error) = &report.error {
        let mut line = error.message.clone();
        line.push('\n');
        return write_stderr(line.as_bytes());
    }
    write_stdout(human_data(report.data.as_ref()).as_bytes())
}

/// Renderiza os dados em texto humano (strings, objetos chave-valor ou JSON compacto).
///
/// O campo `session` (quando presente) é impresso **em destaque**, após uma linha em branco — é o
/// id que o `run` devolve para `--resume`.
fn human_data(data: Option<&Value>) -> String {
    let _span = katu_core::trace_fn!("report::human_data");

    match data {
        Some(Value::String(text)) => format!("{text}\n"),
        Some(Value::Object(map)) => {
            let mut out = String::new();
            // A resposta do modelo é o **corpo**: sai crua, antes dos metadados (que a repetiriam
            // como `text = "…"`). `run` sem `--json` passa a devolver o que o agente escreveu.
            if let Some(Value::String(text)) = map.get("text")
                && !text.is_empty()
            {
                out.push_str(text);
                out.push('\n');
                out.push('\n');
            }
            for (key, value) in map {
                if key == "session" || key == "text" {
                    continue;
                }
                out.push_str(key);
                out.push_str(" = ");
                out.push_str(&value.to_string());
                out.push('\n');
            }
            if let Some(Value::String(session)) = map.get("session") {
                out.push('\n');
                out.push_str(session);
                out.push('\n');
            }
            out
        }
        Some(value) => format!("{value}\n"),
        None => String::new(),
    }
}

/// Escreve em `stdout`.
fn write_stdout(bytes: &[u8]) -> Result<(), Error> {
    let _span = katu_core::trace_fn!("report::write_stdout");

    let stdout = io::stdout();
    write_line(&mut stdout.lock(), bytes)
}

/// Escreve em `stderr`.
fn write_stderr(bytes: &[u8]) -> Result<(), Error> {
    let _span = katu_core::trace_fn!("report::write_stderr");

    let stderr = io::stderr();
    write_line(&mut stderr.lock(), bytes)
}

/// Escreve todos os bytes; `BrokenPipe` é sucesso (o consumidor fechou o *pipe*).
fn write_line<W: Write>(writer: &mut W, bytes: &[u8]) -> Result<(), Error> {
    let _span = katu_core::trace_fn!("report::write_line");

    match writer.write_all(bytes) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(err) => Err(Error::io("<stream>", err)),
    }
}

/// Só para testar o caminho de escrita.
#[cfg(test)]
mod tests {
    use super::{Report, human_data, write_line};
    use katu_core::error::Error;
    use std::io::{self, Write};

    struct BrokenPipe;

    impl Write for BrokenPipe {
        fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn epipe_is_success() -> Result<(), Error> {
        write_line(&mut BrokenPipe, b"x")?;
        Ok(())
    }

    #[test]
    fn human_renders_string_and_object() {
        let text = human_data(Some(&serde_json::json!("olá")));
        assert_eq!(text, "olá\n");
        let object = human_data(Some(&serde_json::json!({"a": 1})));
        assert!(object.contains("a = 1"));
    }

    #[test]
    fn human_prints_the_run_text_as_the_body() {
        let data = serde_json::json!({
            "session": "s_1",
            "text": "resumo do repo",
            "chars": 14,
        });
        let out = human_data(Some(&data));
        assert!(out.starts_with("resumo do repo\n\n"), "{out}");
        assert!(out.contains("chars = 14"), "{out}");
        assert!(out.contains("s_1"), "{out}");
        assert!(
            !out.contains("text = "),
            "o texto não se repete como campo: {out}"
        );
    }

    #[test]
    fn ok_report_serializes_success() -> Result<(), serde_json::Error> {
        let report = Report::ok("version", Some(serde_json::json!({"katu": "0.1.0"})));
        let text = serde_json::to_string(&report)?;
        assert!(text.contains("\"success\":true"));
        assert!(text.contains("\"command\":\"version\""));
        assert!(!text.contains("exit"));
        Ok(())
    }
}
