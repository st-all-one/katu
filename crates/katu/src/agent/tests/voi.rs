//! Testes do gate de Value of Information (A3/W8-4): não repetir o já conhecido, nunca o
//! irreconstruível, e o *default* off até A/B com o modelo.

use katu_core::kernel::CallId;
use katu_core::ports::{FixedClock, Timestamp};
use katu_core::provider::{Provider, ProviderEvent, StopReason};
use katu_providers::{FakeProvider, Turn};
use serde_json::json;

use super::{options, request, root};
use crate::agent::turn::voi::{Decision, Voi};
use crate::agent::{Ports, run_turn};
use crate::ports::{StdEnv, StdFs, StdProcess};
use crate::runtime::Runtime;

/// Uma chamada de leitura com o argumento `path` e um `CallId` distinto.
fn read(id: &str, path: &str) -> ProviderEvent {
    ProviderEvent::ToolCall {
        call: CallId::new(id),
        name: "read".to_string(),
        arguments: json!({ "path": path }),
    }
}

/// Decide uma sequência de `(nome, argumentos)` e devolve as decisões.
fn decisions(pairs: &[(&str, serde_json::Value)]) -> Vec<Decision> {
    let mut voi = Voi::new();
    pairs
        .iter()
        .map(|(name, args)| voi.decide(name, args))
        .collect()
}

#[test]
fn a_duplicate_read_is_skipped() {
    let args = json!({ "path": "a.txt" });
    let got = decisions(&[("read", args.clone()), ("read", args)]);
    assert_eq!(got, vec![Decision::Execute, Decision::Skip]);
}

#[test]
fn distinct_reads_are_executed() {
    let got = decisions(&[
        ("read", json!({ "path": "a.txt" })),
        ("read", json!({ "path": "b.txt" })),
    ]);
    assert_eq!(got, vec![Decision::Execute, Decision::Execute]);
}

#[test]
fn an_irreconstructible_call_is_never_skipped() {
    let args = json!({ "path": "a.txt", "content": "x" });
    let got = decisions(&[("write", args.clone()), ("write", args)]);
    assert_eq!(got, vec![Decision::Execute, Decision::Execute]);
}

#[test]
fn a_mutation_invalidates_the_cached_read() {
    // `read a` → `write b` (mutação: invalida a informação cacheada) → `read a` (já não é duplicado).
    let got = decisions(&[
        ("read", json!({ "path": "a.txt" })),
        ("write", json!({ "path": "b.txt", "content": "x" })),
        ("read", json!({ "path": "a.txt" })),
    ]);
    assert_eq!(
        got,
        vec![Decision::Execute, Decision::Execute, Decision::Execute]
    );
}

#[test]
fn the_decisions_are_deterministic() {
    let pairs = [
        ("read", json!({ "path": "a.txt" })),
        ("read", json!({ "path": "a.txt" })),
        ("write", json!({ "path": "b.txt", "content": "x" })),
        ("read", json!({ "path": "a.txt" })),
    ];
    assert_eq!(decisions(&pairs), decisions(&pairs));
}

/// Conta os `ToolResult` cujo delta marca uma chamada saltada pelo gate.
fn skipped_results(runtime: &Runtime<'_>) -> Result<usize, Box<dyn std::error::Error>> {
    use katu_core::kernel::Message;

    let mut count: usize = 0;
    for message in runtime.messages()? {
        if let Message::ToolResult {
            delta: Some(delta), ..
        } = message
            && delta.contains("(voi)")
        {
            count = count.saturating_add(1);
        }
    }
    Ok(count)
}

