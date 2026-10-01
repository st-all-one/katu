//! E06-T08 — **gate do épico:** a política é imposta **na operação**, não num wrapper.
//!
//! Cada tool real é despachada por `katu_core::kernel::dispatch`: com um veredicto `Deny`, o efeito
//! **não** ocorre (o ficheiro não muda, o comando não corre, a lixeira não cresce) e a negação é
//! observada no executor (`Effect::Skipped`), não numa camada de prompt/listeners. Com `Allow`, a
//! mesma tool corre — a decisão vem sempre de `dispatch`, nunca do texto do pedido.

use std::path::{Path, PathBuf};

use katu_core::kernel::{State, dispatch};
use katu_core::plan::{Feature, FeatureStatus, Plan, ScopeContract};
use katu_core::ports::{FakeEnv, FixedClock, Fs, MemFs, MemProcess, Timestamp};
use katu_policy::{
    Enforcement, PolicyError, ResolvedArgv, ResolvedPath, Rule, RuleCategory, RuleExamples, RuleId,
    RuleScope, RuleSet, Severity, ToolArgs, ToolName, ToolUse,
};
use katu_tools::edit::{EditFileTool, Replacement};
use katu_tools::exec::ExecTool;
use katu_tools::move_file::MoveFileTool;
use katu_tools::plan::PlanTool;
use katu_tools::trash::TrashTool;
use katu_tools::write_file::WriteFileTool;

type TestResult<T> = Result<T, Box<dyn std::error::Error>>;

fn deny(tool: ToolName) -> RuleSet {
    RuleSet {
        vocab: 3,
        rules: vec![Rule {
            id: RuleId::from("gate"),
            statement: "gate".to_string(),
            scope: RuleScope::Command { tool },
            enforcement: Enforcement::DenyCommand { tool },
            severity: Severity::Critical,
            category: RuleCategory::Enforced,
            remedy: None,
            expires_at: None,
            waiver: None,
            examples: RuleExamples::default(),
        }],
    }
}

fn allow_all() -> RuleSet {
    RuleSet {
        vocab: 3,
        rules: Vec::new(),
    }
}

fn path(value: &str) -> Result<ResolvedPath, PolicyError> {
    ResolvedPath::from_canonical(value)
}

fn write_use(target: &str) -> Result<ToolUse, PolicyError> {
    let resolved = path(target)?;
    Ok(ToolUse {
        name: ToolName::Write,
        args: ToolArgs::Write {
            path: resolved.clone(),
            bytes: 3,
        },
        resolved_paths: vec![resolved.clone()],
        argv: None,
        cwd: resolved,
    })
}

fn edit_use(target: &str) -> Result<ToolUse, PolicyError> {
    let resolved = path(target)?;
    Ok(ToolUse {
        name: ToolName::Edit,
        args: ToolArgs::Edit {
            path: resolved.clone(),
        },
        resolved_paths: vec![resolved.clone()],
        argv: None,
        cwd: resolved,
    })
}

fn move_use(from: &str, to: &str) -> Result<ToolUse, PolicyError> {
    let source = path(from)?;
    let target = path(to)?;
    Ok(ToolUse {
        name: ToolName::Move,
        args: ToolArgs::Move {
            from: source.clone(),
            to: target.clone(),
        },
        resolved_paths: vec![source.clone(), target],
        argv: None,
        cwd: source,
    })
}

fn trash_use(target: &str) -> Result<ToolUse, PolicyError> {
    let resolved = path(target)?;
    Ok(ToolUse {
        name: ToolName::Trash,
        args: ToolArgs::Trash {
            path: resolved.clone(),
        },
        resolved_paths: vec![resolved.clone()],
        argv: None,
        cwd: resolved,
    })
}

fn exec_use(argv: &[&str]) -> Result<ToolUse, PolicyError> {
    let cwd = path("/work")?;
    let resolved = ResolvedArgv::new(argv.iter().map(|arg| (*arg).to_string()).collect())?;
    Ok(ToolUse {
        name: ToolName::Exec,
        args: ToolArgs::Exec {
            argv: resolved.clone(),
            cwd: cwd.clone(),
        },
        resolved_paths: vec![cwd.clone()],
        argv: Some(resolved),
        cwd,
    })
}

fn plan_use() -> Result<ToolUse, PolicyError> {
    let cwd = path("/work")?;
    Ok(ToolUse {
        name: ToolName::Plan,
        args: ToolArgs::Plan,
        resolved_paths: vec![cwd.clone()],
        argv: None,
        cwd,
    })
}

