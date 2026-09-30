//! Teste do modo de planeamento (E20-T11): regra de escrita só sob `.katu/` e `!` bloqueado.

use katu_core::kernel::facts_for;
use katu_core::ports::{FixedClock, Timestamp};
use katu_policy::{
    Decision, Facts, ResolvedArgv, ResolvedPath, ToolArgs, ToolName, ToolUse, evaluate,
};

use super::root;
use crate::ports::StdFs;
use crate::runtime::Runtime;

/// Factos de uma escrita num caminho.
fn write_facts(runtime: &Runtime<'_>, cwd: &ResolvedPath, path: &ResolvedPath) -> Facts {
    let use_ = ToolUse {
        name: ToolName::Write,
        args: ToolArgs::Write {
            path: path.clone(),
            bytes: 1,
        },
        resolved_paths: vec![path.clone()],
        argv: None,
        cwd: cwd.clone(),
    };
    facts_for(runtime.session.state(), &use_, 0)
}

/// Factos de uma execução `sh -c <cmd>`.
fn exec_facts(
    runtime: &Runtime<'_>,
    cwd: &ResolvedPath,
) -> Result<Facts, Box<dyn std::error::Error>> {
    let argv = ResolvedArgv::new(vec![
        "sh".to_string(),
        "-c".to_string(),
        "echo oi".to_string(),
    ])?;
    let use_ = ToolUse {
        name: ToolName::Exec,
        args: ToolArgs::Exec {
            argv: argv.clone(),
            cwd: cwd.clone(),
        },
        resolved_paths: vec![cwd.clone()],
        argv: Some(argv),
        cwd: cwd.clone(),
    };
    Ok(facts_for(runtime.session.state(), &use_, 0))
}

/// Avalia os factos contra as regras correntes do runtime.
fn decide(runtime: &Runtime<'_>, facts: &Facts) -> Result<Decision, Box<dyn std::error::Error>> {
    Ok(evaluate(facts, &runtime.rules)?)
}

#[test]
fn plan_mode_denies_writes_outside_katu() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("plan-write")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_790_778_540_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "planeia")?;
    let cwd = ResolvedPath::from_canonical(runtime.root())?;
    let outside = ResolvedPath::from_canonical(root.join("src/main.rs"))?;
    let inside = ResolvedPath::from_canonical(root.join(".katu/plan/x.md"))?;

    assert!(
        decide(&runtime, &write_facts(&runtime, &cwd, &outside))?.is_allow(),
        "fora do plano é permitido"
    );
    runtime.set_plan_mode(true)?;
    assert!(matches!(
        decide(&runtime, &write_facts(&runtime, &cwd, &outside))?,
        Decision::Deny { .. }
    ));
    assert!(
        decide(&runtime, &write_facts(&runtime, &cwd, &inside))?.is_allow(),
        "sob .katu é permitido"
    );
    runtime.set_plan_mode(false)?;
    assert!(decide(&runtime, &write_facts(&runtime, &cwd, &outside))?.is_allow());

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

#[test]
fn plan_mode_blocks_shell_and_writes_the_artifact() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("plan-shell")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_790_778_540_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "planeia")?;
    let cwd = ResolvedPath::from_canonical(runtime.root())?;

    runtime.set_plan_mode(true)?;
    assert!(matches!(
        decide(&runtime, &exec_facts(&runtime, &cwd)?)?,
        Decision::Deny { .. }
    ));

    let artifact = runtime.write_plan_artifact(&fs)?;
    assert!(artifact.exists(), "o artefacto do plano é escrito");
    let name = artifact
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    assert!(name.starts_with("2609301429Z-"), "nome UTC: {name}");

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}
