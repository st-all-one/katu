//! Construtores por tool (E12-T05): os argumentos do modelo viram `ToolUse` + executor.
//!
//! Só é chamado depois de [`super::route`] reconhecer o nome; qualquer argumento em falta falha
//! fechado antes de o kernel avaliar.

use katu_core::error::ToolOutcome;
use katu_core::kernel::{Tool, ToolOutput};
use katu_core::plan::Plan;
use katu_policy::{ControlId, ResolvedPath, SearchMode, ToolArgs, ToolName, ToolUse};
use katu_tools::edit::EditFileTool;
use katu_tools::exec::{DEFAULT_TIMEOUT_MS, ExecTool};
use katu_tools::move_file::MoveFileTool;
use katu_tools::read::{ReadBudget, ReadTool, View};
use katu_tools::search::{DEFAULT_LIMIT, SearchTool, search_use};
use katu_tools::trash::TrashTool;
use katu_tools::write_file::WriteFileTool;
use serde_json::Value;

use super::{
    Ports, RouteError, Routed, optional_bool, optional_str, optional_usize, parse_range,
    required_argv, required_str, resolve, use_of,
};

/// Tool `read` com view/range/símbolo.
pub(super) fn read<'a>(
    ports: &Ports<'a>,
    cwd: &ResolvedPath,
    args: &Value,
) -> Result<Routed<'a>, RouteError> {
    let path = resolve(ports.fs, cwd, &required_str(args, "path")?)?;
    let view = match optional_str(args, "view") {
        Some(name) => View::parse(&name).ok_or(RouteError::InvalidArg("view"))?,
        None => View::Summary,
    };
    let range = optional_str(args, "range")
        .map(|raw| parse_range(&raw))
        .transpose()?;
    let use_ = use_of(
        ToolName::Read,
        ToolArgs::Read { path: path.clone() },
        vec![path],
        None,
        cwd,
    );
    let tool = ReadTool {
        fs: ports.fs,
        view,
        range,
        symbol: optional_str(args, "symbol"),
        base: optional_str(args, "base"),
        budget: ReadBudget::default(),
    };
    Ok(Routed::Plain {
        use_,
        tool: Box::new(tool),
    })
}

/// Tool `write` de ficheiro novo.
pub(super) fn write<'a>(
    ports: &Ports<'a>,
    cwd: &ResolvedPath,
    args: &Value,
) -> Result<Routed<'a>, RouteError> {
    let path = resolve(ports.fs, cwd, &required_str(args, "path")?)?;
    let content = required_str(args, "content")?;
    let bytes = u64::try_from(content.len()).unwrap_or(u64::MAX);
    let use_ = use_of(
        ToolName::Write,
        ToolArgs::Write {
            path: path.clone(),
            bytes,
        },
        vec![path],
        None,
        cwd,
    );
    let tool = WriteFileTool {
        fs: ports.fs,
        content: content.into_bytes(),
    };
    Ok(Routed::Plain {
        use_,
        tool: Box::new(tool),
    })
}

/// Tool `edit` (patch otimista).
pub(super) fn edit<'a>(
    ports: &Ports<'a>,
    cwd: &ResolvedPath,
    args: &Value,
) -> Result<Routed<'a>, RouteError> {
    let path = resolve(ports.fs, cwd, &required_str(args, "path")?)?;
    let use_ = use_of(
        ToolName::Edit,
        ToolArgs::Edit { path: path.clone() },
        vec![path],
        None,
        cwd,
    );
    let tool = EditFileTool {
        fs: ports.fs,
        old: required_str(args, "old")?,
        new: required_str(args, "new")?,
        dry_run: optional_bool(args, "dry_run"),
    };
    Ok(Routed::Plain {
        use_,
        tool: Box::new(tool),
    })
}

