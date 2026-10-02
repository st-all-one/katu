//! Fluxo de login guiado na TUI (E21): `/login` escolhe o provider e pede o que falta.
//!
//! O parse é **puro** e devolve um [`Command`] quando a borda tem de agir; a chave fica no pedido e
//! é mascarada no render ([`App::input_display`]). O embedding nunca é tocado aqui.

use katu_core::diag::{Level, events};

use crate::action::Mode;
use crate::entry::Status;
use crate::menu::Menu;
use crate::message::{Command, LoginRequest};

use super::App;

/// Passo do login guiado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LoginStep {
    /// À espera da chave do opencode.
    ApiKey,
    /// À espera da URL base do llama.cpp.
    Base,
}

/// Login em curso: provider já escolhido, à espera dos dados que faltam.
#[derive(Debug, Clone)]
pub(super) struct LoginFlow {
    /// Id canônico do provider.
    pub(super) provider: String,
    /// Passo corrente.
    pub(super) step: LoginStep,
}

impl App {
    /// `/login` sem argumentos abre o menu; com argumentos resolve já o pedido.
    pub(super) fn slash_login(&mut self, rest: &str) -> Option<Command> {
        let _span = katu_core::fn_span!(Level::Debug, events::TUI_SLASH, "app::slash_login");
        let mut parts = rest.split_whitespace();
        let Some(provider) = parts.next() else {
            self.menu = Some(Menu::login());
            self.mode = Mode::Menu;
            return None;
        };
        let provider = normalize_login_provider(provider);
        let argument = parts.next();
        if provider == "llama" {
            return Some(Command::Login(LoginRequest {
                provider,
                api_key: None,
                base: argument.map(str::to_string),
                model: None,
                logout: false,
            }));
        }
        let api_key = argument.map(str::to_string).filter(|key| !key.is_empty());
        Some(Command::Login(LoginRequest {
            provider,
            api_key,
            base: None,
            model: None,
            logout: false,
        }))
    }

    /// `/logout`: termina a sessão do agente principal.
    pub(super) fn slash_logout(&mut self) -> Command {
        let _span = katu_core::fn_span!(Level::Debug, events::TUI_SLASH, "app::slash_logout");
        self.status = Status::Message("a terminar a sessão…".to_string());
        Command::Login(LoginRequest {
            provider: "opencode-go".to_string(),
            api_key: None,
            base: None,
            model: None,
            logout: true,
        })
    }

    /// Inicia o passo de entrada do provider escolhido no menu de login.
    pub(super) fn begin_login(&mut self, provider: &str) -> Option<Command> {
        let _span = katu_core::fn_span!(Level::Debug, events::TUI_SLASH, "app::begin_login");
        let step = if provider == "llama" {
            LoginStep::Base
        } else {
            LoginStep::ApiKey
        };
        self.status = Status::Message(match step {
            LoginStep::ApiKey => "chave da API do opencode (não mostrada)".to_string(),
            LoginStep::Base => "URL base do llama.cpp (vazio = default)".to_string(),
        });
        self.login = Some(LoginFlow {
            provider: provider.to_string(),
            step,
        });
        self.mode = Mode::Insert;
        self.input.clear();
        None
    }

    /// Trata a linha submetida quando há um login em curso.
    pub(super) fn login_submit(&mut self, text: &str) -> Option<Command> {
        let _span = katu_core::fn_span!(Level::Debug, events::TUI_ACTION, "app::login_submit");
        let flow = self.login.take()?;
        match flow.step {
            LoginStep::ApiKey => {
                if text.is_empty() {
                    self.status = Status::Failure("chave vazia: login cancelado".to_string());
                    return None;
                }
                Some(Command::Login(LoginRequest {
                    provider: flow.provider,
                    api_key: Some(text.to_string()),
                    base: None,
                    model: None,
                    logout: false,
                }))
            }
            LoginStep::Base => {
                let base = if text.is_empty() {
                    None
                } else {
                    Some(text.to_string())
                };
                Some(Command::Login(LoginRequest {
                    provider: flow.provider,
                    api_key: None,
                    base,
                    model: None,
                    logout: false,
                }))
            }
        }
    }

    /// `true` quando há um login guiado em curso.
    #[must_use]
    pub fn login_pending(&self) -> bool {
        let _span = katu_core::trace_fn!("app::login_pending");

        self.login.is_some()
    }
}

/// Normaliza o nome curto para o id canônico do login.
fn normalize_login_provider(name: &str) -> String {
    let _span = katu_core::trace_fn!("app::normalize_login_provider");

    match name {
        "opencode" | "opencode-go" | "go" => "opencode-go".to_string(),
        "opencode-zen" | "zen" => "opencode-zen".to_string(),
        "llama" | "llama.cpp" | "llama-cpp" | "local" => "llama".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Action;
    use crate::menu::{Menu, MenuKind};

    /// Extrai o pedido de login de um comando.
    fn request(command: Option<Command>) -> Option<LoginRequest> {
        match command {
            Some(Command::Login(request)) => Some(request),
            _ => None,
        }
    }

    #[test]
    fn login_without_arguments_opens_the_menu() {
        let mut app = App::new();
        assert!(app.slash_login("").is_none());
        assert_eq!(app.menu().map(Menu::kind), Some(MenuKind::Login));
    }

    #[test]
    fn login_opencode_with_key_emits_a_request() -> Result<(), Box<dyn std::error::Error>> {
        let mut app = App::new();
        let request = request(app.slash_login("opencode sk-1")).ok_or("devia emitir login")?;
        assert_eq!(request.provider, "opencode-go");
        assert_eq!(request.api_key.as_deref(), Some("sk-1"));
        assert!(!request.logout);
        Ok(())
    }

    #[test]
    fn login_llama_with_base_emits_a_request() -> Result<(), Box<dyn std::error::Error>> {
        let mut app = App::new();
        let base = "http://127.0.0.1:9999/v1";
        let request =
            request(app.slash_login(&format!("llama {base}"))).ok_or("devia emitir login")?;
        assert_eq!(request.provider, "llama");
        assert_eq!(request.base.as_deref(), Some(base));
        Ok(())
    }

    #[test]
    fn guided_login_asks_for_the_key() -> Result<(), Box<dyn std::error::Error>> {
        let mut app = App::new();
        assert!(app.begin_login("opencode-go").is_none());
        assert!(app.login_pending());
        let request = request(app.login_submit("sk-secreta")).ok_or("devia emitir login")?;
        assert_eq!(request.api_key.as_deref(), Some("sk-secreta"));
        assert!(!app.login_pending());
        Ok(())
    }

    #[test]
    fn guided_login_accepts_the_default_base_with_empty_input()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut app = App::new();
        assert!(app.begin_login("llama").is_none());
        let request = request(app.login_submit("")).ok_or("devia emitir login")?;
        assert_eq!(request.provider, "llama");
        assert_eq!(request.base, None, "vazio deixa o default do provider");
        Ok(())
    }

    #[test]
    fn api_key_is_masked_in_the_input_display() {
        let mut app = App::new();
        assert!(app.begin_login("opencode-go").is_none());
        for character in "sk-1".chars() {
            app.apply_action(Action::Insert(character));
        }
        assert_eq!(app.input(), "sk-1");
        assert_eq!(app.input_display(), "****");
    }

    #[test]
    fn logout_emits_a_logout_request() {
        let mut app = App::new();
        assert!(request(Some(app.slash_logout())).is_some_and(|request| request.logout));
    }
}
