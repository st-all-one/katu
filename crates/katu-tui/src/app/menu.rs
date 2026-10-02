//! Comandos `/` e mini-menus (E20-T10).
//!
//! A linha de mensagem aceita comandos quando começa por `/`. Vive num módulo filho para manter
//! `app.rs` sob o teto de linhas; o parse é **puro** e devolve um [`Command`] quando a borda tem de
//! agir.

use katu_core::diag::{Level, events};
use katu_core::provider::Thinking;

use crate::action::{Action, Mode};
use crate::entry::{Entry, Role, Status};
use crate::menu::{Menu, MenuChoice, MenuKind};
use crate::message::Command;

use super::App;

impl App {
    /// Mini-menu aberto, se houver (E20-T10).
    #[must_use]
    pub fn menu(&self) -> Option<&Menu> {
        let _span = katu_core::trace_fn!("app::menu::menu");

        self.menu.as_ref()
    }

    /// A sobreposição de ajuda está aberta (E20-T10).
    #[must_use]
    pub const fn help_open(&self) -> bool {
        matches!(self.mode, Mode::Help)
    }

    /// Abre o menu de modelos (a lista vem da borda, `Update::Models`).
    fn open_model_menu(&mut self) {
        let _span = katu_core::trace_fn!("app::menu::open_model_menu");

        match Menu::models(self.controls.models(), self.controls.model()) {
            Some(menu) => {
                self.menu = Some(menu);
                self.mode = Mode::Menu;
            }
            None => self.status = Status::Message("sem modelos publicados".to_string()),
        }
    }

    /// Abre o menu de pensamento (só os graus suportados pelo modelo ativo).
    fn open_thinking_menu(&mut self) {
        let _span = katu_core::trace_fn!("app::menu::open_thinking_menu");

        let options = if self.thinking_options.is_empty() {
            vec![Thinking::Off]
        } else {
            self.thinking_options.clone()
        };
        self.menu = Some(Menu::thinking(&options, self.controls.reasoning()));
        self.mode = Mode::Menu;
    }

    /// Aplica as capacidades publicadas pela borda (E20-T10).
    ///
    /// Se o modelo acabou de mudar (`pending_menu`), abre o submenu de pensamento adaptado às
    /// novas capacidades — só quando há mais do que `off` para escolher.
    pub(super) fn apply_thinking_options(&mut self, options: Vec<Thinking>) {
        let _span = katu_core::fn_span!(
            Level::Debug,
            events::TUI_ACTION,
            "app::apply_thinking_options"
        );
        self.thinking_options = options;
        if self.pending_menu == Some(MenuKind::Thinking) {
            self.pending_menu = None;
            if self.thinking_options.len() > 1 {
                self.open_thinking_menu();
            }
        }
    }

    /// Trata as ações de sobreposição/menu (`OpenHelp`…`CloseOverlay`).
    pub(super) fn overlay_action(&mut self, action: Action) -> Option<Command> {
        let _span = katu_core::fn_span!(Level::Debug, events::TUI_ACTION, "app::overlay_action");
        match action {
            Action::OpenHelp => self.mode = Mode::Help,
            Action::MenuUp => {
                if let Some(menu) = self.menu.as_mut() {
                    menu.up();
                }
            }
            Action::MenuDown => {
                if let Some(menu) = self.menu.as_mut() {
                    menu.down();
                }
            }
            Action::MenuConfirm => return self.confirm_menu(),
            Action::OpenTrash => {
                self.trash.open();
                self.mode = Mode::Trash;
                return Some(Command::Trash);
            }
            Action::OpenTranscript => {
                self.viewer.open();
                self.mode = Mode::Transcript;
                return Some(Command::Transcript);
            }
            Action::CloseOverlay => {
                self.trash.close();
                self.viewer.close();
                self.menu = None;
                self.mode = Mode::Normal;
            }
            _ => {}
        }
        None
    }

    /// Confirma a escolha do menu e devolve o `Command` à borda.
    fn confirm_menu(&mut self) -> Option<Command> {
        let _span = katu_core::fn_span!(Level::Debug, events::TUI_ACTION, "app::confirm_menu");
        let choice = self.menu.as_ref().and_then(|menu| menu.selected().cloned());
        self.menu = None;
        self.mode = Mode::Normal;
        match choice {
            Some(MenuChoice::Model(model)) => {
                self.controls.set_model(&model);
                self.pending_menu = Some(MenuKind::Thinking);
                Some(Command::SetModel(model))
            }
            Some(MenuChoice::Thinking(thinking)) => {
                self.controls.set_thinking(thinking);
                Some(Command::SetThinking(thinking))
            }
            Some(MenuChoice::Login(id)) => self.begin_login(&id),
            None => None,
        }
    }