fn valid_plan() -> Plan {
    Plan::new(
        ScopeContract::new(
            vec!["src/**".to_string()],
            vec!["**/secrets/**".to_string()],
            vec!["testes passam".to_string()],
            "reverter o commit",
        ),
        vec![Feature::new("F1", "fazer", FeatureStatus::Pending)],
    )
}

#[test]
fn deny_write_does_not_touch_the_file() -> TestResult<()> {
    let fs = MemFs::new();
    let tool = WriteFileTool {
        fs: &fs,
        content: b"abc".to_vec(),
    };
    let dispatch = dispatch(
        &State::initial(),
        &write_use("/work/a.rs")?,
        &deny(ToolName::Write),
        0,
        &tool,
    )?;
    assert!(!dispatch.ran());
    assert!(!fs.exists(Path::new("/work/a.rs")));
    Ok(())
}

#[test]
fn allow_write_writes() -> TestResult<()> {
    let fs = MemFs::new();
    let tool = WriteFileTool {
        fs: &fs,
        content: b"abc".to_vec(),
    };
    let dispatch = dispatch(
        &State::initial(),
        &write_use("/work/a.rs")?,
        &allow_all(),
        0,
        &tool,
    )?;
    assert!(dispatch.ran());
    assert!(fs.exists(Path::new("/work/a.rs")));
    Ok(())
}

#[test]
fn deny_edit_does_not_patch() -> TestResult<()> {
    let fs = MemFs::new();
    fs.write_atomic(Path::new("/work/a.rs"), b"let x = 1;\n")?;
    let tool = EditFileTool {
        fs: &fs,
        replacements: vec![Replacement::new("let x = 1;", "let x = 2;")],
        dry_run: false,
    };
    let dispatch = dispatch(
        &State::initial(),
        &edit_use("/work/a.rs")?,
        &deny(ToolName::Edit),
        0,
        &tool,
    )?;
    assert!(!dispatch.ran());
    assert_eq!(fs.read(Path::new("/work/a.rs"))?, b"let x = 1;\n".to_vec());
    Ok(())
}

#[test]
fn deny_move_does_not_rename() -> TestResult<()> {
    let fs = MemFs::new();
    fs.write_atomic(Path::new("/work/a.rs"), b"x")?;
    let tool = MoveFileTool { fs: &fs };
    let dispatch = dispatch(
        &State::initial(),
        &move_use("/work/a.rs", "/work/b.rs")?,
        &deny(ToolName::Move),
        0,
        &tool,
    )?;
    assert!(!dispatch.ran());
    assert!(fs.exists(Path::new("/work/a.rs")));
    assert!(!fs.exists(Path::new("/work/b.rs")));
    Ok(())
}

#[test]
fn deny_trash_does_not_move() -> TestResult<()> {
    let fs = MemFs::new();
    fs.write_atomic(Path::new("/work/src/a.rs"), b"x")?;
    let clock = FixedClock::new(Timestamp::from_millis(1));
    let tool = TrashTool {
        fs: &fs,
        clock: &clock,
        root: PathBuf::from("/work"),
    };
    let dispatch = dispatch(
        &State::initial(),
        &trash_use("/work/src/a.rs")?,
        &deny(ToolName::Trash),
        0,
        &tool,
    )?;
    assert!(!dispatch.ran());
    assert!(fs.exists(Path::new("/work/src/a.rs")));
    assert!(!fs.exists(Path::new("/work/.katu/trash/src/a.rs")));
    Ok(())
}

#[test]
fn deny_exec_does_not_run_the_command() -> TestResult<()> {
    let process = MemProcess::ok("boom");
    let env = FakeEnv::new();
    let fs = MemFs::new();
    let tool = ExecTool {
        process: &process,
        env: &env,
        fs: &fs,
        root: Path::new("/work"),
        timeout_ms: 1_000,
        parent: None,
    };
    let dispatch = dispatch(
        &State::initial(),
        &exec_use(&["bash", "-c", "rm -rf /work"])?,
        &deny(ToolName::Exec),
        0,
        &tool,
    )?;
    assert!(!dispatch.ran());
    assert!(process.runs().is_empty());
    Ok(())
}

#[test]
fn deny_plan_does_not_validate() -> TestResult<()> {
    let tool = PlanTool { plan: valid_plan() };
    let dispatch = dispatch(
        &State::initial(),
        &plan_use()?,
        &deny(ToolName::Plan),
        0,
        &tool,
    )?;
    assert!(!dispatch.ran());
    assert!(dispatch.report().is_none());
    Ok(())
}
