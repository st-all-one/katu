//! Challenge-and-response para aprovações humanas (E10-T04, §33).
//!
//! Uma aprovação **nunca** é um *rubber-stamp*: exige responder a perguntas positivas (checklist)
//! **e** escrever uma justificação. Só então o chamador assina (`granted_by`) e o kernel concede a
//! capacidade mínima. Este módulo é **puro** (estado + render), sem I/O.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[cfg(test)]
mod tests;
mod view;

pub(crate) use view::render;

/// Perguntas positivas do checklist (todas obrigatórias).
pub const QUESTIONS: [&str; 3] = [
    "Li a regra e a evidência.",
    "Quero que esta operação corra.",
    "A autorização é só para este alvo.",
];

/// Foco do challenge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// Checklist de perguntas.
    Checklist,
    /// Justificação escrita.
    Reason,
}

/// Passo do challenge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Continua à espera.
    Continue,
    /// Completo e aprovado.
    Approved,
    /// Cancelado (fail-closed).
    Cancelled,
}

/// Tecla normalizada do challenge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// Sobe.
    Up,
    /// Desce.
    Down,
    /// Alterna o foco.
    Tab,
    /// Alterna a pergunta atual.
    Toggle,
    /// Caractere na justificação.
    Char(char),
    /// Apaga o último caractere da justificação.
    Backspace,
    /// Tenta submeter.
    Submit,
    /// Cancela.
    Cancel,
}

/// Pedido de challenge apresentado ao humano.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChallengePrompt {
    /// Nome ao modelo da tool.
    pub tool: String,
    /// Regra que exige a aprovação.
    pub rule: String,
    /// Âmbito concreto (caminho/host/comando).
    pub scope: String,
}

/// Assinatura resultante de um challenge completo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChallengeSignature {
    /// Justificação (`override_reason`).
    pub reason: String,
    /// Quem assinou (`granted_by`).
    pub granted_by: String,
}

/// Direção de movimento do cursor do checklist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Move {
    /// Sobe.
    Up,
    /// Desce.
    Down,
}

/// Estado puro do challenge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Challenge {
    prompt: ChallengePrompt,
    checked: Vec<bool>,
    reason: String,
    focus: Focus,
    cursor: usize,
}

impl Challenge {
    /// Cria um challenge para o pedido `prompt`.
    #[must_use]
    pub fn new(prompt: ChallengePrompt) -> Self {
        Self {
            prompt,
            checked: vec![false; QUESTIONS.len()],
            reason: String::new(),
            focus: Focus::Checklist,
            cursor: 0,
        }
    }

    /// Pedido apresentado.
    #[must_use]
    pub fn prompt(&self) -> &ChallengePrompt {
        &self.prompt
    }

    /// Estado de cada pergunta.
    #[must_use]
    pub fn checked(&self) -> &[bool] {
        &self.checked
    }

    /// Foco atual.
    #[must_use]
    pub const fn focus(&self) -> Focus {
        self.focus
    }

    /// Índice da pergunta em foco.
    #[must_use]
    pub const fn cursor(&self) -> usize {
        self.cursor
    }

    /// Justificação escrita.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// `true` se todas as perguntas foram respondidas e há justificação.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.checked.iter().all(|checked| *checked) && !self.reason.trim().is_empty()
    }

    /// Aplica uma tecla, devolvendo o passo seguinte.
    pub fn apply(&mut self, key: Key) -> Step {
        match key {
            Key::Cancel => return Step::Cancelled,
            Key::Tab => self.focus = toggle_focus(self.focus),
            Key::Up => self.move_cursor(Move::Up),
            Key::Down => self.move_cursor(Move::Down),
            Key::Toggle if self.focus == Focus::Checklist => {
                if let Some(checked) = self.checked.get_mut(self.cursor) {
                    *checked = !*checked;
                }
            }
            Key::Toggle if self.focus == Focus::Reason => self.reason.push(' '),
            Key::Char(c) if self.focus == Focus::Reason => self.reason.push(c),
            Key::Backspace if self.focus == Focus::Reason => {
                self.reason.pop();
            }
            Key::Submit if self.is_complete() => return Step::Approved,
            _ => {}
        }
        Step::Continue
    }

    /// Assinatura final, quando o challenge está completo.
    #[must_use]
    pub fn signature(&self, granted_by: &str) -> Option<ChallengeSignature> {
        self.is_complete().then(|| ChallengeSignature {
            reason: self.reason.trim().to_string(),
            granted_by: granted_by.to_string(),
        })
    }

    fn move_cursor(&mut self, direction: Move) {
        if self.focus != Focus::Checklist {
            return;
        }
        let last = QUESTIONS.len().saturating_sub(1);
        self.cursor = match direction {
            Move::Down => self.cursor.saturating_add(1).min(last),
            Move::Up => self.cursor.saturating_sub(1),
        };
    }
}

/// Alterna o foco.
const fn toggle_focus(focus: Focus) -> Focus {
    match focus {
        Focus::Checklist => Focus::Reason,
        Focus::Reason => Focus::Checklist,
    }
}

/// Normaliza uma tecla do terminal (o `Ctrl-C` cancela sempre).
#[must_use]
pub(super) fn map_key(key: KeyEvent) -> Option<Key> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Some(Key::Cancel);
    }
    match key.code {
        KeyCode::Esc => Some(Key::Cancel),
        KeyCode::Tab | KeyCode::BackTab => Some(Key::Tab),
        KeyCode::Up => Some(Key::Up),
        KeyCode::Down => Some(Key::Down),
        KeyCode::Enter => Some(Key::Submit),
        KeyCode::Backspace => Some(Key::Backspace),
        KeyCode::Char(' ') => Some(Key::Toggle),
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => Some(Key::Char(c)),
        _ => None,
    }
}