    /// Trata a linha submetida quando começa por `/` (E20-T10).
    pub(super) fn slash(&mut self, command: &str) -> Option<Command> {
        let _span = katu_core::fn_span!(Level::Debug, events::TUI_SLASH, "app::slash");
        let name = command.split_whitespace().next().unwrap_or("");
        if let Some(skill) = name.strip_prefix("skill:") {
            return self.skill(skill);
        }
        match name {
            "model" => {
                self.open_model_menu();
                None
            }
            "thinking" => {
                self.open_thinking_menu();
                None
            }
            "help" => {
                self.mode = Mode::Help;
                None
            }
            "compact" => Some(Command::Compact),
            "plan" => {
                self.status = Status::Message("a alternar plano…".to_string());
                Some(Command::Plan)
            }
            "verify" => Some(Command::Verify),
            "trash" => {
                self.trash.open();
                self.mode = Mode::Trash;
                Some(Command::Trash)
            }
            "transcript" => {
                self.viewer.open();
                self.mode = Mode::Transcript;
                Some(Command::Transcript)
            }
            "login" => self.slash_login(command.strip_prefix("login").unwrap_or("").trim()),
            "logout" => Some(self.slash_logout()),
            "quit" | "q" => {
                self.quit = true;
                Some(Command::Quit)
            }
            other => {
                self.status = Status::Failure(format!("comando desconhecido: /{other}"));
                None
            }
        }
    }

    /// Toma a mensagem escrita e devolve o pedido de submissão (se não estiver vazia).
    ///
    /// `@<path>` cita caminhos (E20-T12) e `!<cmd>` executa shell pela política (E20-T12); no modo
    /// plano a regra `plan-no-shell` recusa `!` com evidência.
    pub(super) fn submit(&mut self) -> Option<Command> {
        let _span = katu_core::fn_span!(Level::Debug, events::TUI_ACTION, "app::submit");
        let text = self.input.trim().to_string();
        self.input.clear();
        self.mode = Mode::Normal;
        if self.login.is_some() {
            return self.login_submit(&text);
        }
        if text.is_empty() {
            return None;
        }
        if let Some(command) = text.strip_prefix('/') {
            return self.slash(command.trim());
        }
        if let Some(rest) = text.strip_prefix('@') {
            return self.cite(rest);
        }
        if let Some(rest) = text.strip_prefix('!') {
            return self.shell(rest);
        }
        let goal = self.take_goal(text);
        self.transcript.push(Entry {
            role: Role::User,
            text: goal.clone(),
        });
        self.scroll = 0;
        self.status = Status::Working;
        self.clear_live();
        Some(Command::Submit(goal))
    }

    /// Caminhos citados à espera do próximo turno (E20-T12).
    #[must_use]
    pub fn citations(&self) -> &[String] {
        let _span = katu_core::trace_fn!("app::menu::citations");

        &self.citations
    }

    /// Enfileira os caminhos de uma linha `@<path>` (E20-T12).
    fn cite(&mut self, rest: &str) -> Option<Command> {
        let _span = katu_core::fn_span!(Level::Debug, events::TUI_ACTION, "app::cite");
        let paths = parse_citations(rest);
        if paths.is_empty() {
            self.status = Status::Failure("uso: @<caminho> [@<caminho>…]".to_string());
            return None;
        }
        for path in paths {
            self.citations.push(path);
        }
        let joined = self.citations.join(", ");
        self.status = Status::Message(format!("citado: {joined}"));
        None
    }

    /// Prefixa o objetivo com os caminhos citados pendentes e limpa-os (E20-T12).
    fn take_goal(&mut self, goal: String) -> String {
        let _span = katu_core::trace_fn!("app::menu::take_goal");

        if self.citations.is_empty() {
            return goal;
        }
        let cited = self.citations.join(" ");
        self.citations.clear();
        format!("{cited}\n\n{goal}")
    }

    /// Força o carregamento de uma skill pelo nome (E20-T13).
    fn skill(&mut self, name: &str) -> Option<Command> {
        let _span = katu_core::fn_span!(Level::Debug, events::TUI_ACTION, "app::skill");
        let name = name.trim();
        if name.is_empty() {
            self.status = Status::Failure("uso: /skill:<nome>".to_string());
            return None;
        }
        self.transcript.push(Entry {
            role: Role::User,
            text: format!("/skill:{name}"),
        });
        self.scroll = 0;
        self.status = Status::Working;
        self.clear_live();
        Some(Command::Skill(name.to_string()))
    }

    /// Prepara um comando shell `!<cmd>` (E20-T12).
    fn shell(&mut self, rest: &str) -> Option<Command> {
        let _span = katu_core::fn_span!(Level::Debug, events::TUI_ACTION, "app::shell");
        let command = rest.trim();
        if command.is_empty() {
            self.status = Status::Failure("uso: !<comando>".to_string());
            return None;
        }
        self.transcript.push(Entry {
            role: Role::User,
            text: format!("!{command}"),
        });
        self.scroll = 0;
        self.status = Status::Working;
        self.clear_live();
        Some(Command::Shell(command.to_string()))
    }
}

/// Extrai os caminhos de uma linha de citação (`src @a` → `@src`, `@a`).
fn parse_citations(rest: &str) -> Vec<String> {
    let _span = katu_core::trace_fn!("app::menu::parse_citations");

    rest.split_whitespace()
        .filter(|token| !token.is_empty())
        .map(|token| {
            if token.starts_with('@') {
                token.to_string()
            } else {
                format!("@{token}")
            }
        })
        .collect()
}
