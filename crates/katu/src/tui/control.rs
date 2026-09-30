//! Controlo de modelo/pensamento na TUI (E12-T10): valida contra o catálogo e regista no kernel.

use katu_core::error::Error;
use katu_core::kernel::Control;
use katu_core::provider::{ModelCapabilities, Thinking};
use katu_tui::Update;

use super::AgentHandler;

impl AgentHandler<'_> {
    /// Define o modelo ativo (validado contra o catálogo; erro que ensina).
    pub(super) fn set_model(&mut self, model: String) -> Vec<Update> {
        let caps = self.provider.capabilities(&model);
        self.apply_control(&Control::SetModel { model }, &caps)
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
