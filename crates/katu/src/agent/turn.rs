//! Execução de um turno (E12-T05/E10): pedido ao provider, observação efémera e tools §42.

use katu_core::error::ToolOutcome;
use katu_core::kernel::CallId;
use katu_policy::ApprovalRequest;
use serde_json::Value;

use super::{AgentError, CallOutcome, Ports, TurnReport, TurnRequest, execute_call};
use crate::runtime::Runtime;

mod request;
mod run;

pub(crate) use run::run_turn_with;

/// Evento **efémero** do turno (E10-T05): observação ao vivo, fora do log e do contexto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Activity<'a> {
    /// Delta de texto do assistente.
    Text(&'a str),
    /// Delta de raciocínio.
    Thinking(&'a str),
    /// Tool pedida pelo modelo.
    Tool {
        /// Nome ao modelo da tool.
        name: &'a str,
        /// Argumentos **crus** enviados pelo modelo (transparência/diagnóstico).
        args: &'a str,
    },
    /// Tool concluída (sucesso/parcial).
    ToolDone {
        /// Nome ao modelo da tool.
        name: &'a str,
    },
    /// Recusa determinística com a regra e a evidência (E10-T04).
    Refused {
        /// Nome ao modelo da tool.
        name: &'a str,
        /// Regra que negou.
        rule: &'a str,
        /// Evidência (argumento concreto).
        evidence: &'a str,
    },
    /// Tool indisponível por falta de um controlo (ex.: aprovação, E10-T04).
    Unavailable {
        /// Nome ao modelo da tool.
        name: &'a str,
        /// Controlo em falta.
        control: &'a str,
    },
}

/// Consumidor de atividade efémera (implementado pela borda/TUI).
pub(crate) trait ActivitySink {
    /// Recebe um evento efémero.
    fn activity(&mut self, activity: Activity<'_>);

    /// Pede **aprovação humana** (challenge-and-response, §33). Devolve o override assinado ou
    /// `None` (fail-closed: sem resposta, a recusa mantém-se). Por omissão, não aprova nada.
    fn approve(&mut self, _prompt: &ApprovalPrompt<'_>) -> Option<Approval> {
        None
    }

    /// `true` se o utilizador pediu para **cancelar** o turno (cancelamento cooperativo).
    ///
    /// O loop verifica-o em cada fronteira (antes de cada passo e de cada tool call) e o sink do
    /// provider devolve [`Flow::Break`] quando cancelado. Por omissão, nunca cancela.
    fn cancelled(&self) -> bool {
        false
    }

    /// Prompt de *steering* empilhado durante o turno (E20-T16); `None` se não há.
    ///
    /// O loop consulta-o **entre passos** e injeta-o como mensagem de utilizador no passo seguinte.
    fn steer(&mut self) -> Option<String> {
        None
    }
}

/// Pedido de aprovação apresentado ao humano (E07-T05, §33).
pub(crate) struct ApprovalPrompt<'a> {
    /// Nome ao modelo da tool.
    pub tool: &'a str,
    /// Pedido da política (regra, motivo e âmbito).
    pub request: &'a ApprovalRequest,
}

/// Aprovação **assinada** por humano (`override_reason` + `granted_by`).
pub(crate) struct Approval {
    /// Justificação (`override_reason`); não pode ser vazia.
    pub reason: String,
    /// Quem assinou (`granted_by`); não pode ser vazio.
    pub granted_by: String,
}

/// Observador que ignora tudo (`katu run`).
struct NoActivity;

impl ActivitySink for NoActivity {
    fn activity(&mut self, _activity: Activity<'_>) {}
}

/// Executa um turno completo sem observador externo.
///
/// # Errors
/// [`AgentError`] em falha do provider, da sessão ou do roteamento (fail-closed).
pub(crate) fn run_turn(
    runtime: &mut Runtime<'_>,
    request: TurnRequest<'_>,
) -> Result<TurnReport, AgentError> {
    run_turn_with(runtime, request, &mut NoActivity)
}

/// Executa as tool calls de um passo pela ordem §42, com o caminho de aprovação (E07-T05).
///
/// Devolve `false` se o utilizador cancelou a meio (o chamador fecha o turno).
fn run_calls(
    runtime: &mut Runtime<'_>,
    ports: &Ports<'_>,
    calls: Vec<(CallId, String, Value)>,
    activity: &mut dyn ActivitySink,
) -> Result<bool, AgentError> {
    for (call, name, arguments) in calls {
        if activity.cancelled() {
            return Ok(false);
        }
        let mut outcome = execute_call(runtime, ports, call.clone(), &name, &arguments)?;
        emit_outcome(activity, &name, &outcome.outcome);
        retry_with_approval(
            runtime,
            ports,
            &mut outcome,
            &call,
            &name,
            &arguments,
            activity,
        )?;
    }
    Ok(true)
}

/// Reencaminha o resultado de uma tool ao observador: sucesso, recusa ou indisponibilidade
/// (E10-T04). Uma recusa **não** mostra "concluída".
pub(super) fn emit_outcome(activity: &mut dyn ActivitySink, name: &str, outcome: &ToolOutcome) {
    match outcome {
        ToolOutcome::Denied { rule_id, evidence } => {
            activity.activity(Activity::Refused {
                name,
                rule: rule_id.as_str(),
                evidence: evidence.argument.as_str(),
            });
        }
        ToolOutcome::Unavailable { control, .. } => {
            activity.activity(Activity::Unavailable {
                name,
                control: control.as_str(),
            });
        }
        _ => activity.activity(Activity::ToolDone { name }),
    }
}

/// Re-executa uma chamada recusada por falta de aprovação.
///
/// Se a política pediu aprovação, pede-a ao humano e, com uma assinatura e a capacidade mínima
/// derivada, **re-executa** a chamada (novo `CallId`, ordem §42 mantida). Sem assinatura, sem
/// capacidade derivável ou sem uso resolvido, a recusa mantém-se (fail-closed).
#[allow(
    clippy::too_many_arguments,
    reason = "o retry reutiliza o contexto do turno sem o esconder num struct de vida curta"
)]
fn retry_with_approval(
    runtime: &mut Runtime<'_>,
    ports: &Ports<'_>,
    outcome: &mut CallOutcome,
    call: &CallId,
    name: &str,
    arguments: &Value,
    activity: &mut dyn ActivitySink,
) -> Result<(), AgentError> {
    let (Some(request), Some(use_)) = (outcome.approval.clone(), outcome.use_.clone()) else {
        return Ok(());
    };
    let prompt = ApprovalPrompt {
        tool: name,
        request: &request,
    };
    let Some(approval) = activity.approve(&prompt) else {
        return Ok(());
    };
    let Some(capability) = katu_policy::capability_for_request(&use_, &runtime.rules, &request)
    else {
        return Ok(());
    };
    runtime.session.approve(
        request.rule_id.clone(),
        capability,
        &approval.reason,
        &approval.granted_by,
    )?;
    let retry = CallId::new(format!("{}#approved", call.as_str()));
    *outcome = execute_call(runtime, ports, retry, name, arguments)?;
    emit_outcome(activity, name, &outcome.outcome);
    Ok(())
}
