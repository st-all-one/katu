//! Utilitários do painel de atividade **efémero** (E10-T05/T04).
//!
//! Os eventos efémeros vivem no protocolo ([`katu_core::api::Live`]); aqui fica só o recorte do
//! buffer de streaming, que é uma preocupação de apresentação (nunca toca no log nem no transcript).

use katu_core::diag::{Level, events};

/// Mantém apenas a **cauda** de `buffer` (até `max` bytes), em fronteira de caractere (E10-T03).
///
/// O painel de atividade é efémero: interessa o que está a chegar agora, não o histórico todo.
pub(crate) fn trim_tail(buffer: &mut String, max: usize) {
    let _span = katu_core::fn_span!(Level::Trace, events::TUI_LIVE, "live::trim_tail");
    if buffer.len() <= max {
        return;
    }
    let cut = buffer.len().saturating_sub(max);
    let start = buffer
        .char_indices()
        .map(|(index, _)| index)
        .find(|&index| index >= cut)
        .unwrap_or(buffer.len());
    buffer.replace_range(..start, "");
}

#[cfg(test)]
mod tests {
    use super::trim_tail;

    #[test]
    fn trim_tail_keeps_the_end_on_a_char_boundary() {
        let mut buffer = "ábcdef".to_string();
        trim_tail(&mut buffer, 4);
        assert_eq!(buffer, "cdef", "corta em fronteira, não a meio de 'á'");
    }

    #[test]
    fn trim_tail_is_a_no_op_below_the_cap() {
        let mut buffer = "curto".to_string();
        trim_tail(&mut buffer, 64);
        assert_eq!(buffer, "curto");
    }
}
