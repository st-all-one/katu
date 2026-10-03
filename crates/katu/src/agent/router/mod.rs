//! Roteador de tool calls (E12-T05): JSON do modelo → `ToolUse` resolvido + executor.
//!
//! É a costura que faltava entre o provider (que produz `name` + argumentos JSON) e o kernel (que
//! avalia um [`ToolUse`] **tipado**, §42). Resolve caminhos **antes** do veredicto (E07-T02) e
//! falha fechado: argumento em falta, view inválida ou tool desconhecida **não** executam nada.
//!
//! Os construtores por tool vivem em [`tools`]; aqui fica o contrato (`Ports`/`Routed`) e o
//! despacho, para manter cada ficheiro sob o limite.

mod tools;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use katu_core::diag::{Level, events};
use katu_core::kernel::Tool;
use katu_core::memory::{Anchor, NoteType, PreWriteReq, RecallReq};
use katu_core::plan::Plan;
use katu_core::ports::{Cancel, Clock, Env, Fs, Process};
use katu_policy::{ResolvedArgv, ResolvedPath, SearchMode, ToolArgs, ToolName, ToolUse};
use katu_tools::read::LineRange;
use katu_tools::resolve::resolve as resolve_via_port;
use serde_json::Value;

use tools::{bash, edit, move_, plan, read, search, trash, write};

/// Portas que o roteador precisa (todas por referência; `Copy`).
#[derive(Clone, Copy)]
pub(super) struct Ports<'a> {
    /// Porta de ficheiros.
    pub fs: &'a dyn Fs,
    /// Porta de processos (`bash`).
    pub process: &'a dyn Process,
    /// Porta de ambiente (`bash`).
    pub env: &'a dyn Env,
    /// Relógio (lixeira).
    pub clock: &'a dyn Clock,
    /// Raiz do projeto (lixeira).
    pub root: &'a Path,
    /// Cancelamento cooperativo do turno (L-P3), lido pelo `bash`.
    pub cancel: Option<&'a dyn Cancel>,
}

/// Resultado do roteamento.
pub(super) enum Routed<'a> {
    /// Tool genérica: `ToolUse` + executor (o kernel despacha pela ordem §42).
    Plain {
        /// Uso resolvido.
        use_: ToolUse,
        /// Executor.
        tool: Box<dyn Tool + 'a>,
    },
    /// Recall de memória (caminho `Session::tool_call` com `RecallTool`).
    MemoryRecall {
        /// Consulta.
        req: RecallReq,
    },
    /// Escrita de memória (caminho `Session::memory_write` com o gate de E05).
    MemoryRecord {
        /// Nota pré-validada.
        req: PreWriteReq,
    },
    /// Validação/registo de plano (E09-T04): plano do artefacto do projeto.
    Plan {
        /// Uso resolvido.
        use_: ToolUse,
        /// Plano carregado no arranque.
        plan: Plan,
    },
}

/// Erro de roteamento (fail-closed: nada executa).
#[derive(Debug, thiserror::Error)]
pub(crate) enum RouteError {
    /// Argumento obrigatório em falta.
    #[error("argumento em falta: {0}")]
    MissingArg(&'static str),
    /// Argumento com forma inválida.
    #[error("argumento inválido: {0}")]
    InvalidArg(&'static str),
    /// Listas de comprimento diferente (Q-07: `old` e `new` são pares).
    #[error(
        "`old` e `new` têm de ter o mesmo comprimento: {old} trecho(s) a substituir vs. {new} substituto(s)"
    )]
    LengthMismatch {
        /// Trechos a substituir.
        old: usize,
        /// Substitutos.
        new: usize,
    },
    /// Caminho que não resolve.
    #[error("caminho inválido: {0}")]
    Path(String),
    /// Tool fora do catálogo fechado.
    #[error("tool desconhecida: {0}")]
    UnknownTool(String),
}

/// Roteia um pedido do modelo para o `ToolUse` + executor correspondentes.
pub(super) fn route<'a>(
    ports: &Ports<'a>,
    cwd: &ResolvedPath,
    name: &str,
    args: &Value,
    loaded: Option<&Plan>,
) -> Result<Routed<'a>, RouteError> {
    let _span = katu_core::fn_span!(Level::Debug, events::AGENT_ROUTE, "router::route");
    match name {
        "read" => read(ports, cwd, args),
        "write" => write(ports, cwd, args),
        "edit" => edit(ports, cwd, args),
        "move" => move_(ports, cwd, args),
        "trash" => trash(ports, cwd, args),
        "bash" => bash(ports, cwd, args),
        "grep" => search(ports, cwd, args, SearchMode::Grep),
        "find" => search(ports, cwd, args, SearchMode::Find),
        "ls" => search(ports, cwd, args, SearchMode::Ls),
        "plan" => Ok(plan(cwd, loaded)),
        "memory" => memory(args),
        other => Err(RouteError::UnknownTool(other.to_string())),
    }
}

/// Nome canônico (`ToolName`) de uma tool do catálogo; `None` fora do catálogo.
///
/// Usado no caminho de **erro de argumentos**: o erro é devolvido ao modelo como resultado de tool,
/// pelo que o `ToolCall` é logado com um `ToolUse` sintético (`ToolArgs::Other`).
#[must_use]
pub(super) fn tool_name_for(name: &str) -> Option<ToolName> {
    let _span = katu_core::trace_fn!("agent::router::tool_name_for");

    Some(match name {
        "read" => ToolName::Read,
        "write" => ToolName::Write,
        "edit" => ToolName::Edit,
        "move" => ToolName::Move,
        "trash" => ToolName::Trash,
        "bash" => ToolName::Exec,
        "grep" | "find" | "ls" => ToolName::Search,
        "plan" => ToolName::Plan,
        "memory" => ToolName::MemoryRecall,
        _ => return None,
    })
}

