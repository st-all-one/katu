//! Comandos da superfície para o kernel (`KERNEL_SURFACE` §2.1).

use serde::{Deserialize, Serialize};

use crate::provider::Thinking;

use super::ApprovalGrant;

/// Pedido de login emitido pela superfície (a borda escreve config/credenciais).
///
/// O embedding fica no `katu.toml` e **não** é tocado: o pedido só leva o provider/modelo/base do
/// agente e, no opencode, a chave.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoginRequest {
    /// Provider: `opencode`, `opencode-zen` ou `llama`.
    pub provider: String,
    /// Chave da API do opencode; `None` mantém a guardada.
    pub api_key: Option<String>,
    /// URL base; `None` usa o default do provider.
    pub base: Option<String>,
    /// Modelo; `None` usa o default do provider.
    pub model: Option<String>,
    /// `true` termina a sessão (apaga a chave guardada).
    pub logout: bool,
}

/// Comando emitido pela superfície para o kernel executar (K4: todo o comando de trabalho tem um
/// evento terminal).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    /// Submeter uma mensagem ao agente (abre um turno).
    Submit(String),
    /// Pede o cancelamento cooperativo do turno (K8); aceite mesmo com o kernel ocupado.
    Cancel,
    /// Prompt de *steering* aplicado no passo seguinte do turno (E20-T16).
    Steer(String),
    /// Resposta a um [`Event::ApprovalRequest`](super::Event::ApprovalRequest) pendente.
    ///
    /// `None` = recusado/cancelado (fail-closed, §33); `Some` = concessão assinada.
    Approval(Option<ApprovalGrant>),
    /// Faz login num provider de agente (opencode **ou** llama.cpp).
    Login(LoginRequest),
    /// Altera o modelo do **próximo** turno (E10-T07/E12-T10); o agente não se auto-escala.
    SetModel(String),
    /// Altera o grau de pensamento do **próximo** turno (E10-T07/E12-T10).
    SetThinking(Thinking),
    /// Pede a lista da lixeira (E10-T07/E06-T09).
    Trash,
    /// Pede a transcrição durável para a vista read-only (E10-T05).
    Transcript,
    /// Restaura um item da lixeira pelo token guardado.
    Restore(String),
    /// Esvazia a lixeira (destrutivo; a borda pede challenge, E10-T07).
    EmptyTrash,
    /// Pré-visualiza a compactação do histórico (E09-T07/E10-T07).
    Compact,
    /// Corre o gate de verificação e pede override se bloquear (E09-T03).
    Verify,
    /// Liga/desliga o modo de planeamento (E20-T11).
    Plan,
    /// Executa um comando shell pela política/contenção (E20-T12).
    Shell(String),
    /// Força o carregamento de uma skill pelo nome (E20-T13).
    Skill(String),
    /// Encerra a thread do kernel (aceite mesmo com o kernel ocupado).
    Shutdown,
}
