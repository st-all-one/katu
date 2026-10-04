//! Execução de um turno (E12-T05/E10): pedido ao provider, observação efémera e tools §42.

use katu_core::diag::{Level, events};
use katu_core::error::ToolOutcome;
use katu_core::kernel::CallId;
use katu_core::ports::{Cancel, Progress};
use katu_policy::ApprovalRequest;
use katu_tools::schema::concurrency_of;
use serde_json::Value;

use super::{AgentError, CallOutcome, Ports, execute_call};
#[cfg(test)]
use super::{TurnReport, TurnRequest};
use crate::defaults;
use crate::runtime::Runtime;

mod batch;
mod declared;
mod echo;
mod looping;
mod request;
mod run;
mod sink;
mod stream;
pub(crate) mod voi;

#[cfg(test)]
pub(crate) use batch::PARALLEL_BATCHES;
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
    /// Tool concluída (sucesso/parcial), com um **resumo** do resultado (`LIVE_FLOW` LF4).
    ToolDone {
        /// Nome ao modelo da tool.
        name: &'a str,
        /// Resumo de uma linha do resultado (vazio quando não há envelope).
        summary: &'a str,
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

    /// Oportunidade de **sondar input** sem haver delta (L-P1).
    ///
    /// O loop chama-o a cada `STREAM_POLL_MS` enquanto o provider não emite nada, pelo que
    /// `Esc`/`Ctrl-C` são vistos mesmo com o stream parado. Por omissão, não faz nada.
    fn tick(&mut self) {
        let _span = katu_core::trace_fn!("agent::turn::tick");
    }

    /// Pede **aprovação humana** (challenge-and-response, §33). Devolve o override assinado ou
    /// `None` (fail-closed: sem resposta, a recusa mantém-se). Por omissão, não aprova nada.
    fn approve(&mut self, _prompt: &ApprovalPrompt<'_>) -> Option<Approval> {
        let _span = katu_core::trace_fn!("agent::turn::approve");

        None
    }

    /// `true` se o utilizador pediu para **cancelar** o turno (cancelamento cooperativo).
    ///
    /// O loop verifica-o em cada fronteira (antes de cada passo e de cada tool call) e o sink do
    /// provider devolve [`Flow::Break`] quando cancelado. Por omissão, nunca cancela.
    fn cancelled(&self) -> bool {
        let _span = katu_core::trace_fn!("agent::turn::cancelled");

        false
    }

    /// Prompt de *steering* empilhado durante o turno (E20-T16); `None` se não há.
    ///
    /// O loop consulta-o **entre passos** e injeta-o como mensagem de utilizador no passo seguinte.
    fn steer(&mut self) -> Option<String> {
        let _span = katu_core::trace_fn!("agent::turn::steer");

        None
    }

    /// `true` se a superfície pediu **um** passo extra antes do fim natural (`S1/PI_GAINS`).
    ///
    /// One-shot: consumido na leitura (um pedido vale um passo). Por omissão, nunca.
    fn continue_once(&mut self) -> bool {
        let _span = katu_core::trace_fn!("agent::turn::continue_once");

        false
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

/// Observador que ignora tudo (usado pelos testes do turno).
#[cfg(test)]
struct NoActivity;

#[cfg(test)]
impl ActivitySink for NoActivity {
    fn activity(&mut self, _activity: Activity<'_>) {
        let _span = katu_core::trace_fn!("agent::turn::activity");
    }
}

/// Executa um turno completo sem observador externo (conveniência dos testes).
///
/// # Errors
/// [`AgentError`] em falha do provider, da sessão ou do roteamento (fail-closed).
#[cfg(test)]
pub(crate) fn run_turn(
    runtime: &mut Runtime<'_>,
    request: TurnRequest<'_>,
) -> Result<TurnReport, AgentError> {
    let _span = katu_core::trace_fn!("agent::turn::run_turn");

    run_turn_with(runtime, request, &mut NoActivity)
}

/// Desfecho da execução das tool calls de um passo (`Q1/PI_GAINS`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CallsOutcome {
    /// Segue para o próximo passo (comportamento normal).
    Continue,
    /// **Todas** as calls do passo pediram o fim normal do turno.
    Terminate,
    /// O utilizador cancelou a meio.
    Cancelled,
}

/// Executa as tool calls de um passo pela ordem §42, com o caminho de aprovação (E07-T05).
///
/// As calls **consecutivas** classificadas `Shared` (só-leitura) formam um lote: são preparadas
/// pela ordem do modelo, executadas num pool limitado e cometidas nessa ordem (B-01/B-02). Uma
/// call exclusiva **esvazia** o lote antes de correr — é uma barreira, como no contrato do PTC.
///
/// `Q1/PI_GAINS`: se **todas** as calls do passo declararem `terminate`, o passo termina o turno (a
/// regra "todas" espelha o `shouldTerminateToolBatch` do pi).
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "as tool calls de um passo (runtime, portas, cancelamento, progresso, calls, observador) e o seu desfecho terminal vivem juntos para a ordem §42 ser legível"
)]
fn run_calls(
    runtime: &mut Runtime<'_>,
    ports: &Ports<'_>,
    cancel: Option<&dyn Cancel>,
    progress: &dyn Progress,
    calls: Vec<(CallId, String, Value)>,
    activity: &mut dyn ActivitySink,
) -> Result<CallsOutcome, AgentError> {
    let _span = katu_core::trace_fn!("agent::turn::run_calls");

    // Um passo sem calls (ex.: só calls truncadas) não termina o turno (`Q1/PI_GAINS`).
    if calls.is_empty() {
        return Ok(CallsOutcome::Continue);
    }
    let mut batch: Vec<(CallId, String, Value)> = Vec::new();
    let mut all_terminate = true;
    for (call, name, arguments) in calls {
        if activity.cancelled() {
            return Ok(CallsOutcome::Cancelled);
        }
        if concurrency_of(&name).is_shared() {
            batch.push((call, name, arguments));
            if batch.len() >= batch::MAX_PARALLEL_CALLS {
                all_terminate &= batch::run_shared(
                    runtime,
                    ports,
                    progress,
                    std::mem::take(&mut batch),
                    activity,
                )?;
                if activity.cancelled() {
                    return Ok(CallsOutcome::Cancelled);
                }
            }
            continue;
        }
        if !batch.is_empty() {
            all_terminate &= batch::run_shared(
                runtime,
                ports,
                progress,
                std::mem::take(&mut batch),
                activity,
            )?;
            if activity.cancelled() {
                return Ok(CallsOutcome::Cancelled);
            }
        }
        let mut outcome = execute_call(
            runtime,
            ports,
            cancel,
            progress,
            call.clone(),
            &name,
            &arguments,
        )?;
        emit_outcome(activity, &name, &outcome.outcome, outcome.delta.as_deref());
        retry_with_approval(
            runtime,
            ports,
            cancel,
            progress,
            &mut outcome,
            &call,
            &name,
            &arguments,
            activity,
        )?;
        all_terminate &= outcome.terminate;
    }
    if !batch.is_empty() {
        all_terminate &= batch::run_shared(runtime, ports, progress, batch, activity)?;
    }
    Ok(if all_terminate {
        CallsOutcome::Terminate
    } else {
        CallsOutcome::Continue
    })
}

