//! Testes do guard de loop no turno (Q-12/F7): corte cedo, turno fechado, teto de passos.

use katu_core::kernel::CallId;
use katu_core::ports::{FixedClock, Timestamp};
use katu_core::provider::{Provider, ProviderEvent, StopReason};
use katu_providers::{FakeProvider, Turn};
use serde_json::json;

use super::{Ports, options, request, root};
use crate::agent::{Termination, run_turn};
use crate::ports::{StdEnv, StdFs, StdProcess};
use crate::runtime::Runtime;

/// A **mesma** leitura (nome + argumentos iguais ao `read_call` de `super`) com um `CallId`
/// distinto por passo: é a assinatura que o guard usa, não o identificador da chamada.
fn repeated_read(index: u32) -> ProviderEvent {
    ProviderEvent::ToolCall {
        call: CallId::new(format!("c{index}")),
        name: "read".to_string(),
        arguments: json!({"path": "nota.txt", "view": "full"}),
    }
}

/// Guião de `total` passos, todos com a mesma leitura.
fn loop_script(total: u32) -> Vec<Turn> {
    (0..total)
        .map(|index| Turn {
            events: vec![repeated_read(index)],
            stop: StopReason::ToolCalls,
        })
        .collect()
}

#[test]
fn a_repeated_call_is_cut_before_the_step_cap() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("loop-guard")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "lê o ficheiro")?;
    let provider: std::sync::Arc<dyn Provider> =
        std::sync::Arc::new(FakeProvider::new("fake", loop_script(8)));
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };

    let report = run_turn(
        &mut runtime,
        request(&provider, ports, "lê o ficheiro", &options(8)),
    )?;
    let Termination::Loop { step, reason } = report.termination else {
        return Err(format!("esperava um corte por loop, veio {:?}", report.termination).into());
    };
    assert_eq!(step, 5, "corta no 5.º passo, não no teto de 8");
    assert!(reason.contains("loop detectado"), "{reason}");
    // L-Q3: o fim anormal fica **visível** (mensagem do assistente) e o turno fica fechado.
    assert!(report.text.contains("repetição"), "{}", report.text);
    // O turno fica **fechado** (`turn_open` vem do log): `TurnStart` sem `TurnEnd` deixaria a
    // retomada inconsistente.
    assert!(!runtime.session.state().turn_open);
    Ok(())
}

#[test]
fn the_step_cap_also_closes_the_turn() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("cap-closes")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "lê tudo")?;
    // Chamadas **diversas** (não é loop): quem corta é o teto de passos.
    let script: Vec<Turn> = (0..4)
        .map(|index| Turn {
            events: vec![ProviderEvent::ToolCall {
                call: CallId::new(format!("c{index}")),
                name: "read".to_string(),
                arguments: json!({"path": format!("nota{index}.txt"), "view": "full"}),
            }],
            stop: StopReason::ToolCalls,
        })
        .collect();
    let provider: std::sync::Arc<dyn Provider> =
        std::sync::Arc::new(FakeProvider::new("fake", script));
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };
    let report = run_turn(
        &mut runtime,
        request(&provider, ports, "lê tudo", &options(1)),
    )?;
    assert_eq!(report.termination, Termination::MaxSteps { steps: 1 });
    assert!(
        report.text.contains("limite de 1 passos"),
        "{}",
        report.text
    );
    assert!(
        !runtime.session.state().turn_open,
        "o teto de passos também fecha o turno"
    );
    Ok(())
}

#[test]
fn a_diverse_turn_is_not_cut() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("no-cut")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "lê dois ficheiros")?;
    let script = vec![
        Turn {
            events: vec![repeated_read(0)],
            stop: StopReason::ToolCalls,
        },
        Turn {
            events: vec![ProviderEvent::ToolCall {
                call: CallId::new("c9"),
                name: "read".to_string(),
                arguments: json!({"path": "outro.txt", "view": "full"}),
            }],
            stop: StopReason::ToolCalls,
        },
        Turn::text("fim"),
    ];
    let provider: std::sync::Arc<dyn Provider> =
        std::sync::Arc::new(FakeProvider::new("fake", script));
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };
    let report = run_turn(
        &mut runtime,
        request(&provider, ports, "lê dois ficheiros", &options(6)),
    )?;
    assert_eq!(report.steps, 3);
    assert_eq!(report.text, "fim");
    assert!(!runtime.session.state().turn_open);
    Ok(())
}
