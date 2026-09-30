//! E07-T03 — **gate do épico:** controlo em falta = recusa; contenção **soft** é honesta.
//!
//! Onde há controlo, o katu **não** executa sem veredicto (`Denied`/`Unavailable` com o
//! `ControlId` exato). Onde **não** há, o teste di-lo em voz alta: a contenção é *soft*, um comando
//! lançado fora das tools do katu continua a correr — a limitação é declarada, nunca escondida.

use katu_core::containment::{ContainmentStatus, Jail, NoJail, SandboxEnforcement};
use katu_core::error::ToolOutcome;
use katu_core::kernel::{State, Tool, ToolOutput, dispatch};
use katu_core::ports::{Env, ExecRequest, FakeEnv, MemProcess, Process};
use katu_policy::{
    Enforcement, PolicyError, ResolvedArgv, ResolvedPath, Rule, RuleCategory, RuleExamples, RuleId,
    RuleScope, RuleSet, Severity, ToolArgs, ToolName, ToolUse,
};
use katu_tools::exec::ExecTool;

type TestResult<T> = Result<T, Box<dyn std::error::Error>>;

fn path(value: &str) -> Result<ResolvedPath, PolicyError> {
    ResolvedPath::from_canonical(value)
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

fn allow_all() -> RuleSet {
    RuleSet {
        vocab: 3,
        rules: Vec::new(),
    }
}

#[test]
fn unknown_vocabulary_never_runs() -> TestResult<()> {
    let process = MemProcess::ok("");
    let env = FakeEnv::new();
    let tool = ExecTool {
        process: &process,
        env: &env,
        timeout_ms: 1_000,
        parent: None,
    };
    let rules = RuleSet {
        vocab: 999,
        rules: Vec::new(),
    };
    let result = dispatch(&State::initial(), &exec_use(&["ls"])?, &rules, 0, &tool);
    assert!(
        result.is_err(),
        "vocabulário desconhecido devia falhar-fechado"
    );
    assert!(process.runs().is_empty());
    Ok(())
}

#[test]
fn missing_argv_is_unavailable_and_does_not_run() -> TestResult<()> {
    let process = MemProcess::ok("");
    let env = FakeEnv::new();
    let tool = ExecTool {
        process: &process,
        env: &env,
        timeout_ms: 1_000,
        parent: None,
    };
    let mut use_ = exec_use(&["ls"])?;
    use_.argv = None;
    let dispatch = dispatch(&State::initial(), &use_, &allow_all(), 0, &tool)?;
    assert!(matches!(
        dispatch.outcome(),
        ToolOutcome::Unavailable { .. }
    ));
    assert!(process.runs().is_empty());
    Ok(())
}

#[test]
fn relative_path_cannot_be_resolved() {
    assert!(path("relativo/dir").is_err());
}

#[test]
fn kernel_mode_is_soft_and_no_jail_fails_closed() {
    let status = ContainmentStatus::mvp();
    assert!(!status.kernel_isolated());
    assert!(
        NoJail.acquire(SandboxEnforcement::Full).is_err(),
        "a jail futura não está implementada: pedir `Full` tem de falhar-fechado"
    );
}

#[test]
fn authorization_missing_outside_the_workspace_is_refused() -> TestResult<()> {
    // E07-T03/T05: sem capacidade explícita, ler fora do workspace pede aprovação — e a tool não
    // corre. A autorização interativa (CLI/TUI, E10) é a única forma de a conceder.
    const CONTAINMENT: &str = include_str!("../../../policy/containment.toml");

    struct NoopRead;
    impl Tool for NoopRead {
        fn name(&self) -> ToolName {
            ToolName::Read
        }
        fn execute(&self, _use_: &ToolUse) -> ToolOutput {
            ToolOutput::ok()
        }
    }

    let target = path("/etc/passwd")?;
    let use_ = ToolUse {
        name: ToolName::Read,
        args: ToolArgs::Read {
            path: target.clone(),
        },
        resolved_paths: vec![target.clone()],
        argv: None,
        cwd: target,
    };
    let state = State {
        workspace: Some(path("/work")?),
        ..State::initial()
    };
    let rules = RuleSet::from_toml(CONTAINMENT)?;
    let dispatch = dispatch(&state, &use_, &rules, 0, &NoopRead)?;
    assert!(
        !dispatch.ran(),
        "fora do workspace sem autorização não corre"
    );
    assert!(matches!(
        dispatch.outcome(),
        ToolOutcome::Unavailable { .. }
    ));
    Ok(())
}

#[test]
fn soft_containment_does_not_confine_the_host() -> TestResult<()> {
    let process = MemProcess::ok("");
    let env = FakeEnv::new();
    let tool = ExecTool {
        process: &process,
        env: &env,
        timeout_ms: 1_000,
        parent: None,
    };

    // Dentro das tools do katu, a política nega e o comando não corre.
    let rules = RuleSet {
        vocab: 3,
        rules: vec![Rule {
            id: RuleId::from("deny-exec"),
            statement: "nega exec".to_string(),
            scope: RuleScope::Command {
                tool: ToolName::Exec,
            },
            enforcement: Enforcement::DenyCommand {
                tool: ToolName::Exec,
            },
            severity: Severity::Critical,
            category: RuleCategory::Enforced,
            expires_at: None,
            waiver: None,
            examples: RuleExamples::default(),
        }],
    };
    let dispatch = dispatch(
        &State::initial(),
        &exec_use(&["bash", "-c", "rm -rf /"])?,
        &rules,
        0,
        &tool,
    )?;
    assert!(!dispatch.ran());
    assert!(process.runs().is_empty());

    // Fora das tools do katu, o mesmo comando corre: a contenção é **soft** (não há isolamento).
    assert!(
        process
            .run(&ExecRequest {
                argv: vec!["bash".to_string(), "-c".to_string(), "rm -rf /".to_string()],
                cwd: "/".into(),
                env: env.vars(),
                timeout_ms: 1_000,
            })
            .is_ok()
    );
    assert_eq!(process.runs().len(), 1, "o host não é confinado (soft)");
    Ok(())
}