/// Reencaminha o resultado de uma tool ao observador: sucesso, recusa ou indisponibilidade
/// (E10-T04). Uma recusa **não** mostra "concluída".
pub(super) fn emit_outcome(
    activity: &mut dyn ActivitySink,
    name: &str,
    outcome: &ToolOutcome,
    delta: Option<&str>,
) {
    let _span = katu_core::trace_fn!("agent::turn::emit_outcome");

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
        _ => {
            // LF4: o painel mostra uma linha do resultado, não o envelope inteiro.
            let summary = summarize(delta);
            activity.activity(Activity::ToolDone {
                name,
                summary: &summary,
            });
        }
    }
}

/// Teto do resumo de uma tool no painel efémero (uma linha legível).
const MAX_SUMMARY_CHARS: usize = 200;

/// Resume o `delta` de uma tool numa linha curta (`LIVE_FLOW` LF4).
pub(super) fn summarize(delta: Option<&str>) -> String {
    let _span = katu_core::trace_fn!("agent::turn::summarize");

    let Some(delta) = delta else {
        return String::new();
    };
    let first = delta.lines().next().unwrap_or_default().trim();
    if first.chars().count() <= MAX_SUMMARY_CHARS {
        return first.to_string();
    }
    let head: String = first.chars().take(MAX_SUMMARY_CHARS).collect();
    format!("{head}…")
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
    cancel: Option<&dyn Cancel>,
    progress: &dyn Progress,
    outcome: &mut CallOutcome,
    call: &CallId,
    name: &str,
    arguments: &Value,
    activity: &mut dyn ActivitySink,
) -> Result<(), AgentError> {
    let _span = katu_core::fn_span!(
        Level::Debug,
        events::POLICY_CAPABILITY,
        "turn::retry_with_approval"
    );
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
    // S-03: a política é instrumentada pelo chamador (firewall); o rótulo atribui o custo a
    // `policy::capability_for` dentro do span de `retry_with_approval`.
    let capability = {
        let _span = katu_core::fn_span!(
            Level::Trace,
            events::POLICY_CAPABILITY,
            "policy::capability_for"
        );
        katu_policy::capability_for_request(&use_, &runtime.rules, &request)
    };
    let Some(capability) = capability else {
        return Ok(());
    };
    // D3: a aprovação exige a chave MAC (fail-closed sem ela).
    let mac_key = defaults::from_root(runtime.root())
        .mac_key
        .unwrap_or_default();
    runtime.session.approve(
        request.rule_id.clone(),
        capability.clone(),
        &approval.reason,
        &approval.granted_by,
        &mac_key,
    )?;
    let retry = CallId::new(format!("{}#approved", call.as_str()));
    *outcome = execute_call(runtime, ports, cancel, progress, retry, name, arguments)?;
    emit_outcome(activity, name, &outcome.outcome, outcome.delta.as_deref());
    // B-06: a aprovação é **one-shot** — depois de usada, a capacidade é revogada. A próxima
    // escalação exige nova aprovação (não é herdada).
    runtime.session.revoke_approval(&capability)?;
    Ok(())
}
