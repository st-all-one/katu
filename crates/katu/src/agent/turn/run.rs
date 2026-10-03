//! Loop de passos do turno (E12-T05/E10): provider → sink efémero → tools §42.
//!
//! Vive num módulo filho para manter `turn.rs` sob o teto de linhas. O sink acumula o texto e as
//! tool calls de cada passo e reencaminha a atividade **efémera** (deltas, tools, argumentos crus);
//! nada disto entra no log. O cancelamento cooperativo fecha o turno de forma limpa.

use std::sync::Arc;

use katu_core::context::SelectionPolicy;
use katu_core::diag::{Level, events};
use katu_core::kernel::{CallId, Guard};
use katu_core::ports::Cancel;
use katu_core::provider::{
    Provider, ProviderError, ProviderRequest, StopReason, TokenUsage, ToolDef,
};
use serde_json::Value;

use super::declared;
use super::looping::cut_if_looping;
use super::request::build_request;
use super::sink::TurnSink;
use super::stream::drain_stream;
use super::{ActivitySink, run_calls};
use crate::agent::{
    AgentError, Ports, Termination, TurnOptions, TurnReport, TurnRequest, catalog, settle_truncated,
};
use crate::defaults;
use crate::runtime::Runtime;

/// Executa um turno completo, reencaminhando atividade efémera ao `activity` (E10-T05).
///
/// # Errors
/// [`AgentError`] em falha do provider, da sessão ou do roteamento (fail-closed).
pub(crate) fn run_turn_with(
    runtime: &mut Runtime<'_>,
    request: TurnRequest<'_>,
    activity: &mut dyn ActivitySink,
) -> Result<TurnReport, AgentError> {
    let _span = katu_core::trace_fn!("agent::turn::run::run_turn_with");

    let TurnRequest {
        provider,
        ports,
        goal,
        options,
        cancel,
    } = request;
    let _span = katu_core::fn_span!(Level::Info, events::KERNEL_TURN, "run::run_turn_with");
    if !runtime.session.state().turn_open {
        runtime.begin_turn()?;
    }
    runtime.record_user(goal)?;
    // Q-04: a secção `estado` do turno entra no log **antes** de qualquer pedido ao provider, pelo
    // que o prompt de sistema que o modelo vê é reconstruível do log (E04).
    runtime.record_prompt_state(options.max_steps)?;
    // Q-12: o turno fecha **sempre**, mesmo quando o loop guard corta a meio (ou o teto de passos
    // estoura): um `TurnStart` sem `TurnEnd` deixaria o log inconsistente para a retomada.
    let driven = drive(runtime, &ports, &provider, options, activity, cancel);
    // L-S1: `record_turn_end` é o **único** ponto que fecha o turno (normal, erro, cancel, teto),
    // já com a reconciliação de calls pendentes (L-Q1).
    match driven {
        Ok(accum) => finish(runtime, accum),
        Err(error) => {
            let turn = runtime.turn();
            runtime.record_turn_end(turn)?;
            Err(error)
        }
    }
}

/// Retries de uma resposta vazia antes de fechar com mensagem visível (L-Q3).
const MAX_EMPTY_RETRIES: u8 = 2;

/// Retries de uma resposta que ecoa o `delta` de uma tool (L-Q4).
const MAX_ECHO_RETRIES: u8 = 1;

/// Nota visível quando o eco persiste após o orçamento de retries (G5): o eco **nunca** é aceite.
const ECHO_NOTICE: &str =
    "(o modelo repetiu o resultado de uma tool em vez de responder; resposta substituída)";

