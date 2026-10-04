//! Controlo de modelo/pensamento no kernel (E12-T10): valida contra o catálogo e regista no log.

use katu_core::api::Event as Update;
use katu_core::diag::{Level, events};
use katu_core::error::Error;
use katu_core::kernel::Control;
use katu_core::provider::{ModelCapabilities, Provider, Thinking};

use super::Kernel;

/// Graus de pensamento suportados pelo modelo (E20-T10): `[off]` se não raciocina.
pub(crate) fn thinking_options(provider: &dyn Provider, model: &str) -> Vec<Thinking> {
    let _span = katu_core::trace_fn!("kernel::control::thinking_options");

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

/// Modelos oferecidos no seletor da TUI (E12-T02/T10): do **endpoint**, com queda no catálogo.
///
/// Tenta a descoberta ao vivo (`dynamic_models`); se falhar ou vier vazia, usa o catálogo estático.
/// O default vem primeiro para o índice zero coincidir com o modelo do arranque.
pub(crate) fn models_for(provider: &dyn Provider, default: &str) -> Vec<String> {
    let _span = katu_core::trace_fn!("kernel::control::models_for");

    let discovered = provider.dynamic_models().unwrap_or_default();
    katu_core::event!(
        Level::Debug,
        events::PROVIDER_MODELS,
        "source" => if discovered.is_empty() { "catalog" } else { "endpoint" },
        "count" => discovered.len(),
    );
    let listed = if discovered.is_empty() {
        provider.models()
    } else {
        discovered
    };
    let mut models: Vec<String> = listed
        .into_iter()
        .filter(|model| model.as_str() != default)
        .collect();
    models.insert(0, default.to_string());
    models
}

impl Kernel<'_> {
    /// Define o modelo ativo (validado contra o catálogo; erro que ensina).
    ///
    /// Devolve também os graus de pensamento do novo modelo (E20-T10), para o menu se adaptar.
    pub(super) fn set_model(&mut self, model: String) -> Vec<Update> {
        let _span = katu_core::trace_fn!("kernel::control::set_model");

        let options = thinking_options(self.provider.as_ref(), &model);
        let caps = self.provider.capabilities(&model);
        let mut updates = self.apply_control(&Control::SetModel { model }, &caps);
        updates.push(Update::ThinkingOptions(options));
        updates
    }

    /// Define o grau de pensamento do modelo **ativo** (o agente nunca se auto-escala).
    pub(super) fn set_thinking(&mut self, thinking: Thinking) -> Vec<Update> {
        let _span = katu_core::trace_fn!("kernel::control::set_thinking");

        let current = self.runtime.control();
        let model = current.model.unwrap_or_else(|| self.model.model.clone());
        let caps = self.provider.capabilities(&model);
        self.apply_control(&Control::SetThinking { thinking }, &caps)
    }

    /// Aplica o controlo no runtime (logado) e traduz em `Update`.
    fn apply_control(&mut self, control: &Control, caps: &ModelCapabilities) -> Vec<Update> {
        let _span = katu_core::trace_fn!("kernel::control::apply_control");

        let summary = control.summary();
        match self.runtime.set_control(control, caps) {
            Ok(()) => vec![Update::Info(summary)],
            Err(error) => vec![Update::Error(Error::from(error).to_string())],
        }
    }
}
