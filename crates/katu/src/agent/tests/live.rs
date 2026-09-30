//! Testes do observador efémero do turno (E10-T04/T05): recusas e progresso **fora** do log.

use katu_core::error::ToolOutcome;
use katu_core::kernel::{CallId, Message};
use katu_core::ports::{FixedClock, Timestamp};
use katu_core::provider::{ProviderEvent, StopReason};
use katu_policy::{Evidence, RuleId};
use katu_providers::{FakeProvider, Turn};
use serde_json::json;

use super::{options, plan_call, request, root, write_call};
use crate::agent::turn::emit_outcome;
use crate::agent::{Activity, ActivitySink, Ports, run_turn_with};
use crate::ports::{StdEnv, StdFs, StdProcess};
use crate::runtime::Runtime;

mod approval;

/// Observador de teste: acumula progresso e recusas sem tocar no log.
#[derive(Default)]
struct Recorder {
    text: String,
    thinking: String,
    tools: Vec<String>,
    refusals: Vec<String>,
    unavailable: Vec<String>,
}

impl ActivitySink for Recorder {
    fn activity(&mut self, activity: Activity<'_>) {
        match activity {
            Activity::Text(delta) => self.text.push_str(delta),
            Activity::Thinking(delta) => self.thinking.push_str(delta),
            Activity::Tool { name } => self.tools.push(format!("→ {name}")),
            Activity::ToolDone { name } => self.tools.push(format!("✓ {name}")),
            Activity::Refused { rule, .. } => self.refusals.push(rule.to_string()),
            Activity::Unavailable { control, .. } => self.unavailable.push(control.to_string()),
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

#[test]
fn refused_outcome_is_reported_with_rule_and_evidence() {
    let rule = RuleId::from("contain-sensitive-read");
    let outcome = ToolOutcome::Denied {
        rule_id: rule.clone(),
        evidence: Evidence::new("leitura sensível", ".env", rule),
    };
    let mut recorder = Recorder::default();
    emit_outcome(&mut recorder, "read", &outcome);
    assert_eq!(
        recorder.refusals,
        ["contain-sensitive-read".to_string()],
        "refusals={:?} tools={:?} unavailable={:?}",
        recorder.refusals,
        recorder.tools,
        recorder.unavailable
    );
    assert!(
        recorder.tools.is_empty(),
        "uma recusa não é mostrada como concluída"
    );
}

#[test]
fn unavailable_tool_is_surfaced_to_the_observer() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("plan-unavailable")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    // Sem `scope_contract.json`/`feature_list.json`: a tool `plan` fica indisponível.
    let mut runtime = Runtime::open(&fs, &clock, &root, "planear")?;
    let provider = FakeProvider::new(
        "fake",
        vec![
            Turn {
                events: vec![plan_call()],
                stop: StopReason::ToolCalls,
            },
            Turn::text("ok"),
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
    run_turn_with(
        &mut runtime,
        request(&provider, ports, "planear", &options(4)),
        &mut recorder,
    )?;
    assert_eq!(recorder.unavailable, ["scope-contract".to_string()]);
    assert!(recorder.refusals.is_empty());
    runtime.session().verify()?;
    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

#[test]
fn sensitive_read_is_denied_and_surfaced_end_to_end() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("sensitive-read")?;
    std::fs::write(root.join(".env"), "SEGREDO=1")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "ler")?;
    let provider = FakeProvider::new(
        "fake",
        vec![
            Turn {
                events: vec![ProviderEvent::ToolCall {
                    call: CallId::new("s1"),
                    name: "read".to_string(),
                    arguments: json!({"path": ".env"}),
                }],
                stop: StopReason::ToolCalls,
            },
            Turn::text("ok"),
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
    run_turn_with(
        &mut runtime,
        request(&provider, ports, "ler", &options(4)),
        &mut recorder,
    )?;
    assert_eq!(recorder.refusals, ["contain-sensitive-read".to_string()]);
    assert!(
        !recorder.tools.iter().any(|tool| tool.starts_with('✓')),
        "uma recusa não passa por 'concluída': {:?}",
        recorder.tools
    );
    // O veredicto fica no log (auditável), não só no ecrã.
    let messages = runtime.messages()?;
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, Message::ToolResult { .. })),
        "o resultado da tool (recusa) é logado"
    );
    runtime.session().verify()?;
    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}
