//! Testes do roteador: resolução de caminhos, despacho por família e fail-closed.

use std::path::{Path, PathBuf};

use katu_core::ports::{Clock, FakeEnv, Fs, MemFs, MemProcess, Timestamp};
use katu_policy::{ResolvedPath, SearchMode, ToolArgs, ToolName};
use serde_json::json;

use super::{Ports, RouteError, Routed, route};

/// Relógio fixo dos testes.
struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        Timestamp::from_millis(0)
    }
}

/// Portas de teste sobre um `MemFs` com `/work/src/lib.rs`.
struct Fixture {
    fs: MemFs,
    process: MemProcess,
    env: FakeEnv,
    clock: FixedClock,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let fs = MemFs::new();
        fs.write_atomic(Path::new("/work/src/lib.rs"), b"x")?;
        Ok(Self {
            fs,
            process: MemProcess::ok(""),
            env: FakeEnv::new(),
            clock: FixedClock,
            root: PathBuf::from("/work"),
        })
    }

    fn ports(&self) -> Ports<'_> {
        Ports {
            fs: &self.fs,
            process: &self.process,
            env: &self.env,
            clock: &self.clock,
            root: &self.root,
        }
    }
}

/// Diretório de trabalho canónico dos testes.
fn cwd() -> Result<ResolvedPath, katu_policy::PolicyError> {
    ResolvedPath::from_canonical("/work")
}

#[test]
fn read_resolves_a_relative_path() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let routed = route(
        &fixture.ports(),
        &cwd()?,
        "read",
        &json!({"path": "src/lib.rs"}),
        None,
    )?;
    let Routed::Plain { use_, .. } = routed else {
        return Err("esperava uma tool genérica".into());
    };
    assert_eq!(use_.name, ToolName::Read);
    assert_eq!(
        use_.args,
        ToolArgs::Read {
            path: ResolvedPath::from_canonical("/work/src/lib.rs")?
        }
    );
    Ok(())
}

#[test]
fn ls_uses_the_cwd_without_a_query() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let routed = route(&fixture.ports(), &cwd()?, "ls", &json!({"path": "."}), None)?;
    let Routed::Plain { use_, .. } = routed else {
        return Err("esperava uma tool genérica".into());
    };
    assert_eq!(use_.name, ToolName::Search);
    let ToolArgs::Search { mode, query, .. } = use_.args else {
        return Err("esperava busca".into());
    };
    assert_eq!(mode, SearchMode::Ls);
    assert!(query.is_empty());
    Ok(())
}

#[test]
fn plan_without_a_contract_is_unavailable() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let routed = route(
        &fixture.ports(),
        &cwd()?,
        "plan",
        &json!({"goal": "x", "next_action": "y"}),
        None,
    )?;
    let Routed::Plain { use_, .. } = routed else {
        return Err("esperava a tool indisponível".into());
    };
    assert_eq!(use_.name, ToolName::Plan);
    Ok(())
}

#[test]
fn plan_with_a_loaded_contract_is_routed() -> Result<(), Box<dyn std::error::Error>> {
    use katu_core::plan::{Feature, FeatureStatus, Plan, ScopeContract};

    let fixture = Fixture::new()?;
    let plan = Plan::new(
        ScopeContract::new(
            Vec::new(),
            vec!["**/secrets/**".into()],
            Vec::new(),
            "reverter",
        ),
        vec![Feature::new("F1", "fazer", FeatureStatus::Pending)],
    );
    let routed = route(
        &fixture.ports(),
        &cwd()?,
        "plan",
        &json!({"goal": "x", "next_action": "y"}),
        Some(&plan),
    )?;
    let Routed::Plan { use_, plan } = routed else {
        return Err("esperava o registo de plano".into());
    };
    assert_eq!(use_.name, ToolName::Plan);
    assert_eq!(plan.feature_list.len(), 1);
    Ok(())
}

#[test]
fn unknown_tool_is_fail_closed() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let result = route(&fixture.ports(), &cwd()?, "rm", &json!({}), None);
    assert!(matches!(result, Err(RouteError::UnknownTool(_))));
    Ok(())
}

#[test]
fn missing_argument_does_not_execute() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new()?;
    let result = route(&fixture.ports(), &cwd()?, "read", &json!({}), None);
    assert!(matches!(result, Err(RouteError::MissingArg("path"))));
    Ok(())
}
