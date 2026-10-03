//! Fecho durável do turno (L-Q1): reconciliação das tool calls pendentes.
//!
//! Invariante **I2**: nenhum `ToolCall` sem `ToolResult` no log. Um turno pode morrer a meio de um
//! lote (erro, cancelamento, teto ou `SIGKILL`); sem reconciliação, a projeção para o modelo
//! ficaria com um par desalinhado e o pedido seguinte levaria uma conversa malformada.
//!
//! [`Session::reconcile_pending`] fecha cada pendente com
//! [`ToolOutcome::Unavailable`] de controlo `interrupted` e um `delta` que **ensina** — nunca
//! executa a tool. A ordem §42 mantém-se: o `ToolResult` fecha o `ToolCall` que já está no log.

use super::{Session, SessionError};
use crate::diag::{Level, events};
use crate::error::ToolOutcome;
use crate::kernel::{CallId, Event};
use katu_policy::{ControlId, ToolName};

impl Session<'_> {
    /// Fecha as tool calls pendentes com um resultado `Unavailable{interrupted}` (L-Q1).
    ///
    /// Chamado no fecho do turno (normal, erro, cancel ou teto) e na retomada de um turno aberto
    /// por um processo morto. Devolve o número de calls reconciliadas — num turno normal (sem
    /// pendentes) é **0** e nenhum evento é acrescentado ao log.
    ///
    /// # Errors
    /// [`SessionError`] se o evento não puder ser logado.
    pub fn reconcile_pending(&mut self) -> Result<usize, SessionError> {
        let _span = crate::fn_span!(
            Level::Debug,
            events::AGENT_RECONCILE,
            "kernel::session::reconcile_pending"
        );
        // BTreeMap: ordem canónica por `CallId`, pelo que a reconciliação é determinística.
        let pending: Vec<(CallId, ToolName)> = self
            .state
            .pending
            .iter()
            .map(|(call, tool)| (call.clone(), tool.name))
            .collect();
        for (call, name) in &pending {
            let outcome = ToolOutcome::Unavailable {
                control: ControlId::new("interrupted"),
                rule_id: None,
            };
            let delta = format!(
                "a tool `{}` foi interrompida antes de concluir (turno fechado); o resultado está \
                 indisponível — repete a chamada se ainda for necessária",
                name.as_str()
            );
            self.apply(&Event::ToolResult {
                call: call.clone(),
                outcome,
                delta: Some(delta),
            })?;
        }
        Ok(pending.len())
    }
}
