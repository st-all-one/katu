//! E09-T03: o gate corre sobre o log da sessão e exige um relatório não bloqueado.

use std::path::Path;

use katu_core::kernel::CallId;
use katu_core::ports::{FixedClock, Timestamp};
use katu_core::provider::{Provider, ProviderEvent, StopReason};
use katu_providers::{FakeProvider, Turn};
use serde_json::json;

use super::{options, request, root};
use crate::agent::{Ports, run_turn};
use crate::ports::{StdEnv, StdFs, StdProcess};
use crate::runtime::{Runtime, VerifyRequest};

/// Artefacto de plano com `forbidden.txt` no escopo proibido (E09-T04).
fn write_plan(root: &Path) -> Result<(), std::io::Error> {
    std::fs::write(
        root.join("scope_contract.json"),
        r#"{"allowed_files":["**"],"forbidden_files":["forbidden.txt"],"acceptance_criteria":["x"],"rollback_plan":"reverter"}"#,
    )?;
    std::fs::write(
        root.join("feature_list.json"),
        r#"[{"id":"F1","description":"fazer","status":"in_progress"}]"#,
    )
}

#[test]
fn gate_blocks_a_write_to_a_forbidden_file() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("verify-block")?;
    write_plan(&root)?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "g")?;
    let provider: std::sync::Arc<dyn Provider> = std::sync::Arc::new(FakeProvider::new(
        "fake",
        vec![
            Turn {
                events: vec![ProviderEvent::ToolCall {
                    call: CallId::new("c1"),
                    name: "write".to_string(),
                    arguments: json!({"path": "forbidden.txt", "content": "x"}),
                }],
                stop: StopReason::ToolCalls,
            },
            Turn::text("feito"),
        ],
    ));
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };
    run_turn(&mut runtime, request(&provider, ports, "g", &options(2)))?;

    let report = runtime.verify(VerifyRequest {
        coverage_floor_bps: 0,
        strict: false,
    })?;
    assert!(report.is_blocked(), "o ficheiro proibido bloqueia");
    assert!(
        report
            .checks
            .iter()
            .any(|check| check.id == "scope.forbidden" && check.status.as_str() == "block")
    );
    runtime.record_verification(&report)?;
    assert!(runtime.session().state().verification.is_some());

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}