/// Os cinco cenários canónicos do A/B.
fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "duplicate_read",
            calls: vec![
                ("read", json!({ "path": "a.txt" })),
                ("read", json!({ "path": "a.txt" })),
            ],
        },
        Scenario {
            name: "distinct_reads",
            calls: vec![
                ("read", json!({ "path": "a.txt" })),
                ("read", json!({ "path": "b.txt" })),
            ],
        },
        Scenario {
            name: "irreconstructible_write",
            calls: vec![
                ("write", json!({ "path": "a.txt", "content": "x" })),
                ("write", json!({ "path": "a.txt", "content": "y" })),
            ],
        },
        Scenario {
            name: "mutation_invalidates",
            calls: vec![
                ("read", json!({ "path": "a.txt" })),
                ("write", json!({ "path": "b.txt", "content": "x" })),
                ("read", json!({ "path": "a.txt" })),
            ],
        },
        Scenario {
            name: "repeated_grep",
            calls: vec![
                ("grep", json!({ "pattern": "fn main", "path": "src" })),
                ("grep", json!({ "pattern": "fn main", "path": "src" })),
            ],
        },
    ]
}

/// Serializa o artefacto (JSON determinístico).
fn artifact_json(scenarios: &[Scenario], avoided: u32, executed: u32) -> serde_json::Value {
    let total = avoided.saturating_add(executed);
    json!({
        "schema": "katu.bench.voi.v1",
        "question": "o gate de VOI evita tool calls sem saltar o irreconstruível",
        "rule": "não chamar quando VOI = 0 < custo (só-leitura já satisfeita); nunca o irreconstruível",
        "scenarios": scenarios.iter().map(|s| {
            let mut voi = Voi::new();
            let skipped = s
                .calls
                .iter()
                .filter(|(n, a)| voi.decide(n, a) == Decision::Skip)
                .count();
            json!({ "name": s.name, "calls": s.calls.len(), "avoided": skipped })
        }).collect::<Vec<_>>(),
        "totals": { "calls": total, "avoided": avoided, "executed": executed },
        "criterion": "evitar só-leitura duplicada e nunca saltar o irreconstruível",
        "criterion_met": avoided >= 2 && executed >= 3,
        "caveat": "proxy determinístico (nenhum modelo local emite tool calls nativas): mede o que o gate evita, não o que o modelo perderia; a adoção por omissão exige A/B com o modelo",
        "decision": "ligado por config com `behavior.tool_voi = true` **e** seleção `suffix` (com `utility` o gate não atua: podia descartar a unidade lida)",
    })
}

#[test]
fn the_gate_can_be_disabled_by_config() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("voi-off")?;
    std::fs::create_dir_all(root.join(".katu"))?;
    std::fs::write(
        root.join(".katu").join("katu.toml"),
        "behavior.context_selection = \"suffix\"\nbehavior.tool_voi = false\n",
    )?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "lê o ficheiro")?;
    let provider: std::sync::Arc<dyn Provider> = std::sync::Arc::new(FakeProvider::new(
        "fake",
        vec![
            Turn {
                events: vec![read("c1", "a.txt"), read("c2", "a.txt")],
                stop: StopReason::ToolCalls,
            },
            Turn::text("fim"),
        ],
    ));
    let ports = Ports {
        fs: &fs,
        process: &StdProcess,
        env: &StdEnv,
    };
    run_turn(
        &mut runtime,
        request(&provider, ports, "lê o ficheiro", &options(4)),
    )?;
    assert_eq!(
        skipped_results(&runtime)?,
        0,
        "com o gate desligado, ambas as leituras executam"
    );
    Ok(())
}

#[test]
fn the_gate_is_on_with_suffix_selection() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("voi-suffix")?;
    std::fs::create_dir_all(root.join(".katu"))?;
    std::fs::write(
        root.join(".katu").join("katu.toml"),
        "behavior.context_selection = \"suffix\"\nbehavior.tool_voi = true\n",
    )?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "lê o ficheiro")?;
    let provider: std::sync::Arc<dyn Provider> = std::sync::Arc::new(FakeProvider::new(
        "fake",
        vec![
            Turn {
                events: vec![read("c1", "a.txt"), read("c2", "a.txt")],
                stop: StopReason::ToolCalls,
            },
            Turn::text("fim"),
        ],
    ));
    let ports = Ports {
        fs: &fs,
        process: &StdProcess,
        env: &StdEnv,
    };
    run_turn(
        &mut runtime,
        request(&provider, ports, "lê o ficheiro", &options(4)),
    )?;
    assert_eq!(
        skipped_results(&runtime)?,
        1,
        "com `suffix` + `tool_voi`, a segunda leitura é saltada"
    );
    Ok(())
}

