//! Estado central da UI (E10-T02/T05/T06): [`App`] detém tudo; o render só lê dele.
//!
//! `apply_action` é **pura** (devolve um [`Command`] quando há efeito); `apply_update` injeta o
//! resultado do executor. Nenhuma variável de UI paralela: fase e pendências vêm do `App`, que a
//! borda alimenta a partir do `State` do kernel.
//!
//! O **painel de atividade** (E10-T05) é efémero: mostra o stream do modelo e as tools em curso,
//! e **nunca** entra no transcript durável nem no contexto do modelo.

use crate::action::{Action, Mode};
use crate::live::Live;

/// Papel de uma entrada da conversa (cor e prefixo no render).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Mensagem do utilizador.
    User,
    /// Resposta do modelo.
    Assistant,
    /// Chamada/resultado de tool.
    Tool,
    /// Nota informativa.
    Info,
    /// Erro.
    Error,
}

/// Uma linha da conversa (durável na sessão; o `App` só a mostra).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Papel.
    pub role: Role,
    /// Texto.
    pub text: String,
}

/// Estado da barra de estado.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Status {
    /// Parado.
    #[default]
    Idle,
    /// À espera de um efeito (o utilizador não acha que travou).
    Working,
    /// Mensagem transitória.
    Message(String),
    /// Falha.
    Failure(String),
}

/// Comando emitido pela UI para a borda executar (I/O fora do render).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Submeter uma mensagem ao agente.
    Submit(String),
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
    /// Turno concluído (limpa a pendência).
    Done,
}

/// Estado central da UI.
#[derive(Debug)]
pub struct App {
    input: String,
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
            transcript: Vec::new(),
            mode: Mode::default(),
            status: Status::default(),
            phase: "task".to_string(),
            streaming: String::new(),
            thinking: String::new(),
            live: Vec::new(),
            scroll: 0,
            quit: false,
        }
    }

    /// Modo de entrada corrente.
    #[must_use]
    pub const fn mode(&self) -> Mode {
        self.mode
    }

    /// Linha de mensagem em edição.
    #[must_use]
    pub fn input(&self) -> &str {
        &self.input
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
            }
            Action::Confirm => self.status = Status::Message("nada a confirmar".to_string()),
            Action::ScrollUp => self.scroll = self.scroll.saturating_add(1),
            Action::ScrollDown => self.scroll = self.scroll.saturating_sub(1),
            Action::Quit => {
                self.quit = true;
                return Some(Command::Quit);
            }
        }
        None
    }

    /// Toma a mensagem escrita e devolve o pedido de submissão (se não estiver vazia).
    fn submit(&mut self) -> Option<Command> {
        let text = self.input.trim().to_string();
        self.input.clear();
        self.mode = Mode::Normal;
        if text.is_empty() {
            return None;
        }
        self.transcript.push(Entry {
            role: Role::User,
            text: text.clone(),
        });
        self.scroll = 0;
        self.status = Status::Working;
        self.clear_live();
        Some(Command::Submit(text))
    }

    /// Injeta o resultado do executor.
    pub fn apply_update(&mut self, update: Update) {
        match update {
            Update::Assistant(text) => self.push(Role::Assistant, text),
            Update::Tool(text) => self.push(Role::Tool, text),
            Update::Info(text) => self.push(Role::Info, text),
            Update::Error(text) => {
                self.status = Status::Failure(text.clone());
                self.push(Role::Error, text);
            }
            Update::Phase(phase) => self.phase = phase,
            Update::Live(live) => self.apply_live(live),
            Update::Done => {
                self.clear_live();
                self.status = Status::Idle;
            }
        }
    }

    /// Aplica um evento efémero ao painel de atividade (não toca no transcript).
    fn apply_live(&mut self, live: Live) {
        match live {
            Live::Text(delta) => self.streaming.push_str(&delta),
            Live::Thinking(delta) => self.thinking.push_str(&delta),
            Live::Tool(name) => self.live.push(format!("→ {name}")),
            Live::ToolDone(name) => self.live.push(format!("✓ {name}")),
            Live::Refused { rule, evidence } => {
                let text = format!("⛔ {rule}: {evidence}");
                self.live.push(text.clone());
                self.push(Role::Error, text);
            }
            Live::Unavailable { control } => {
                self.live.push(format!("⚠ falta {control}"));
            }
            Live::Clear => self.clear_live(),
        }
    }

    /// Limpa o painel de atividade.
    fn clear_live(&mut self) {
        self.streaming.clear();
        self.thinking.clear();
        self.live.clear();
    }

    /// Acrescenta uma entrada (ignora texto vazio) e volta ao fundo.
    fn push(&mut self, role: Role, text: String) {
        if !text.is_empty() {
            self.transcript.push(Entry { role, text });
            self.scroll = 0;
        }
    }
}
