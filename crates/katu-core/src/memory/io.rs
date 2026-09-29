//! Pedidos e resultados da porta `Memory`. Tipos do katu; nada do knudge.

use super::types::{Anchor, Basis, NoteRef, NoteType, Score};

/// Pedido de pré-validação de escrita.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreWriteReq {
    /// Afirmação (uma por nota).
    pub statement: String,
    /// Espécie da nota.
    pub note_type: NoteType,
    /// Âncora de código, quando aplicável.
    pub anchor: Option<Anchor>,
    /// Corpo detalhado.
    pub body: String,
}

/// Decisão do `pre_write` (dedup).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PreWriteOutcome {
    /// Criar uma nota nova.
    Create,
    /// Fundir numa nota existente.
    Merge {
        /// Nota alvo.
        target: NoteRef,
        /// Similaridade.
        score: Score,
        /// Base do score.
        basis: Basis,
    },
    /// Rejeitar: duplicata forte (≥ 0.92).
    Reject {
        /// Nota duplicada.
        duplicate: NoteRef,
        /// Similaridade.
        score: Score,
        /// Base do score.
        basis: Basis,
    },
}

/// Pedido de pré-validação de edição.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreEditReq {
    /// Nota alvo.
    pub note: NoteRef,
    /// Nova afirmação.
    pub statement: String,
    /// Âncora de código, quando aplicável.
    pub anchor: Option<Anchor>,
}

/// Decisão do `pre_edit`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PreEditOutcome {
    /// Revisão no lugar.
    Update,
    /// Substituição (novo id + `superseded_by`).
    Supersede {
        /// Nota substituta.
        target: NoteRef,
    },
    /// Rejeitar com motivo.
    Reject {
        /// Motivo legível.
        reason: String,
    },
}

/// Pedido de consulta (recall) à memória.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecallReq {
    /// Consulta em linguagem natural (uma frase).
    pub query: String,
    /// Número máximo de resultados a devolver.
    pub limit: usize,
}

/// Resultado de uma consulta: nota + afirmação + score.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecallHit {
    /// Referência à nota.
    pub note: NoteRef,
    /// Afirmação.
    pub statement: String,
    /// Similaridade (pontos base).
    pub score: Score,
    /// Base do score (medido/inferido).
    pub basis: Basis,
    /// Âncora de código, quando aplicável.
    pub anchor: Option<Anchor>,
}

/// Pedido de fim de sessão.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionEndReq {
    /// Tarefa a fechar, quando há.
    pub task: Option<NoteRef>,
    /// Ator (agente/utilizador).
    pub actor: String,
}

/// Resultado do fim de sessão.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionEndOutcome {
    /// Notas efetivamente cometidas.
    pub committed: Vec<NoteRef>,
    /// Avisos *soft* promovidos (nunca silenciados).
    pub warnings: Vec<String>,
}

/// Saúde do backend, como valor (sem `bool` solto).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Health {
    /// Saudável.
    #[default]
    Healthy,
    /// Degradado (a funcionar, com avisos).
    Degraded,
}

/// Estado do backend de memória.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryStatus {
    /// Nome do backend (ex.: `knudge-in-process`).
    pub backend: String,
    /// Saúde.
    pub health: Health,
    /// Avisos.
    pub warnings: Vec<String>,
}
