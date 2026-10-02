//! Erros de argumentos do modelo devolvidos como resultado de tool (não abortam o turno).

use katu_core::kernel::{CallId, Message};
use katu_core::ports::{FixedClock, Timestamp};
use katu_core::provider::{ProviderEvent, StopReason};
use katu_providers::{FakeProvider, Turn};
use serde_json::json;

use super::{Ports, options, request, root};
use crate::agent::run_turn;
use crate::ports::{StdEnv, StdFs, StdProcess};
use crate::runtime::Runtime;

/// Um argumento inválido do modelo **não** aborta o turno: vira resultado de tool (o modelo vê o
/// erro e pode corrigir no passo seguinte).
#[test]
fn a_bad_argument_is_returned_to_the_model() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("bad-arg")?;
    std::fs::write(root.join("a.txt"), "AAA")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "lê um range inválido")?;
    let provider = FakeProvider::new(
        "fake",
        vec![
            Turn {
                events: vec![ProviderEvent::ToolCall {
                    call: CallId::new("r1"),
                    name: "read".to_string(),
                    arguments: json!({"path": "a.txt", "view": "range", "range": "abc"}),
                }],
                stop: StopReason::ToolCalls,
            },
            Turn::text("corrigi"),
        ],
    );
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };

    let report = run_turn(
        &mut runtime,
        request(&provider, ports, "lê um range inválido", &options(4)),
    )?;
    assert_eq!(report.calls, 1, "a call corre (vira resultado de erro)");
    assert_eq!(
        report.text, "corrigi",
        "o turno continua até ao texto final"
    );
    let delivered = runtime.messages()?.iter().any(|message| {
        matches!(
            message,
            Message::ToolResult { delta: Some(delta), .. } if delta.contains("argumento inválido")
        )
    });
    assert!(delivered, "o modelo devia receber o erro de argumento");
    runtime.session().verify()?;

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}
