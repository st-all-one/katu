//! Mensagens entre a UI e a borda (E10): comandos (UI → borda) e atualizações (borda → UI).
//!
//! O render nunca faz I/O: a UI emite [`Command`]s e a borda devolve [`Update`]s (E10-T02).

use katu_core::provider::Thinking;

use crate::live::Live;
use crate::trash::TrashEntry;

/// Pedido de login emitido pela TUI (a borda escreve config/credenciais).
///
/// O embedding fica no `katu.toml` e **não** é tocado: o pedido só leva o provider/modelo/base do
/// agente e, no opencode, a chave.
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// Comando emitido pela UI para a borda executar (I/O fora do render).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Submeter uma mensagem ao agente.
    Submit(String),
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
    /// Sair.
    Quit,
}

/// Atualização injetada pela borda na UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Update {
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
}
