//! Eventos do kernel para a superfície (`KERNEL_SURFACE` §2.2).
//!
//! Todo o comando de trabalho emite **pelo menos** um evento terminal ([`Event::Done`],
//! [`Event::Error`] ou [`Event::Cancelled`]): a superfície nunca fica pendurada (K4).

use serde::{Deserialize, Serialize};

use crate::provider::Thinking;

use super::{Live, TrashEntry};

/// Pedido de aprovação humana (challenge-and-response, §33).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalRequest {
    /// Nome ao modelo da tool.
    pub tool: String,
    /// Regra que exige a aprovação.
    pub rule: String,
    /// Âmbito concreto (caminho/host/comando).
    pub scope: String,
}

/// Concessão assinada de uma aprovação humana (resposta a [`ApprovalRequest`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalGrant {
    /// Justificação (`override_reason`).
    pub reason: String,
    /// Quem assinou (`granted_by`).
    pub granted_by: String,
}

/// Evento injetado pela superfície no estado central da UI (E10-T02).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    /// Resposta (texto final) do modelo.
    Assistant(String),
    /// Uma tool correu.
    Tool(String),
    /// Nota informativa.
    Info(String),
    /// Erro do turno.
    Error(String),
    /// Fase corrente do kernel (E10-T06).
    Phase(String),
    /// Observação efémera do turno (E10-T05): só o painel de atividade.
    Live(Live),
    /// Modelos disponíveis no provider (E10-T07); o primeiro é o default.
    Models(Vec<String>),
    /// Graus de pensamento suportados pelo modelo ativo (E20-T10); o menu só oferece estes.
    ThinkingOptions(Vec<Thinking>),
    /// Próxima ação declarada no checkpoint de fase (E10-T06).
    NextAction(String),
    /// Uso/custo do último turno (E12-T03/T10), já formatado pela borda.
    Usage(String),
    /// Lista da lixeira publicada pela borda (E10-T07/E06-T09).
    Trash(Vec<TrashEntry>),
    /// Linhas da transcrição durável, lidas do ficheiro pela borda (E10-T05).
    Transcript(Vec<String>),
    /// Turno cancelado pelo utilizador (cancelamento cooperativo).
    Cancelled,
    /// Estado do modo de planeamento confirmado pela borda (E20-T11).
    Plan(bool),
    /// Turno concluído (limpa a pendência).
    Done,
    /// O kernel precisa de uma decisão humana (a superfície responde com
    /// [`Command::Approval`](super::Command::Approval)).
    ApprovalRequest(ApprovalRequest),
    /// O kernel está ocupado com um turno e recusou um comando que muta o estado.
    Busy(String),
}
