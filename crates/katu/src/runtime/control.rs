//! Controlo de modelo/pensamento no runtime (E12-T10): evento no log, estado autoritativo.

use katu_core::kernel::{Control, ControlState, Event};
use katu_core::provider::ModelCapabilities;

use super::{Runtime, RuntimeError};

impl Runtime<'_> {
    /// Estado do controlo (E12-T10): modelo ativo + grau de pensamento.
    #[must_use]
    pub(crate) fn control(&self) -> ControlState {
        self.session.state().control.clone()
    }

    /// Aplica um controlo do **utilizador**, validado contra o catálogo (E12-T10).
    ///
    /// O evento fica no log (auditável e sobrevive a *resume*); o agente nunca chega aqui.
    ///
    /// # Errors
    /// [`RuntimeError::Control`] se o grau de pensamento não servir o modelo (erro que ensina);
    /// [`RuntimeError::Session`] se o evento não puder ser logado.
    pub(crate) fn set_control(
        &mut self,
        control: &Control,
        caps: &ModelCapabilities,
    ) -> Result<(), RuntimeError> {
        control.validate(&self.session.state().control, caps)?;
        self.session.apply(&Event::Control {
            control: control.clone(),
        })?;
        Ok(())
    }
}
