//! E07-T02 — resolução de symlink **antes** do veredicto: um link dentro do workspace que aponta
//! para fora resolve-se para fora e a política pede aprovação (nunca é lido por engano).

use std::path::Path;

use katu_core::error::ToolOutcome;
use katu_core::kernel::{State, Tool, ToolOutput, dispatch};
use katu_core::ports::{Fs, MemFs};
use katu_policy::{ResolvedPath, RuleSet, ToolArgs, ToolName, ToolUse};
use katu_tools::resolve::resolve;

type TestResult<T> = Result<T, Box<dyn std::error::Error>>;

/// Política de contenção **real** (E07-T05).
const CONTAINMENT: &str = include_str!("../../../policy/containment.toml");

/// Tool de leitura que nunca devolve payload (só interessa se correu ou não).
struct NoopRead;

impl Tool for NoopRead {
    fn name(&self) -> ToolName {
        ToolName::Read
    }

    fn execute(&self, _use_: &ToolUse) -> ToolOutput {
        ToolOutput::ok()
    }
}

fn read_use(path: &ResolvedPath) -> ToolUse {
    ToolUse {
        name: ToolName::Read,
        args: ToolArgs::Read { path: path.clone() },
        resolved_paths: vec![path.clone()],
        argv: None,
        cwd: path.clone(),
    }
}

#[test]
fn symlink_escaping_the_workspace_requires_approval() -> TestResult<()> {
    let fs = MemFs::new();
    fs.write_atomic(Path::new("/work/src/main.rs"), b"ok")?;
    fs.write_atomic(Path::new("/etc/passwd"), b"secret")?;
    fs.symlink(Path::new("/work/escape"), Path::new("/etc/passwd"));

    // O caminho pedido parece estar sob `/work`; a resolução canónica revela que não está.
    let resolved = resolve(&fs, Path::new("/work/escape"))?;
    assert_eq!(resolved.as_str(), "/etc/passwd");

    let state = State {
        workspace: Some(ResolvedPath::from_canonical("/work")?),
        ..State::initial()
    };
    let rules = RuleSet::from_toml(CONTAINMENT)?;
    let dispatch = dispatch(&state, &read_use(&resolved), &rules, 0, &NoopRead)?;

    assert!(!dispatch.ran(), "o symlink não pode destrancar a leitura");
    assert!(matches!(
        dispatch.outcome(),
        ToolOutcome::Unavailable { .. }
    ));
    Ok(())
}

#[test]
fn symlink_staying_inside_the_workspace_is_allowed() -> TestResult<()> {
    let fs = MemFs::new();
    fs.write_atomic(Path::new("/work/src/real.rs"), b"ok")?;
    fs.symlink(
        Path::new("/work/src/alias.rs"),
        Path::new("/work/src/real.rs"),
    );

    let resolved = resolve(&fs, Path::new("/work/src/alias.rs"))?;
    assert_eq!(resolved.as_str(), "/work/src/real.rs");

    let state = State {
        workspace: Some(ResolvedPath::from_canonical("/work")?),
        ..State::initial()
    };
    let rules = RuleSet::from_toml(CONTAINMENT)?;
    let dispatch = dispatch(&state, &read_use(&resolved), &rules, 0, &NoopRead)?;
    assert!(dispatch.ran(), "um link que fica dentro é normal");
    Ok(())
}
