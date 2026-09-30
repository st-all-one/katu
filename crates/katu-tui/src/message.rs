//! Mensagens entre a UI e a borda (E10): comandos (UI → borda) e atualizações (borda → UI).
//!
//! O render nunca faz I/O: a UI emite [`Command`]s e a borda devolve [`Update`]s (E10-T02).

use katu_core::provider::Thinking;

use crate::live::Live;
use crate::trash::TrashEntry;

/// Comando emitido pela UI para a borda executar (I/O fora do render).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Submeter uma mensagem ao agente.
    Submit(String),
    /// Altera o modelo do **próximo** turno (E10-T07/E12-T10); o agente não se auto-escala.
    SetModel(String),
    /// Altera o grau de pensamento do **próximo** turno (E10-T07/E12-T10).
    SetThinking(Thinking),
    /// Pede a lista da lixeira (E10-T07/E06-T09).
    Trash,
    /// Restaura um item da lixeira pelo token guardado.
    Restore(String),
    /// Pré-visualiza a compactação do histórico (E09-T07/E10-T07).
    Compact,
    /// Corre o gate de verificação e pede override se bloquear (E09-T03).
    Verify,
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
    /// Lista da lixeira publicada pela borda (E10-T07/E06-T09).
    Trash(Vec<TrashEntry>),
    /// Turno concluído (limpa a pendência).
    Done,
}
