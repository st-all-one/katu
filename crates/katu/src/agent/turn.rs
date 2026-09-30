//! Execução de um turno (E12-T05/E10): pedido ao provider, observação efémera e tools §42.

use katu_core::diag::{Level, events};
use katu_core::error::ToolOutcome;
use katu_core::kernel::CallId;
use katu_core::provider::{Flow, ProviderEvent, ProviderSink};
use katu_policy::ApprovalRequest;
use serde_json::Value;

use super::{AgentError, CallOutcome, Ports, TurnReport, TurnRequest, catalog, execute_call};
use crate::runtime::Runtime;

mod request;

use request::build_request;

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

/// Sink que acumula o turno e reencaminha a atividade efémera.
struct TurnSink<'a> {
    activity: &'a mut dyn ActivitySink,
    text: String,
    calls: Vec<(CallId, String, Value)>,
}

impl ProviderSink for TurnSink<'_> {
    fn on_event(&mut self, event: ProviderEvent) -> Flow {
        match event {
            ProviderEvent::Text(delta) => {
                self.activity.activity(Activity::Text(&delta));
                self.text.push_str(&delta);
            }
            ProviderEvent::Thinking(delta) => {
                self.activity.activity(Activity::Thinking(&delta));
            }
            ProviderEvent::ToolCall {
                call,
                name,
                arguments,
            } => {
                self.activity.activity(Activity::Tool { name: &name });
                self.calls.push((call, name, arguments));
            }
            _ => {}
        }
        Flow::Continue
    }
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

/// Executa um turno completo, reencaminhando atividade efémera ao `activity` (E10-T05).
///
/// # Errors
/// [`AgentError`] em falha do provider, da sessão ou do roteamento (fail-closed).
pub(crate) fn run_turn_with(
    runtime: &mut Runtime<'_>,
    request: TurnRequest<'_>,
    activity: &mut dyn ActivitySink,
) -> Result<TurnReport, AgentError> {
    let TurnRequest {
        provider,
        ports,
        goal,
        options,
    } = request;
    let _span = katu_core::span!(Level::Info, events::KERNEL_TURN);
    if !runtime.session.state().turn_open {
        runtime.begin_turn()?;
    }
    runtime.record_user(goal)?;
    let tools = catalog::tool_defs();
    let mut text = String::new();
    let mut calls = 0_usize;
    let mut usage = None;
    let mut steps = 0_u32;
    loop {
        steps = steps.saturating_add(1);
        let request = build_request(runtime, options, &tools)?;
        let mut sink = TurnSink {
            activity: &mut *activity,
            text: String::new(),
            calls: Vec::new(),
        };
        let outcome = provider.stream(&request, &mut sink)?;
        usage = outcome.usage.or(usage);
        let TurnSink {
            text: sink_text,
            calls: sink_calls,
            activity: _,
        } = sink;
        if !sink_text.is_empty() {
            runtime.record_assistant(&sink_text)?;
            text.push_str(&sink_text);
        }
        calls = calls.saturating_add(sink_calls.len());
        if sink_calls.is_empty() {
            let turn = runtime.turn();
            runtime.record_turn_end(turn)?;
            return Ok(TurnReport {
                steps,
                text,
                calls,
                usage,
            });
        }
        run_calls(runtime, &ports, sink_calls, activity)?;
        if steps >= options.max_steps {
            return Err(AgentError::TooManySteps { steps });
        }
    }
}

/// Executa as tool calls de um passo pela ordem §42, com o caminho de aprovação (E07-T05).
fn run_calls(
    runtime: &mut Runtime<'_>,
    ports: &Ports<'_>,
    calls: Vec<(CallId, String, Value)>,
    activity: &mut dyn ActivitySink,
) -> Result<(), AgentError> {
    for (call, name, arguments) in calls {
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
    Ok(())
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
