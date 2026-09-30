//! Parser SSE **incremental** (E12-T06): nunca bufferiza a resposta inteira.
//!
//! Consome `text/event-stream` linha a linha, dispatchando cada evento de `data` completo ao
//! callback. Suporta `\r\n`, comentários (`:`) e múltiplas linhas `data:` por evento. O estado
//! incompleto fica em `pending` — o fragmento seguinte continua exatamente onde parou.

use katu_core::provider::Flow;

/// Serviço de um `data:` completo.
enum Line {
    /// Linha vazia: evento pronto.
    Dispatch,
    /// Linha sem interesse (comentário, outro campo).
    Ignore,
}

/// Parser incremental de Server-Sent Events.
#[derive(Debug, Default)]
pub struct SseParser {
    pending: Vec<u8>,
    data: String,
}

impl SseParser {
    /// Cria um parser vazio.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Alimenta um fragmento; chama `on_data` por cada evento de dados completo.
    ///
    /// Devolve [`Flow::Break`] se `on_data` o devolver (cancelamento propagado).
    pub fn push(&mut self, chunk: &[u8], on_data: &mut dyn FnMut(&str) -> Flow) -> Flow {
        self.pending.extend_from_slice(chunk);
        let mut consumed = 0_usize;
        let mut flow = Flow::Continue;
        while let Some(offset) = self
            .pending
            .get(consumed..)
            .and_then(|rest| rest.iter().position(|byte| *byte == b'\n'))
        {
            let end = consumed.saturating_add(offset);
            let line = strip_cr(self.pending.get(consumed..end).unwrap_or(&[]));
            if matches!(classify(line), Line::Dispatch) {
                let payload = self
                    .data
                    .strip_suffix('\n')
                    .unwrap_or(&self.data)
                    .to_string();
                if !payload.is_empty() {
                    flow = on_data(&payload);
                    self.data.clear();
                    if matches!(flow, Flow::Break) {
                        consumed = end.saturating_add(1);
                        break;
                    }
                }
            } else if let Some(value) = data_value(line) {
                self.data.push_str(&value);
                self.data.push('\n');
            }
            consumed = end.saturating_add(1);
        }
        let keep = consumed.min(self.pending.len());
        self.pending.drain(..keep);
        flow
    }
}

/// Classifica a linha (já sem `\r`).
fn classify(line: &[u8]) -> Line {
    if line.is_empty() {
        Line::Dispatch
    } else {
        Line::Ignore
    }
}

/// Extrai o valor de uma linha `data:` (remove um espaço inicial).
fn data_value(line: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(line).ok()?;
    let value = text.strip_prefix("data:")?;
    let value = value.strip_prefix(' ').unwrap_or(value);
    Some(value.to_string())
}

/// Remove o `\r` final, se existir.
fn strip_cr(line: &[u8]) -> &[u8] {
    match line.last() {
        Some(b'\r') => line.get(..line.len().saturating_sub(1)).unwrap_or(&[]),
        _ => line,
    }
}

#[cfg(test)]
mod tests;