/// Tool `move` (renomeação atómica).
pub(super) fn move_<'a>(
    ports: &Ports<'a>,
    cwd: &ResolvedPath,
    args: &Value,
) -> Result<Routed<'a>, RouteError> {
    let from = resolve(ports.fs, cwd, &required_str(args, "from")?)?;
    let to = resolve(ports.fs, cwd, &required_str(args, "to")?)?;
    let use_ = use_of(
        ToolName::Move,
        ToolArgs::Move {
            from: from.clone(),
            to: to.clone(),
        },
        vec![from, to],
        None,
        cwd,
    );
    Ok(Routed::Plain {
        use_,
        tool: Box::new(MoveFileTool { fs: ports.fs }),
    })
}

/// Tool `trash` (lixeira recuperável).
pub(super) fn trash<'a>(
    ports: &Ports<'a>,
    cwd: &ResolvedPath,
    args: &Value,
) -> Result<Routed<'a>, RouteError> {
    let path = resolve(ports.fs, cwd, &required_str(args, "path")?)?;
    let use_ = use_of(
        ToolName::Trash,
        ToolArgs::Trash { path: path.clone() },
        vec![path],
        None,
        cwd,
    );
    let tool = TrashTool {
        fs: ports.fs,
        clock: ports.clock,
        root: ports.root.to_path_buf(),
    };
    Ok(Routed::Plain {
        use_,
        tool: Box::new(tool),
    })
}

/// Tool `bash` (argv sem shell).
pub(super) fn bash<'a>(
    ports: &Ports<'a>,
    cwd: &ResolvedPath,
    args: &Value,
) -> Result<Routed<'a>, RouteError> {
    let command = required_argv(args, "argv")?;
    let workdir = match optional_str(args, "cwd") {
        Some(raw) => resolve(ports.fs, cwd, &raw)?,
        None => cwd.clone(),
    };
    let use_ = use_of(
        ToolName::Exec,
        ToolArgs::Exec {
            argv: command.clone(),
            cwd: workdir.clone(),
        },
        vec![workdir.clone()],
        Some(command),
        &workdir,
    );
    let tool = ExecTool {
        process: ports.process,
        env: ports.env,
        timeout_ms: DEFAULT_TIMEOUT_MS,
        parent: None,
    };
    Ok(Routed::Plain {
        use_,
        tool: Box::new(tool),
    })
}

/// Tool de busca (`grep`/`find`/`ls`).
pub(super) fn search<'a>(
    ports: &Ports<'a>,
    cwd: &ResolvedPath,
    args: &Value,
    mode: SearchMode,
) -> Result<Routed<'a>, RouteError> {
    let root = match optional_str(args, "root").or_else(|| optional_str(args, "path")) {
        Some(raw) => resolve(ports.fs, cwd, &raw)?,
        None => cwd.clone(),
    };
    let query = if mode == SearchMode::Ls {
        String::new()
    } else {
        required_str(args, "query")?
    };
    let use_ = search_use(&root, query, mode);
    let tool = SearchTool {
        fs: ports.fs,
        limit: optional_usize(args, "limit").unwrap_or(DEFAULT_LIMIT),
    };
    Ok(Routed::Plain {
        use_,
        tool: Box::new(tool),
    })
}

/// Tool `plan`: valida o plano do artefacto carregado no arranque (E09-T04). Sem artefacto fica
/// **não executável** (controlo `scope-contract` em falta), fail-closed recuperável.
pub(super) fn plan<'a>(cwd: &ResolvedPath, loaded: Option<&Plan>) -> Routed<'a> {
    let use_ = use_of(ToolName::Plan, ToolArgs::Plan, Vec::new(), None, cwd);
    match loaded {
        Some(plan) => Routed::Plan {
            use_,
            plan: plan.clone(),
        },
        None => Routed::Plain {
            use_,
            tool: Box::new(Unavailable {
                tool_name: ToolName::Plan,
                control: "scope-contract",
            }),
        },
    }
}

/// Executor que devolve `Unavailable` (controlo em falta) sem efeito.
struct Unavailable {
    tool_name: ToolName,
    control: &'static str,
}

impl Tool for Unavailable {
    fn name(&self) -> ToolName {
        self.tool_name
    }

    fn execute(&self, _use_: &ToolUse) -> ToolOutput {
        ToolOutput::outcome(ToolOutcome::Unavailable {
            control: ControlId::new(self.control),
            rule_id: None,
        })
    }
}