/// Corre o loop de passos até ao fim (natural, cancelado, vazio, teto ou loop) e devolve o
/// acumulado com o motivo de terminação (L-Q3).
#[allow(
    clippy::too_many_lines,
    clippy::too_many_arguments,
    reason = "o controlo do turno (fronteiras, truncagem, eco, vazio, guard, teto) vive junto para a ordem das decisões ser legível; o provider e o cancelamento são eixos distintos do mesmo passo"
)]
fn drive(
    runtime: &mut Runtime<'_>,
    ports: &Ports<'_>,
    provider: &Arc<dyn Provider>,
    options: &TurnOptions,
    activity: &mut dyn ActivitySink,
    cancel: Option<&dyn Cancel>,
) -> Result<Accum, AgentError> {
    let _span = katu_core::trace_fn!("agent::turn::run::drive");

    let tools = catalog::tool_defs();
    let mut guard = Guard::with_defaults();
    // A3/W8-4: o gate só é **seguro** com a seleção `suffix` — a `utility` pode descartar a unidade
    // lida e o gate passaria a dizer “já presente no contexto” sem o estar. `behavior.tool_voi`
    // fica assim limitado a quando o resultado lido está garantidamente no contexto.
    let mut voi = super::voi::Voi::new();
    let voi_enabled = defaults::from_root(runtime.root())
        .tool_voi
        .unwrap_or(false)
        && runtime.selection() == SelectionPolicy::Suffix;
    let mut accum = Accum::new();
    loop {
        if activity.cancelled() {
            accum.cancelled = true;
            accum.termination = Termination::Cancelled;
            return Ok(accum);
        }
        accum.steps = accum.steps.saturating_add(1);
        let request = build_request(runtime, options, &tools)?;
        let step = stream_step(
            runtime,
            provider,
            &request,
            activity,
            &tools,
            options.idle_ms,
        )?;
        accum.usage = step.usage.or(accum.usage);
        accum.text.push_str(&step.text);
        accum.calls = accum.calls.saturating_add(step.calls.len());
        accum.stop = step.stop.clone();
        if step.cancelled {
            accum.cancelled = true;
            accum.termination = Termination::Cancelled;
            return Ok(accum);
        }
        // L-Q2: tool calls truncadas fecham sem executar; o modelo reformula no passo seguinte.
        let truncated = !step.truncated.is_empty();
        if truncated {
            settle_truncated(runtime, step.truncated)?;
        }
        if step.calls.is_empty() && !truncated {
            // L-Q4/G5: eco do delta de uma tool na resposta final → nudge + retry (1×); no
            // orçamento esgotado o eco **nunca** é aceite — é substituído por nota visível.
            if super::echo::echoes_recent_delta(runtime, &step.text)? {
                accum
                    .text
                    .truncate(accum.text.len().saturating_sub(step.text.len()));
                if accum.echo_retries < MAX_ECHO_RETRIES {
                    accum.echo_retries = accum.echo_retries.saturating_add(1);
                    nudge(
                        runtime,
                        events::AGENT_ECHO,
                        "eco",
                        "a tua resposta repetiu o resultado de uma tool; devolve a resposta final em \
                         texto próprio",
                    )?;
                    continue;
                }
                accum.text.push_str(ECHO_NOTICE);
                accum.termination = Termination::Natural;
                return Ok(accum);
            }
            // L-Q3: resposta vazia → retry limitado; no limite, mensagem visível no fecho.
            // Uma truncagem (`length`) ou filtro de conteúdo não é retentável: fecha com o aviso
            // próprio (L-Q2) em vez de gastar passos a insistir.
            if accum.text.trim().is_empty()
                && !matches!(step.stop, StopReason::Length | StopReason::ContentFilter)
            {
                if accum.empty_retries < MAX_EMPTY_RETRIES {
                    accum.empty_retries = accum.empty_retries.saturating_add(1);
                    nudge(
                        runtime,
                        events::AGENT_EMPTY,
                        "vazio",
                        "não devolveste texto; devolve a resposta final em texto (sem raciocínio)",
                    )?;
                    continue;
                }
                accum.termination = Termination::Empty;
                return Ok(accum);
            }
            accum.termination = Termination::Natural;
            return Ok(accum);
        }
        // Q-12/F7: o guard observa as chamadas do passo **antes** de as executar (corta primeiro).
        if let Some(termination) = cut_if_looping(&mut guard, &step.calls) {
            accum.termination = termination;
            return Ok(accum);
        }
        // A3/W8-4: o gate de VOI não repete uma só-leitura já satisfeita (nunca o irreconstruível).
        let calls = if voi_enabled {
            super::voi::apply(runtime, ports, &mut voi, step.calls)?
        } else {
            step.calls
        };
        if !run_calls(runtime, ports, cancel, calls, activity)? {
            accum.cancelled = true;
            accum.termination = Termination::Cancelled;
            return Ok(accum);
        }
        if let Some(prompt) = activity.steer() {
            runtime.record_user(&prompt)?;
            katu_core::event!(
                Level::Info,
                events::TUI_STEER,
                "chars" => prompt.chars().count()
            );
        }
        if accum.steps >= options.max_steps {
            accum.termination = Termination::MaxSteps { steps: accum.steps };
            return Ok(accum);
        }
    }
}

/// Regista um *nudge* **agent-only** para o modelo corrigir (L-Q3/L-Q4).
///
/// Entra no log como mensagem de utilizador (é a única via model-visible) e emite o evento de
/// diagnóstico do motivo; nunca é apresentado como resposta final.
#[cfg_attr(
    not(feature = "profile"),
    allow(
        unused_variables,
        reason = "o macro no-op ignora os campos (custo zero)"
    )
)]
fn nudge(
    runtime: &mut Runtime<'_>,
    event: &'static str,
    reason: &'static str,
    message: &str,
) -> Result<(), AgentError> {
    let _span = katu_core::fn_span!(Level::Debug, events::AGENT_EMPTY, "run::nudge");
    runtime.record_user(message)?;
    katu_core::event!(Level::Debug, event, "reason" => reason);
    Ok(())
}

/// Resultado de um passo de streaming (texto, tool calls e motivo de paragem).
struct Step {
    text: String,
    calls: Vec<(CallId, String, Value)>,
    truncated: Vec<(CallId, String)>,
    usage: Option<TokenUsage>,
    stop: StopReason,
    cancelled: bool,
}