/// Tool `memory` (`record`/`search`); a execução é do gate de memória (E05).
pub(super) fn memory(args: &Value) -> Result<Routed<'static>, RouteError> {
    let _span = katu_core::trace_fn!("agent::router::memory");

    match required_str(args, "command")?.as_str() {
        "search" => Ok(Routed::MemoryRecall {
            req: RecallReq {
                query: required_str(args, "statement")?,
                limit: optional_usize(args, "limit").unwrap_or(5),
            },
        }),
        "record" => Ok(Routed::MemoryRecord {
            req: PreWriteReq {
                statement: required_str(args, "statement")?,
                note_type: NoteType::Fact,
                anchor: optional_str(args, "anchor").map(Anchor::new),
                body: String::new(),
            },
        }),
        _ => Err(RouteError::InvalidArg("command")),
    }
}

/// Constrói o `ToolUse` com o contexto comum.
pub(super) fn use_of(
    name: ToolName,
    tool_args: ToolArgs,
    paths: Vec<ResolvedPath>,
    argv: Option<ResolvedArgv>,
    cwd: &ResolvedPath,
) -> ToolUse {
    let _span = katu_core::trace_fn!("agent::router::use_of");

    ToolUse {
        name,
        args: tool_args,
        resolved_paths: paths,
        argv,
        cwd: cwd.clone(),
    }
}

/// Resolve um caminho (relativo a `cwd`) seguindo symlinks, antes do veredicto (E07-T02).
pub(super) fn resolve(
    fs: &dyn Fs,
    cwd: &ResolvedPath,
    raw: &str,
) -> Result<ResolvedPath, RouteError> {
    let _span = katu_core::trace_fn!("agent::router::resolve");

    let path = Path::new(raw);
    let full = if path.is_absolute() {
        path.to_path_buf()
    } else {
        PathBuf::from(cwd.as_str()).join(path)
    };
    resolve_via_port(fs, &full).map_err(|_| RouteError::Path(raw.to_string()))
}

/// Interpreta `inicio:fim` (1-based, inclusivo).
pub(super) fn parse_range(raw: &str) -> Result<LineRange, RouteError> {
    let _span = katu_core::trace_fn!("agent::router::parse_range");

    let (first, second) = raw.split_once(':').ok_or(RouteError::InvalidArg("range"))?;
    let parse = |text: &str| text.trim().parse::<u32>().ok();
    match (parse(first), parse(second)) {
        (Some(start), Some(end)) => Ok(LineRange { start, end }),
        _ => Err(RouteError::InvalidArg("range")),
    }
}

/// String obrigatória.
pub(super) fn required_str(args: &Value, key: &'static str) -> Result<String, RouteError> {
    let _span = katu_core::trace_fn!("agent::router::required_str");

    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or(RouteError::MissingArg(key))
}

/// String opcional.
pub(super) fn optional_str(args: &Value, key: &str) -> Option<String> {
    let _span = katu_core::trace_fn!("agent::router::optional_str");

    args.get(key).and_then(Value::as_str).map(str::to_string)
}

/// Booleano opcional (`false` por omissão).
pub(super) fn optional_bool(args: &Value, key: &str) -> bool {
    let _span = katu_core::trace_fn!("agent::router::optional_bool");

    args.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// Inteiro opcional.
pub(super) fn optional_usize(args: &Value, key: &str) -> Option<usize> {
    let _span = katu_core::trace_fn!("agent::router::optional_usize");

    args.get(key)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}

/// `argv` obrigatório (lista de strings não vazia).
/// Lista de textos obrigatória: aceita um texto único (forma histórica) ou uma lista (Q-07).
///
/// A forma única é aceite **de propósito**: um modelo que já aprendeu `"old": "…"` continua a
/// funcionar, e a lista é o mesmo contrato com um elemento. Uma lista vazia é recusada (não é uma
/// edição) e um item que não seja texto também.
pub(super) fn required_text_list(
    args: &Value,
    key: &'static str,
) -> Result<Vec<String>, RouteError> {
    let _span = katu_core::trace_fn!("agent::router::required_text_list");

    match args.get(key) {
        Some(Value::String(text)) => Ok(vec![text.clone()]),
        Some(Value::Array(items)) if !items.is_empty() => items
            .iter()
            .map(|item| {
                item.as_str()
                    .map(str::to_string)
                    .ok_or(RouteError::InvalidArg(key))
            })
            .collect(),
        Some(Value::Array(_)) => Err(RouteError::InvalidArg(key)),
        _ => Err(RouteError::MissingArg(key)),
    }
}

/// Argumento `argv` obrigatório (lista de textos, sem shell).
pub(super) fn required_argv(args: &Value, key: &'static str) -> Result<ResolvedArgv, RouteError> {
    let _span = katu_core::trace_fn!("agent::router::required_argv");

    let list = args
        .get(key)
        .and_then(Value::as_array)
        .ok_or(RouteError::MissingArg(key))?;
    let items: Vec<String> = list
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    ResolvedArgv::new(items).map_err(|_| RouteError::InvalidArg(key))
}