/// Com `utility` o gate **não** atua: a seleção pode descartar a unidade lida e o gate mentiria.
#[test]
fn the_gate_is_off_with_utility_selection() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("voi-utility")?;
    std::fs::create_dir_all(root.join(".katu"))?;
    std::fs::write(
        root.join(".katu").join("katu.toml"),
        "behavior.context_selection = \"utility\"\nbehavior.tool_voi = true\n",
    )?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "lê o ficheiro")?;
    let provider: std::sync::Arc<dyn Provider> = std::sync::Arc::new(FakeProvider::new(
        "fake",
        vec![
            Turn {
                events: vec![read("c1", "a.txt"), read("c2", "a.txt")],
                stop: StopReason::ToolCalls,
            },
            Turn::text("fim"),
        ],
    ));
    let ports = Ports {
        fs: &fs,
        process: &StdProcess,
        env: &StdEnv,
    };
    run_turn(
        &mut runtime,
        request(&provider, ports, "lê o ficheiro", &options(4)),
    )?;
    assert_eq!(
        skipped_results(&runtime)?,
        0,
        "com `utility`, o gate não atua"
    );
    Ok(())
}

/// Um cenário canónico: nome + chamadas (nome, argumentos).
struct Scenario {
    name: &'static str,
    calls: Vec<(&'static str, serde_json::Value)>,
}

/// Corre os cenários e devolve `(evitadas, executadas)`.
fn run_scenarios(scenarios: &[Scenario]) -> (u32, u32) {
    let mut avoided = 0_u32;
    let mut executed = 0_u32;
    for scenario in scenarios {
        let mut voi = Voi::new();
        for (name, args) in &scenario.calls {
            match voi.decide(name, args) {
                Decision::Skip => avoided = avoided.saturating_add(1),
                Decision::Execute => executed = executed.saturating_add(1),
            }
        }
    }
    (avoided, executed)
}

/// A/B determinístico do gate (A3/W8-4): escreve o artefacto em `KATU_VOI_OUT`.
#[test]
#[ignore = "bench A/B: escreve o artefacto em KATU_VOI_OUT"]
#[allow(
    clippy::disallowed_methods,
    reason = "bench `#[ignore]`: escreve o artefacto do protocolo (a via normal é o gate)"
)]
fn ab_voi_by_artifact() -> Result<(), Box<dyn std::error::Error>> {
    let scenarios = scenarios();
    let (avoided, executed) = run_scenarios(&scenarios);
    let value = artifact_json(&scenarios, avoided, executed);
    let text = serde_json::to_string_pretty(&value)?;
    if let Ok(path) = std::env::var("KATU_VOI_OUT") {
        std::fs::write(&path, format!("{text}\n"))?;
    }
    let parsed: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(parsed.get("criterion_met"), Some(&json!(true)));
    Ok(())
}

/// A/B de CI: o gate evita a duplicada e nunca o irreconstruível.
#[test]
fn the_gate_avoids_duplicates_and_keeps_the_irreconstructible() {
    let scenarios = vec![
        Scenario {
            name: "duplicate_read",
            calls: vec![
                ("read", json!({ "path": "a.txt" })),
                ("read", json!({ "path": "a.txt" })),
            ],
        },
        Scenario {
            name: "irreconstructible_write",
            calls: vec![
                ("write", json!({ "path": "a.txt", "content": "x" })),
                ("write", json!({ "path": "a.txt", "content": "y" })),
            ],
        },
    ];
    let (avoided, executed) = run_scenarios(&scenarios);
    assert_eq!(avoided, 1, "só a leitura duplicada é evitada");
    assert_eq!(
        executed, 3,
        "as duas escritas e a primeira leitura executam"
    );
}
