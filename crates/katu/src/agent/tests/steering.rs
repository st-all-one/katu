//! Teste do *steering* linear/síncrono (E20-T16): o prompt enfileirado entra no passo seguinte.

use katu_core::kernel::Message;
use katu_core::ports::{FixedClock, Timestamp};
use katu_core::provider::StopReason;
use katu_providers::{FakeProvider, Turn};

use super::{options, request, root, write_call};
use crate::agent::{Activity, ActivitySink, Ports, run_turn_with};
use crate::ports::{StdEnv, StdFs, StdProcess};
use crate::runtime::Runtime;

/// Sink que injeta um prompt de *steering* depois do primeiro passo.
struct SteerSink {
    prompt: Option<String>,
}

impl ActivitySink for SteerSink {
    fn activity(&mut self, _activity: Activity<'_>) {}

    fn steer(&mut self) -> Option<String> {
        self.prompt.take()
    }
}

#[test]
fn steering_injects_a_user_message_between_steps() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("steer")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "escreve um ficheiro")?;

    let provider = FakeProvider::new(
        "fake",
        vec![
            Turn {
                events: vec![write_call()],
                stop: StopReason::ToolCalls,
            },
            Turn::text("feito"),
        ],
    );
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };
    let options = options(4);
    let mut sink = SteerSink {
        prompt: Some("afinal usa outro nome".to_string()),
    };

    run_turn_with(
        &mut runtime,
        request(&provider, ports, "escreve um ficheiro", &options),
        &mut sink,
    )?;

    // O steer vira mensagem de utilizador **entre** o primeiro e o segundo passo.
    let messages = runtime.messages()?;
    let steered = messages.iter().any(|message| {
        matches!(message, Message::User { text } if text.contains("afinal usa outro nome"))
    });
    assert!(steered, "o steer entra no log como mensagem de utilizador");

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}
