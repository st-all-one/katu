//! Execução concorrente de tool calls `Shared` (B-01/B-02, absorvido do contrato do PTC).
//!
//! O contrato que interessa do PTC é: **a apresentação (como o modelo declara trabalho) é
//! ortogonal à autoridade (o que pode fazer), e ambas passam pelo mesmo pipeline**. Aqui só muda
//! *quando* o efeito corre — a política, o log e a §42 mantêm-se idênticos às calls exclusivas.
//!
//! Três fases, para preservar `Model-visible ⟺ logged` e a ordem de commit do modelo:
//!
//! 1. **preparar** (sequencial, ordem do modelo): rota + `Session::begin_call` (loga o pedido);
//! 2. **executar** (paralelo, pool limitado): `pipeline::dispatch` contra um snapshot imutável;
//! 3. **commitar** (sequencial, ordem do modelo): `Session::settle_call` + atividade + *retry*.
//!
//! Fail-closed: só entra aqui o que a classificação declara
//! [`Concurrency::Shared`](katu_tools::schema::Concurrency::Shared); tudo o resto é exclusivo.

use katu_core::diag::{Level, events};
use katu_core::kernel::{CallContext, CallId, Dispatch, Tool, dispatch};
use katu_policy::{Decision, PolicyError, ToolName, ToolUse};
use serde_json::Value;

use super::{emit_outcome, retry_with_approval};
use crate::agent::{ActivitySink, AgentError, CallOutcome, Ports, router};
use crate::runtime::Runtime;

/// Teto de calls `Shared` num lote paralelo (absorvido do PTC: pool limitado, ≤ 10).
pub(super) const MAX_PARALLEL_CALLS: usize = 8;

/// Contador de lotes efetivamente corridos em paralelo (verificação de teste; B-01).
#[cfg(test)]
pub(crate) static PARALLEL_BATCHES: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Uma call `Shared` já roteada, à espera de efeito.
struct Prepared<'a> {
    call: CallId,
    name: String,
    arguments: Value,
    use_: ToolUse,
    tool: Box<dyn Tool + 'a>,
}

/// Uma call já executada, na ordem do modelo, pronta a cometer.
struct Settled {
    call: CallId,
    name: String,
    arguments: Value,
    use_: ToolUse,
    dispatch: Dispatch,
}

/// Resultado da tentativa paralela.
enum Batch {
    /// Executado em paralelo, pronto a cometer (na ordem do modelo).
    Settled(Vec<Settled>),
    /// O lote não cabe no cost governor: executar sequencialmente (semântica original).
    Fallback(Vec<(CallId, String, Value)>),
}

/// Executa um lote de tool calls `Shared` com um pool limitado (B-01/B-02).
///
/// # Errors
/// [`AgentError`] em falha de roteamento, de orçamento, de política ou de execução (fail-closed).
pub(super) fn run_shared(
    runtime: &mut Runtime<'_>,
    ports: &Ports<'_>,
    calls: Vec<(CallId, String, Value)>,
    activity: &mut dyn ActivitySink,
) -> Result<(), AgentError> {
    let _span = katu_core::fn_span!(
        Level::Debug,
        events::TOOL_CALL,
        "turn::batch::run_shared",
        "calls" => calls.len(),
    );
    // Um lote de uma só call não ganha com threads: segue o caminho sequencial.
    if calls.len() < 2 {
        return run_shared_sequential(runtime, ports, calls, activity);
    }
    match parallel_or_fallback(runtime, ports, calls)? {
        Batch::Settled(settled) => commit(runtime, ports, settled, activity),
        Batch::Fallback(calls) => run_shared_sequential(runtime, ports, calls, activity),
    }
}

/// Fases 1–2 (preparar + executar) para um lote. Devolve dados **próprios**, pelo que os
/// empréstimos do runtime terminam aqui (a fase 3 volta a pedir `&mut Runtime`).
#[allow(
    clippy::too_many_lines,
    reason = "as três fases do lote vivem juntas: separá-las esconderia a ordem §42 que aqui se preserva"
)]
fn parallel_or_fallback(
    runtime: &mut Runtime<'_>,
    ports: &Ports<'_>,
    calls: Vec<(CallId, String, Value)>,
) -> Result<Batch, AgentError> {
    let _span = katu_core::trace_fn!("agent::turn::batch::parallel_or_fallback");

    let root = runtime.root().to_path_buf();
    let loaded = runtime.plan().cloned();
    let now = runtime.clock.now().as_millis();
    let route_ports = router::Ports {
        fs: ports.fs,
        process: ports.process,
        env: ports.env,
        clock: runtime.clock,
        root: &root,
    };

    // Fase 1 — rota **sem** logar, na ordem do modelo.
    let mut routed = Vec::with_capacity(calls.len());
    for (call, name, arguments) in calls {
        let router::Routed::Plain { use_, tool } = router::route(
            &route_ports,
            &runtime.cwd,
            &name,
            &arguments,
            loaded.as_ref(),
        )?
        else {
            return Err(AgentError::Route(router::RouteError::UnknownTool(name)));
        };
        routed.push(Prepared {
            call,
            name,
            arguments,
            use_,
            tool,
        });
    }

    // Guarda de orçamento: o lote tem de caber **inteiro** antes de logar qualquer pedido. Um
    // débito recusado a meio deixaria `ToolCall` sem `ToolResult` no log (violaria §42).
    let tools: Vec<ToolName> = routed.iter().map(|call| call.use_.name).collect();
    if !runtime.session.can_afford_tool_calls(&tools, now) {
        return Ok(Batch::Fallback(
            routed
                .into_iter()
                .map(|call| (call.call, call.name, call.arguments))
                .collect(),
        ));
    }

    // Fase 1b — loga os pedidos, agora que todos cabem.
    for call in &routed {
        runtime
            .session
            .begin_call(call.call.clone(), &call.use_, now)?;
    }

    // Fase 2 — efeito em paralelo, contra um snapshot imutável do estado.
    let dispatches = in_parallel(runtime, &routed, now)?;
    #[cfg(test)]
    PARALLEL_BATCHES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    Ok(Batch::Settled(
        routed
            .into_iter()
            .zip(dispatches)
            .map(|(call, dispatch)| Settled {
                call: call.call,
                name: call.name,
                arguments: call.arguments,
                use_: call.use_,
                dispatch,
            })
            .collect(),
    ))
}

