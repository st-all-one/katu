//! Eventos **efémeros** do painel de atividade (E10-T05/T04).
//!
//! Nunca são escritos no log de sessão nem enviados ao modelo: servem só para o utilizador ver o
//! progresso (deltas, tools) e as recusas de política enquanto o turno corre.

/// Evento efémero do painel de atividade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Live {
    /// Delta de texto do modelo (acumula no corpo do painel).
    Text(String),
    /// Delta de raciocínio (guardado, fora do ecrã por omissão).
    Thinking(String),
    /// Tool pedida pelo modelo.
    Tool(String),
    /// Tool concluída (sucesso/parcial).
    ToolDone(String),
    /// **Recusa de política** com a regra e a evidência (E10-T04); fica no painel e no transcript.
    Refused {
        /// Regra que negou.
        rule: String,
        /// Evidência (argumento concreto).
        evidence: String,
    },
    /// Tool indisponível por falta de um controlo (ex.: aprovação, E10-T04).
    Unavailable {
        /// Controlo em falta.
        control: String,
    },
    /// Limpa o painel (fim de turno).
    Clear,
}

/// Mantém apenas a **cauda** de `buffer` (até `max` bytes), em fronteira de caractere (E10-T03).
///
/// O painel de atividade é efémero: interessa o que está a chegar agora, não o histórico todo.
pub(crate) fn trim_tail(buffer: &mut String, max: usize) {
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