/// Corre um passo (uma chamada ao provider), tratando o cancelamento como fim **limpo**.
///
/// Um [`ProviderError::Cancelled`] (o sink devolveu [`Flow::Break`]) regista o texto parcial e
/// devolve `cancelled: true`; nunca é um erro do turno.
#[allow(
    clippy::too_many_arguments,
    reason = "o passo recebe o runtime, o provider, o pedido, o observador, o catálogo e o teto de inatividade — eixos distintos, não um struct de vida curta"
)]
fn stream_step(
    runtime: &mut Runtime<'_>,
    provider: &Arc<dyn Provider>,
    request: &ProviderRequest,
    activity: &mut dyn ActivitySink,
    tools: &[ToolDef],
    idle_ms: u64,
) -> Result<Step, AgentError> {
    let _span = katu_core::trace_fn!("agent::turn::run::stream_step");

    let mut sink = TurnSink {
        activity: &mut *activity,
        text: String::new(),
        calls: Vec::new(),
        truncated: Vec::new(),
    };
    let outcome = match drain_stream(provider, request, &mut sink, idle_ms) {
        Ok(outcome) => outcome,
        Err(ProviderError::Cancelled) => {
            let TurnSink { text, calls, .. } = sink;
            if !text.is_empty() {
                runtime.record_assistant(&text)?;
            }
            return Ok(Step {
                text,
                calls,
                truncated: Vec::new(),
                usage: None,
                stop: StopReason::EndTurn,
                cancelled: true,
            });
        }
        Err(error) => return Err(error.into()),
    };
    let TurnSink {
        text,
        calls,
        truncated,
        ..
    } = sink;
    // W8-1b: um modelo local sem tool calls nativas declara a chamada no `content`. Sem este
    // degrau o passo termina sem chamadas e o JSON cru vira resposta final (`calls = 0`). A
    // decodificação só atua quando **não** houve chamadas nativas e o nome está no catálogo.
    let (text, calls) = if calls.is_empty() {
        match declared::extract(&text, tools) {
            Some(declared) => (declared.remaining, declared.calls),
            None => (text, calls),
        }
    } else {
        (text, calls)
    };
    if !text.is_empty() {
        runtime.record_assistant(&text)?;
    }
    Ok(Step {
        text,
        calls,
        truncated,
        usage: outcome.usage,
        stop: outcome.stop,
        cancelled: false,
    })
}

/// Acumulado do turno até ao fecho, com o motivo de terminação (L-Q3).
struct Accum {
    text: String,
    calls: usize,
    usage: Option<TokenUsage>,
    steps: u32,
    cancelled: bool,
    stop: StopReason,
    termination: Termination,
    empty_retries: u8,
    echo_retries: u8,
}

impl Accum {
    /// Acumulado vazio com fim natural por omissão.
    fn new() -> Self {
        let _span = katu_core::trace_fn!("agent::turn::run::Accum::new");

        Self {
            text: String::new(),
            calls: 0,
            usage: None,
            steps: 0,
            cancelled: false,
            stop: StopReason::EndTurn,
            termination: Termination::Natural,
            empty_retries: 0,
            echo_retries: 0,
        }
    }
}

/// Fecha o turno e devolve o relatório (comum a todos os finais).
///
/// L-Q3: um fim **anormal** (vazio, teto ou loop) acrescenta uma mensagem do assistente visível ao
/// utilizador — o motivo continua no envelope de máquina, nunca em silêncio.
fn finish(runtime: &mut Runtime<'_>, accum: Accum) -> Result<TurnReport, AgentError> {
    let _span = katu_core::fn_span!(Level::Debug, events::KERNEL_STOP, "run::finish");
    // L-Q2: o motivo de paragem do provider e a terminação ficam no diagnóstico (um facto, um id).
    katu_core::event!(
        Level::Debug,
        events::AGENT_STOP,
        "stop" => crate::agent::stop_label(&accum.stop),
        "termination" => accum.termination.as_str(),
        "steps" => accum.steps
    );
    let turn = runtime.turn();
    let mut text = accum.text;
    // L-Q3: fim anormal (vazio/teto/loop) tem mensagem própria; L-Q2: `length`/`content_filter`
    // sem outra terminação acrescenta o aviso correspondente. A mensagem é logada (visível).
    let note = accum.termination.message().or_else(|| match accum.stop {
        StopReason::Length => Some(
            "a resposta foi truncada pelo teto de tokens de saída; aumenta `--max-tokens` ou divide \
             o pedido"
                .to_string(),
        ),
        StopReason::ContentFilter => Some(
            "a resposta foi interrompida pelo filtro de conteúdo do provider".to_string(),
        ),
        _ => None,
    });
    if let Some(message) = note {
        runtime.record_assistant(&message)?;
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        text.push_str(&message);
    }
    runtime.record_turn_end(turn)?;
    Ok(TurnReport {
        steps: accum.steps,
        text,
        calls: accum.calls,
        usage: accum.usage,
        cancelled: accum.cancelled,
        stop: accum.stop,
        termination: accum.termination,
    })
}
