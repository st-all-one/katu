//! Estado central da UI (E10-T02/T05/T06): [`App`] detém tudo; o render só lê dele.
//!
//! `apply_action` é **pura** (devolve um [`Command`] quando há efeito); `apply_update` injeta o
//! resultado do executor. Nenhuma variável de UI paralela: fase e pendências vêm do `App`, que a
//! borda alimenta a partir do `State` do kernel.
//!
//! O **painel de atividade** (E10-T05) é efémero: mostra o stream do modelo e as tools em curso,
//! e **nunca** entra no transcript durável nem no contexto do modelo.

use katu_core::provider::Thinking;

use crate::action::{Action, Mode};
use crate::controls::Controls;
use crate::entry::{Entry, Status};
use crate::menu::{Menu, MenuKind};
use crate::message::Command;
use crate::transcript::TranscriptView;
use crate::trash::{Trash, TrashEntry};

mod menu;
mod panel;
mod update;
mod viewer;

/// Estado central da UI.
#[derive(Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "estado da UI: dois flags independentes (sair e modo plano), sem invariante partilhado"
)]
pub struct App {
    input: String,
    /// Caminhos citados com `@<path>` à espera do próximo turno (E20-T12).
    citations: Vec<String>,
    /// Modo de planeamento ligado (E20-T11).
    plan: bool,
    /// Buffer de *steering* em curso durante um turno (E20-T16; efémero).
    steering: String,
    transcript: Vec<Entry>,
    mode: Mode,
    status: Status,
    phase: String,
    /// Texto do modelo em curso (coalescido; efémero).
    streaming: String,
    /// Raciocínio em curso (efémero, fora do ecrã por omissão).
    thinking: String,
    /// Eventos discretos do painel (tools, notas).
    live: Vec<String>,
    /// Controlos do core: modelo e grau de pensamento (E10-T07/E12-T10).
    controls: Controls,
    /// Mini-menu de seleção aberto (`/model`, `/thinking`, E20-T10).
    menu: Option<Menu>,
    /// Menu a abrir quando a borda publicar as capacidades do novo modelo (E20-T10).
    pending_menu: Option<MenuKind>,
    /// Graus de pensamento suportados pelo modelo ativo (E20-T10).
    thinking_options: Vec<Thinking>,
    /// Vista da lixeira (E10-T07/E06-T09).
    trash: Trash,
    /// Vista da transcrição durável (E10-T05).
    viewer: TranscriptView,
    /// Próxima ação declarada no checkpoint (E10-T06).
    next_action: Option<String>,
    /// Uso/custo do último turno (E12-T03/T10); `None` antes do primeiro turno.
    usage: Option<String>,
    /// Deslocamento a partir do fundo (0 = mensagem mais recente).
    scroll: u16,
    quit: bool,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// Estado inicial vazio.
    #[must_use]
    pub fn new() -> Self {
        Self {
            input: String::new(),
            citations: Vec::new(),
            plan: false,
            steering: String::new(),
            transcript: Vec::new(),
            mode: Mode::default(),
            status: Status::default(),
            phase: "task".to_string(),
            streaming: String::new(),
            thinking: String::new(),
            live: Vec::new(),
            controls: Controls::new(),
            menu: None,
            pending_menu: None,
            thinking_options: Vec::new(),
            trash: Trash::new(),
            viewer: TranscriptView::new(),
            next_action: None,
            usage: None,
            scroll: 0,
            quit: false,
        }
    }

    /// Modo de entrada corrente.
    #[must_use]
    pub const fn mode(&self) -> Mode {
        self.mode
    }

    /// Modelo selecionado no seletor (E10-T07); `None` até a borda publicar a lista.
    #[must_use]
    pub fn model(&self) -> Option<&str> {
        self.controls.model()
    }

    /// Grau de pensamento escolhido (E10-T07/E12-T10).
    #[must_use]
    pub const fn reasoning(&self) -> Thinking {
        self.controls.reasoning()
    }

    /// A vista da lixeira está aberta (E10-T07).
    #[must_use]
    pub const fn trash_open(&self) -> bool {
        self.trash.is_open()
    }

    /// Entradas da lixeira listadas (mais recentes primeiro).
    #[must_use]
    pub fn trash_items(&self) -> &[TrashEntry] {
        self.trash.items()
    }