/// Executa os efeitos num pool limitado de threads ligadas ao escopo (sem `unsafe`).
fn in_parallel(
    runtime: &Runtime<'_>,
    routed: &[Prepared<'_>],
    now: u64,
) -> Result<Vec<Dispatch>, AgentError> {
    let _span = katu_core::trace_fn!("agent::turn::batch::in_parallel");

    let state = runtime.session.state();
    let rules = &runtime.rules;
    let joined: Vec<std::thread::Result<Result<Dispatch, PolicyError>>> =
        std::thread::scope(|scope| {
            // `collect` é **necessário**: encadear `.map(spawn).map(join)` entrelaça o `spawn` e o
            // `join` por call, pelo que cada thread só nasce quando a anterior termina — o lote
            // fica serializado (medido: 547 ms em vez de ~107 ms).
            #[allow(
                clippy::needless_collect,
                reason = "o `spawn` tem de ser eager para haver paralelismo"
            )]
            let handles: Vec<_> = routed
                .iter()
                .map(|call| {
                    scope.spawn(move || dispatch(state, &call.use_, rules, now, call.tool.as_ref()))
                })
                .collect();
            handles
                .into_iter()
                .map(std::thread::ScopedJoinHandle::join)
                .collect()
        });
    joined
        .into_iter()
        .map(|result| match result {
            Ok(Ok(dispatch)) => Ok(dispatch),
            Ok(Err(source)) => Err(AgentError::Session(source.into())),
            Err(_) => Err(AgentError::Worker),
        })
        .collect()
}

/// Fase 3 — comete na ordem do modelo (a ordem é a do log, não a de conclusão das threads).
fn commit(
    runtime: &mut Runtime<'_>,
    ports: &Ports<'_>,
    settled: Vec<Settled>,
    activity: &mut dyn ActivitySink,
) -> Result<(), AgentError> {
    let _span = katu_core::trace_fn!("agent::turn::batch::commit");

    for call in settled {
        let outcome = call.dispatch.outcome();
        runtime
            .session
            .settle_call(call.call.clone(), outcome.clone())?;
        emit_outcome(activity, &call.name, &outcome);
        let approval = match &call.dispatch.decision {
            Decision::RequireApproval { request } => Some(request.clone()),
            _ => None,
        };
        let mut call_outcome = CallOutcome {
            outcome,
            use_: Some(call.use_),
            approval,
        };
        retry_with_approval(
            runtime,
            ports,
            &mut call_outcome,
            &call.call,
            &call.name,
            &call.arguments,
            activity,
        )?;
    }
    Ok(())
}

/// Caminho sequencial (semântica original): rota, executa e comete uma call de cada vez.
///
/// Usado quando o lote não cabe no cost governor ou quando não há paralelismo a ganhar.
fn run_shared_sequential(
    runtime: &mut Runtime<'_>,
    ports: &Ports<'_>,
    calls: Vec<(CallId, String, Value)>,
    activity: &mut dyn ActivitySink,
) -> Result<(), AgentError> {
    let _span = katu_core::trace_fn!("agent::turn::batch::run_shared_sequential");

    let root = runtime.root().to_path_buf();
    let loaded = runtime.plan().cloned();
    let route_ports = router::Ports {
        fs: ports.fs,
        process: ports.process,
        env: ports.env,
        clock: runtime.clock,
        root: &root,
    };
    for (call, name, arguments) in calls {
        let router::Routed::Plain { use_, tool } = router::route(
            &route_ports,
            &runtime.cwd,
            &name,
            &arguments,
            loaded.as_ref(),
        )?
        else {
            return Err(AgentError::Route(router::RouteError::UnknownTool(name)));
        };
        let now = runtime.clock.now().as_millis();
        let dispatch = runtime.session.tool_call(
            call.clone(),
            &use_,
            CallContext {
                rules: &runtime.rules,
                now_millis: now,
                tool: tool.as_ref(),
            },
        )?;
        let outcome = dispatch.outcome();
        emit_outcome(activity, &name, &outcome);
        let approval = match &dispatch.decision {
            Decision::RequireApproval { request } => Some(request.clone()),
            _ => None,
        };
        let mut call_outcome = CallOutcome {
            outcome,
            use_: Some(use_),
            approval,
        };
        retry_with_approval(
            runtime,
            ports,
            &mut call_outcome,
            &call,
            &name,
            &arguments,
            activity,
        )?;
    }
    Ok(())
}
