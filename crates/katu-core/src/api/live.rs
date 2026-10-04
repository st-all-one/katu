//! Eventos **efémeros** do painel de atividade (E10-T05/T04), transportados no protocolo.
//!
//! Nunca são escritos no log de sessão nem enviados ao modelo: servem só para o utilizador ver o
//! progresso (deltas, tools) e as recusas de política enquanto o turno corre.

use serde::{Deserialize, Serialize};

/// Evento efémero do painel de atividade.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Live {
    /// Delta de texto do modelo (acumula no corpo do painel).
    Text(String),
    /// Delta de raciocínio (guardado, fora do ecrã por omissão).
    Thinking(String),
    /// Tool pedida pelo modelo, com os argumentos **crus** (transparência).
    Tool {
        /// Nome ao modelo da tool.
        name: String,
        /// Argumentos crus enviados pelo modelo.
        args: String,
    },
    /// Tool concluída (sucesso/parcial), com um **resumo** do resultado (`LIVE_FLOW` LF4).
    ToolDone {
        /// Nome ao modelo da tool.
        name: String,
        /// Resumo de uma linha do resultado (vazio quando não há envelope).
        summary: String,
    },
    /// Fragmento de output de uma tool longa (`P1/PI_GAINS`): sinal de vida efémero.
    ToolOutput {
        /// Nome ao modelo da tool.
        name: String,
        /// Fragmento de output (texto *lossy*).
        chunk: String,
    },
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
