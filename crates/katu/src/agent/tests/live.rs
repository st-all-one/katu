//! Teste do observador efémero do turno (E10-T05): deltas ao vivo **fora** do log.

use katu_core::kernel::Message;
use katu_core::ports::{FixedClock, Timestamp};
use katu_core::provider::{ProviderEvent, StopReason};
use katu_providers::{FakeProvider, Turn};

use super::{options, request, root, write_call};
use crate::agent::{Activity, ActivitySink, Ports, run_turn_with};
use crate::ports::{StdEnv, StdFs, StdProcess};
use crate::runtime::Runtime;

/// Observador de teste: acumula deltas e tools sem tocar no log.
#[derive(Default)]
struct Recorder {
    text: String,
    thinking: String,
    tools: Vec<String>,
}

impl ActivitySink for Recorder {
    fn activity(&mut self, activity: Activity<'_>) {
        match activity {
            Activity::Text(delta) => self.text.push_str(delta),
            Activity::Thinking(delta) => self.thinking.push_str(delta),
            Activity::Tool { name } => self.tools.push(format!("→ {name}")),
            Activity::ToolDone { name } => self.tools.push(format!("✓ {name}")),
        }
    }
}

#[test]
fn live_observer_sees_deltas_but_they_stay_out_of_the_log() -> Result<(), Box<dyn std::error::Error>>
{
    let root = root("live-observer")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "olá")?;
    let provider = FakeProvider::new(
        "fake",
        vec![
            Turn {
                events: vec![
                    ProviderEvent::Thinking("penso".to_string()),
                    ProviderEvent::Text("olá".to_string()),
                    write_call(),
                ],
                stop: StopReason::ToolCalls,
            },
            Turn::text("fim"),
        ],
    );
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };

    let mut recorder = Recorder::default();
    let report = run_turn_with(
        &mut runtime,
        request(&provider, ports, "olá", &options(4)),
        &mut recorder,
    )?;
    assert_eq!(report.text, "oláfim");
    assert_eq!(recorder.text, "oláfim");
    assert_eq!(recorder.thinking, "penso");
    assert_eq!(
        recorder.tools,
        ["→ write".to_string(), "✓ write".to_string()]
    );
    let messages = runtime.messages()?;
    assert!(
        !messages
            .iter()
            .any(|m| matches!(m, Message::Assistant { text } if text.contains("penso"))),
        "o raciocínio efémero não entra no contexto do modelo"
    );
    runtime.session().verify()?;
    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}
