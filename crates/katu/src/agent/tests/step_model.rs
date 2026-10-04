//! Testes do modelo por passo (`Q2/PI_GAINS`): o resolver muda o modelo **entre passos** do turno.

use std::sync::Arc;

use katu_core::ports::{FixedClock, Timestamp};
use katu_core::provider::{ModelSpec, Provider, StopReason};
use katu_policy::Phase;
use katu_providers::{FakeProvider, Turn};

use super::{Ports, read_call, request, root};
use crate::agent::{StepModel, TurnOptions, run_turn};
use crate::ports::{StdEnv, StdFs, StdProcess};
use crate::runtime::Runtime;

/// Resolver determinístico de teste: `m<passo>` (prova que o modelo muda a meio do turno).
struct ByStep;

impl StepModel for ByStep {
    fn model_for(&self, _phase: Phase, step: u32) -> Option<ModelSpec> {
        Some(ModelSpec::new(format!("m{step}")))
    }
}

#[test]
fn the_model_can_change_between_steps() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("model-step")?;
    std::fs::write(root.join("nota.txt"), "conteudo\n")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "lê")?;
    let provider: Arc<dyn Provider> = Arc::new(FakeProvider::new(
        "fake",
        vec![
            Turn {
                events: vec![read_call()],
                stop: StopReason::ToolCalls,
            },
            Turn::text("li"),
        ],
    ));
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };
    let options = TurnOptions {
        model: ModelSpec::new("fixo"),
        system: None,
        max_tokens: 128,
        temperature: 0.0,
        max_steps: 4,
        idle_ms: 0,
        step_model: Some(Box::new(ByStep)),
    };

    let report = run_turn(&mut runtime, request(&provider, ports, "lê", &options))?;
    assert_eq!(report.steps, 2);
    assert_eq!(
        report.model, "m2",
        "o modelo do último passo vem do resolver (Q2)"
    );
    runtime.session().verify()?;

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

/// Sem resolver, o modelo fixo é usado em todos os passos.
#[test]
fn without_a_resolver_the_fixed_model_is_used() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("model-fixo")?;
    std::fs::write(root.join("nota.txt"), "conteudo\n")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "lê")?;
    let provider: Arc<dyn Provider> = Arc::new(FakeProvider::new(
        "fake",
        vec![
            Turn {
                events: vec![read_call()],
                stop: StopReason::ToolCalls,
            },
            Turn::text("li"),
        ],
    ));
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };
    let options = TurnOptions {
        model: ModelSpec::new("fixo"),
        system: None,
        max_tokens: 128,
        temperature: 0.0,
        max_steps: 4,
        idle_ms: 0,
        step_model: None,
    };

    let report = run_turn(&mut runtime, request(&provider, ports, "lê", &options))?;
    assert_eq!(report.model, "fixo");
    runtime.session().verify()?;

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}
