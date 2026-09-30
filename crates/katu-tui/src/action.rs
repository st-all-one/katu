//! Keymap **puro** (E10-T02/E20-T10): `map_key(KeyEvent, Mode) -> Option<Action>`.
//!
//! Nenhuma lógica de UI no handler de eventos: o handler só traduz a tecla numa [`Action`] e
//! delega em [`App::apply_action`](crate::App::apply_action). Testável por modo, sem terminal.
//!
//! Na TUI v2 os comandos vivem na linha de mensagem (`/model`, `/thinking`, …) e a ajuda é uma
//! sobreposição (`?`); os antigos atalhos de um toque foram **removidos**.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Modo de entrada da UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Navegação: `q` sai, `i`/Enter escreve, `/` inicia um comando, `?` abre a ajuda.
    #[default]
    Normal,
    /// Edição da linha de mensagem (mensagem ou comando `/`).
    Insert,
    /// Confirmação de uma ação destrutiva (`y`/`n`).
    Confirm,
    /// Navegação na vista da lixeira (E10-T07).
    Trash,
    /// Leitura da transcrição durável (E10-T05).
    Transcript,
    /// Sobreposição de ajuda (`?`, E20-T10).
    Help,
    /// Mini-menu de seleção (`/model`, `/thinking`, E20-T10).
    Menu,
}

/// Ação pura produzida pelo teclado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Entra no modo de edição.
    EnterInsert,
    /// Inicia um comando (`/`): entra em edição com o prefixo.
    StartCommand,
    /// Volta ao modo de navegação.
    LeaveInsert,
    /// Insere um caractere na mensagem.
    Insert(char),
    /// Apaga o último caractere da mensagem.
    Backspace,
    /// Submete a mensagem/comando (efeito na borda).
    Submit,
    /// Cancela a edição/confirmação/sobreposição.
    Cancel,
    /// Confirma a ação pendente.
    Confirm,
    /// Rola a conversa para cima (mais antigo).
    ScrollUp,
    /// Rola a conversa para baixo (mais recente).
    ScrollDown,
    /// Abre a sobreposição de ajuda (`?`, E20-T10).
    OpenHelp,
    /// Seleciona a escolha anterior do menu.
    MenuUp,
    /// Seleciona a escolha seguinte do menu.
    MenuDown,
    /// Confirma a escolha do menu.
    MenuConfirm,
    /// Abre a vista da lixeira (E10-T07).
    OpenTrash,
    /// Abre a vista read-only da transcrição durável (E10-T05).
    OpenTranscript,
    /// Fecha a sobreposição corrente (lixeira/transcrição/ajuda/menu).
    CloseOverlay,
    /// Seleciona a entrada anterior da lixeira.
    TrashUp,
    /// Seleciona a entrada seguinte da lixeira.
    TrashDown,
    /// Rola a transcrição para cima (E10-T05).
    TranscriptUp,
    /// Rola a transcrição para baixo (E10-T05).
    TranscriptDown,
    /// Restaura a entrada selecionada da lixeira.
    Restore,
    /// Esvazia a lixeira (destrutivo; exige challenge, E10-T07).
    EmptyTrash,
    /// Pré-visualiza a compactação do histórico (E10-T07).
    Compact,
    /// Corre o gate de verificação e pede override se bloquear (E09-T03).
    Verify,
    /// Sai da UI.
    Quit,
}

/// Traduz uma tecla na ação correspondente ao modo (função pura, §E10-T02).
#[must_use]
pub fn map_key(key: KeyEvent, mode: Mode) -> Option<Action> {
    let control = key.modifiers.contains(KeyModifiers::CONTROL);
    if control && matches!(key.code, KeyCode::Char('c')) {
        return Some(Action::Quit);
    }
    match mode {
        Mode::Normal => match key.code {
            KeyCode::Char('q') => Some(Action::Quit),
            KeyCode::Enter | KeyCode::Char('i') => Some(Action::EnterInsert),
            KeyCode::Char('/') => Some(Action::StartCommand),
            KeyCode::Char('?') => Some(Action::OpenHelp),
            KeyCode::Up => Some(Action::ScrollUp),
            KeyCode::Down => Some(Action::ScrollDown),
            _ => None,
        },
        Mode::Insert => match key.code {
            KeyCode::Enter => Some(Action::Submit),
            KeyCode::Esc => Some(Action::LeaveInsert),
            KeyCode::Backspace => Some(Action::Backspace),
            KeyCode::Char(_) if control => None,
            KeyCode::Char(character) => Some(Action::Insert(character)),
            _ => None,
        },
        Mode::Confirm => match key.code {
            KeyCode::Char('y' | 'Y') => Some(Action::Confirm),
            KeyCode::Char('n' | 'N') | KeyCode::Esc => Some(Action::Cancel),
            _ => None,
        },
        Mode::Help => match key.code {
            KeyCode::Esc | KeyCode::Char('q' | '?') => Some(Action::CloseOverlay),
            _ => None,
        },
        Mode::Menu => match key.code {
            KeyCode::Up => Some(Action::MenuUp),
            KeyCode::Down => Some(Action::MenuDown),
            KeyCode::Enter => Some(Action::MenuConfirm),
            KeyCode::Esc | KeyCode::Char('q') => Some(Action::CloseOverlay),
            _ => None,
        },
        Mode::Trash => match key.code {
            KeyCode::Char('r') => Some(Action::Restore),
            KeyCode::Char('x') => Some(Action::EmptyTrash),
            KeyCode::Esc | KeyCode::Char('q') => Some(Action::CloseOverlay),
            KeyCode::Up => Some(Action::TrashUp),
            KeyCode::Down => Some(Action::TrashDown),
            _ => None,
        },
        Mode::Transcript => match key.code {
            KeyCode::Esc | KeyCode::Char('q') => Some(Action::CloseOverlay),
            KeyCode::Up => Some(Action::TranscriptUp),
            KeyCode::Down => Some(Action::TranscriptDown),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests;
