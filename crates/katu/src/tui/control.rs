//! Controlo de modelo/pensamento na TUI (E12-T10): valida contra o catálogo e regista no kernel.

use katu_core::error::Error;
use katu_core::kernel::Control;
use katu_core::provider::{ModelCapabilities, Provider, Thinking};
use katu_tui::Update;

use super::AgentHandler;

/// Graus de pensamento suportados pelo modelo (E20-T10): `[off]` se não raciocina.
pub(super) fn thinking_options(provider: &dyn Provider, model: &str) -> Vec<Thinking> {
    if provider.capabilities(model).reasoning {
        vec![
            Thinking::Off,
            Thinking::Low,
            Thinking::Medium,
            Thinking::High,
        ]
    } else {
        vec![Thinking::Off]
    }
}

impl AgentHandler<'_> {
    /// Define o modelo ativo (validado contra o catálogo; erro que ensina).
    ///
    /// Devolve também os graus de pensamento do novo modelo (E20-T10), para o menu se adaptar.
    pub(super) fn set_model(&mut self, model: String) -> Vec<Update> {
        let options = thinking_options(self.provider.as_ref(), &model);
        let caps = self.provider.capabilities(&model);
        let mut updates = self.apply_control(&Control::SetModel { model }, &caps);
        updates.push(Update::ThinkingOptions(options));
        updates
    }

    /// Define o grau de pensamento do modelo **ativo** (o agente nunca se auto-escala).
    pub(super) fn set_thinking(&mut self, thinking: Thinking) -> Vec<Update> {
        let current = self.runtime.control();
        let model = current.model.unwrap_or_else(|| self.model.model.clone());
        let caps = self.provider.capabilities(&model);
        self.apply_control(&Control::SetThinking { thinking }, &caps)
    }

    /// Aplica o controlo no runtime (logado) e traduz em `Update`.
    fn apply_control(&mut self, control: &Control, caps: &ModelCapabilities) -> Vec<Update> {
        let summary = control.summary();
        match self.runtime.set_control(control, caps) {
            Ok(()) => vec![Update::Info(summary)],
            Err(error) => vec![Update::Error(Error::from(error).to_string())],
        }
    }
}