    /// Índice selecionado na lixeira.
    #[must_use]
    pub const fn trash_index(&self) -> usize {
        self.trash.index()
    }

    /// Linha de mensagem em edição.
    #[must_use]
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Buffer de *steering* em curso durante um turno (E20-T16).
    #[must_use]
    pub fn steering(&self) -> &str {
        &self.steering
    }

    /// `true` se o modo de planeamento está ligado (E20-T11).
    #[must_use]
    pub const fn plan_mode(&self) -> bool {
        self.plan
    }

    /// Substitui o buffer de *steering* (chamado pelo pintor durante o turno).
    pub fn set_steering(&mut self, text: &str) {
        self.steering.clear();
        self.steering.push_str(text);
    }

    /// Conversa mostrada.
    #[must_use]
    pub fn transcript(&self) -> &[Entry] {
        &self.transcript
    }

    /// Estado da barra.
    #[must_use]
    pub const fn status(&self) -> &Status {
        &self.status
    }

    /// Fase do kernel (E10-T06).
    #[must_use]
    pub fn phase(&self) -> &str {
        &self.phase
    }

    /// Texto em curso do modelo (painel de atividade, E10-T05).
    #[must_use]
    pub fn streaming(&self) -> &str {
        &self.streaming
    }

    /// Raciocínio em curso (efémero, fora do ecrã por omissão).
    #[must_use]
    pub fn thinking(&self) -> &str {
        &self.thinking
    }

    /// Linhas discretas do painel de atividade.
    #[must_use]
    pub fn live(&self) -> &[String] {
        &self.live
    }

    /// Há um efeito em voo.
    #[must_use]
    pub const fn pending(&self) -> bool {
        matches!(self.status, Status::Working)
    }

    /// Próxima ação declarada no checkpoint (E10-T06); `None` antes do primeiro turno.
    #[must_use]
    pub fn next_action(&self) -> Option<&str> {
        self.next_action.as_deref()
    }

    /// Uso/custo do último turno (E12-T03/T10); `None` antes do primeiro turno.
    #[must_use]
    pub fn usage(&self) -> Option<&str> {
        self.usage.as_deref()
    }

    /// Deslocamento a partir do fundo (para o render).
    #[must_use]
    pub const fn scroll(&self) -> u16 {
        self.scroll
    }

    /// `true` quando o loop deve terminar.
    #[must_use]
    pub const fn should_quit(&self) -> bool {
        self.quit
    }

    /// Aplica uma ação: muda só o estado; devolve [`Command`] quando a borda tem de agir.
    pub fn apply_action(&mut self, action: Action) -> Option<Command> {
        match action {
            Action::EnterInsert => self.mode = Mode::Insert,
            Action::StartCommand => {
                self.mode = Mode::Insert;
                self.input.clear();
                self.input.push('/');
            }
            Action::LeaveInsert => self.mode = Mode::Normal,
            Action::Insert(character) => {
                if self.mode == Mode::Insert {
                    self.input.push(character);
                }
            }
            Action::Backspace => {
                self.input.pop();
            }
            Action::Submit => return self.submit(),
            Action::Cancel => {
                self.mode = Mode::Normal;
                self.input.clear();
                self.menu = None;
            }
            Action::Confirm => self.status = Status::Message("nada a confirmar".to_string()),
            Action::ScrollUp => self.scroll = self.scroll.saturating_add(1),
            Action::ScrollDown => self.scroll = self.scroll.saturating_sub(1),
            Action::OpenHelp
            | Action::MenuUp
            | Action::MenuDown
            | Action::MenuConfirm
            | Action::OpenTrash
            | Action::OpenTranscript
            | Action::CloseOverlay => return self.overlay_action(action),
            Action::TrashUp => self.trash.up(),
            Action::TrashDown => self.trash.down(),
            Action::TranscriptUp => self.viewer.up(),
            Action::TranscriptDown => self.viewer.down(),
            Action::Restore => {
                return self
                    .trash
                    .selected()
                    .map(|entry| Command::Restore(entry.stored.clone()));
            }
            Action::EmptyTrash => return Some(Command::EmptyTrash),
            Action::Compact => return Some(Command::Compact),
            Action::Verify => return Some(Command::Verify),
            Action::Quit => {
                self.quit = true;
                return Some(Command::Quit);
            }
        }
        None
    }
}
