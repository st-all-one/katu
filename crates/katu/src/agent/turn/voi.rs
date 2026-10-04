//! Gate de **Value of Information** para tool calls (A3/W8-4).
//!
//! Regra determinística: **não chamar** uma tool quando o valor esperado da informação que ela
//! traria é **menor que o custo** de a chamar — e **nunca** saltar o **irreconstruível** (uma
//! chamada que altera o workspace: `write`/`edit`/`move`/`trash`/`bash`/`plan`/`memory`).
//!
//! O valor é medido por **informação já disponível no turno**: uma chamada de só-leitura cuja
//! impressão (nome + argumentos canónicos) já foi executada **e** não houve mutação desde então
//! traz informação marginal **zero** — o resultado já está no contexto. O custo é positivo (um
//! pedido ao provider, I/O, tokens). Logo `VOI = 0 < custo` ⇒ **não chamar**.
//!
//! A mutação **invalida** a informação cacheada (conservador: pode re-executar leituras que não
//! foram afetadas, mas **nunca** serve informação obsoleta). É esta a propriedade de segurança: o
//! gate só poupa chamadas **reconstruíveis** e só quando o resultado é **idêntico** ao que já está
//! no contexto.
//!
//! **Decisão (escrita):** o *default* fica **off** até haver A/B com um modelo que emita *tool
//! calls* nativas (precedente Q-02b/Q-03). Sem esse A/B, ligar o gate às cegas podia esconder
//! informação que o modelo precisava; o proxy mede o que se poupa, não o que se perde.

use katu_core::diag::{Level, events};
use katu_core::error::ToolOutcome;
use katu_core::kernel::{CallId, Fingerprint};
use katu_core::ports::NO_PROGRESS;
use katu_tools::schema::concurrency_of;
use serde_json::Value;

use super::{AgentError, Ports, Runtime};
use crate::agent::router;

/// Decisão do gate para uma chamada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Decision {
    /// Executar (informação nova ou irreconstruível).
    Execute,
    /// Não chamar: a informação já está no contexto (`VOI = 0 < custo`).
    Skip,
}

/// Estado do gate num turno: as impressões de só-leitura já satisfeitas.
#[derive(Debug, Default)]
pub(crate) struct Voi {
    satisfied: std::collections::BTreeSet<u64>,
}

impl Voi {
    /// Estado vazio (nada satisfeito ainda).
    #[must_use]
    pub(crate) fn new() -> Self {
        let _span = katu_core::trace_fn!("agent::turn::voi::new");

        Self::default()
    }

    /// Decide e **atualiza** o estado (a decisão é sequencial, na ordem do modelo).
    ///
    /// `name` é o nome ao modelo; `arguments` são os argumentos canónicos do pedido.
    pub(crate) fn decide(&mut self, name: &str, arguments: &Value) -> Decision {
        let _span = katu_core::trace_fn!("agent::turn::voi::decide");

        // Irreconstruível (ou `memory`, cujo caminho de recall não tem `use_` simples): nunca
        // saltar. A mutação invalida a informação cacheada — conservador, mas seguro.
        if name == "memory" || !concurrency_of(name).is_shared() {
            self.satisfied.clear();
            return Decision::Execute;
        }
        let print = Fingerprint::of(name, arguments);
        if self.satisfied.contains(&print.bits()) {
            return Decision::Skip;
        }
        self.satisfied.insert(print.bits());
        Decision::Execute
    }
}

/// Chamadas de um passo (id + nome + argumentos canónicos).
pub(crate) type Calls = Vec<(CallId, String, Value)>;

/// Aplica o gate a um passo: devolve as calls a executar e regista um resultado sintético para as
/// saltadas (o modelo vê que a chamada foi respondida, sem I/O).
///
/// # Errors
/// [`AgentError`] se o registo sintético falhar (roteamento ou log).
pub(crate) fn apply(
    runtime: &mut Runtime<'_>,
    ports: &Ports<'_>,
    voi: &mut Voi,
    calls: Calls,
) -> Result<Calls, AgentError> {
    let _span = katu_core::trace_fn!("agent::turn::voi::apply");

    let mut to_run: Calls = Vec::with_capacity(calls.len());
    for (call, name, arguments) in calls {
        if voi.decide(&name, &arguments) == Decision::Skip {
            record_skip(runtime, ports, call, &name, &arguments)?;
        } else {
            to_run.push((call, name, arguments));
        }
    }
    Ok(to_run)
}

/// Regista um `ToolCall`/`ToolResult` sintético para uma chamada saltada (o modelo recebe uma
/// resposta determinística; nenhuma I/O é feita).
///
/// # Errors
/// [`AgentError`] se o roteamento ou o log falhar.
fn record_skip(
    runtime: &mut Runtime<'_>,
    ports: &Ports<'_>,
    call: CallId,
    name: &str,
    arguments: &Value,
) -> Result<(), AgentError> {
    let _span = katu_core::trace_fn!("agent::turn::voi::record_skip");

    let root = runtime.root().to_path_buf();
    let loaded = runtime.plan().cloned();
    let now = runtime.clock.now().as_millis();
    let route_ports = router::Ports {
        fs: ports.fs,
        process: ports.process,
        env: ports.env,
        clock: runtime.clock,
        root: &root,
        cancel: None,
        progress: &NO_PROGRESS,
    };
    let routed = router::route(&route_ports, &runtime.cwd, name, arguments, loaded.as_ref())?;
    let (router::Routed::Plain { use_, .. } | router::Routed::Plan { use_, .. }) = routed else {
        return Err(AgentError::Route(router::RouteError::UnknownTool(
            name.to_string(),
        )));
    };
    runtime.session.begin_call(call.clone(), &use_, now)?;
    runtime
        .session
        .settle_call(call, ToolOutcome::Ok, Some(skip_delta(name)))?;
    katu_core::event!(
        Level::Debug,
        events::AGENT_VOI_SKIP,
        "tool" => name,
    );
    Ok(())
}

/// Delta model-visible de uma chamada saltada (determinístico).
fn skip_delta(name: &str) -> String {
    let _span = katu_core::trace_fn!("agent::turn::voi::skip_delta");

    format!("(voi) {name}: informação já presente no contexto; chamada não repetida")
}
