//! Mini-menus de comandos `/` (E20-T10): modelo e grau de pensamento.
//!
//! A lista de modelos vem da borda (`Update::Models`); os graus de pensamento vêm das
//! **capacidades** do modelo (`Update::ThinkingOptions`), pelo que o menu só oferece o que o
//! modelo suporta. O menu é estado puro: navegar e confirmar não faz I/O.

use katu_core::diag::{Level, events};
use katu_core::provider::Thinking;

/// Tipo de mini-menu aberto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKind {
    /// Seleção de modelo.
    Model,
    /// Seleção do grau de pensamento.
    Thinking,
}

impl MenuKind {
    /// Título mostrado na sobreposição.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Model => "modelo",
            Self::Thinking => "pensamento",
        }
    }
}

/// Uma escolha do menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuChoice {
    /// Um modelo.
    Model(String),
    /// Um grau de pensamento.
    Thinking(Thinking),
}

impl MenuChoice {
    /// Texto mostrado no menu.
    #[must_use]
    pub fn label(&self) -> String {
        let _span = katu_core::trace_fn!("menu::label");

        match self {
            Self::Model(model) => model.clone(),
            Self::Thinking(thinking) => thinking.as_str().to_string(),
        }
    }
}

/// Mini-menu de seleção (com índice selecionado).
#[derive(Debug, Clone)]
pub struct Menu {
    kind: MenuKind,
    items: Vec<MenuChoice>,
    index: usize,
}

impl Menu {
    /// Menu de modelos (vazio → `None`); posiciona no modelo atual, se conhecido.
    #[must_use]
    pub fn models(models: &[String], current: Option<&str>) -> Option<Self> {
        let _span = katu_core::trace_fn!("menu::models");

        if models.is_empty() {
            return None;
        }
        let index = current
            .and_then(|name| models.iter().position(|model| model == name))
            .unwrap_or(0);
        Some(Self {
            kind: MenuKind::Model,
            items: models.iter().cloned().map(MenuChoice::Model).collect(),
            index,
        })
    }

    /// Menu de graus de pensamento (só os suportados); posiciona no atual.
    #[must_use]
    pub fn thinking(options: &[Thinking], current: Thinking) -> Self {
        let _span = katu_core::trace_fn!("menu::thinking");

        let index = options
            .iter()
            .position(|option| *option == current)
            .unwrap_or(0);
        Self {
            kind: MenuKind::Thinking,
            items: options.iter().copied().map(MenuChoice::Thinking).collect(),
            index,
        }
    }

    /// Tipo do menu.
    #[must_use]
    pub const fn kind(&self) -> MenuKind {
        self.kind
    }

    /// Índice selecionado.
    #[must_use]
    pub const fn index(&self) -> usize {
        self.index
    }

    /// Escolhas (para o render).
    #[must_use]
    pub fn items(&self) -> &[MenuChoice] {
        let _span = katu_core::trace_fn!("menu::items");

        &self.items
    }

    /// Move a seleção para cima (satura no topo).
    pub fn up(&mut self) {
        let _span = katu_core::fn_span!(Level::Debug, events::TUI_SLASH, "menu::up");
        self.index = self.index.saturating_sub(1);
    }

    /// Move a seleção para baixo (satura no fundo).
    pub fn down(&mut self) {
        let _span = katu_core::fn_span!(Level::Debug, events::TUI_SLASH, "menu::down");
        if self.index.saturating_add(1) < self.items.len() {
            self.index = self.index.saturating_add(1);
        }
    }

    /// Escolha selecionada.
    #[must_use]
    pub fn selected(&self) -> Option<&MenuChoice> {
        let _span = katu_core::trace_fn!("menu::selected");

        self.items.get(self.index)
    }
}

#[cfg(test)]
mod tests {
    use katu_core::provider::Thinking;

    use super::{Menu, MenuChoice, MenuKind};

    #[test]
    fn model_menu_positions_on_the_current_model() -> Result<(), Box<dyn std::error::Error>> {
        let models = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let menu = Menu::models(&models, Some("b")).ok_or("menu devia existir")?;
        assert_eq!(menu.kind(), MenuKind::Model);
        assert_eq!(menu.index(), 1);
        assert_eq!(menu.selected(), Some(&MenuChoice::Model("b".to_string())));
        Ok(())
    }

    #[test]
    fn model_menu_is_none_without_models() {
        assert!(Menu::models(&[], None).is_none());
    }

    #[test]
    fn navigation_saturates() -> Result<(), Box<dyn std::error::Error>> {
        let models = vec!["a".to_string(), "b".to_string()];
        let mut menu = Menu::models(&models, Some("a")).ok_or("menu devia existir")?;
        menu.up();
        assert_eq!(menu.index(), 0, "satura no topo");
        menu.down();
        menu.down();
        assert_eq!(menu.index(), 1, "satura no fundo");
        Ok(())
    }

    #[test]
    fn thinking_menu_only_lists_supported_grades() {
        let menu = Menu::thinking(&[Thinking::Off], Thinking::Off);
        assert_eq!(menu.items().len(), 1);
        assert_eq!(menu.kind(), MenuKind::Thinking);
        assert_eq!(menu.selected(), Some(&MenuChoice::Thinking(Thinking::Off)));
    }
}
