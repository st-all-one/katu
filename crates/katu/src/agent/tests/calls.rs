//! Tool calls por passo (B-02) e A/B do paralelismo in-process (B-01).
//!
//! B-02 **já era suportado**: um passo pode trazer várias calls e todas correm, na ordem do
//! modelo, cada uma pela ordem §42. O paralelismo entre calls foi medido e **rejeitado**
//! (`bench/e18/batch/PROTOCOL.md`): o efeito real da tool `read` é *CPU-bound*, pelo que o pool só
//! acrescenta contenção.

use std::hint::black_box;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use katu_core::kernel::{CallId, Message, Tool};
use katu_core::ports::{FixedClock, Timestamp};
use katu_core::provider::{Provider, ProviderEvent, StopReason};
use katu_policy::{ResolvedPath, ToolArgs, ToolName, ToolUse};
use katu_providers::{FakeProvider, Turn};
use katu_tools::read::{ReadBudget, ReadTool, View};
use serde_json::json;

use super::{Ports, options, request, root};
use crate::agent::run_turn;
use crate::agent::turn::PARALLEL_BATCHES;
use crate::ports::{StdEnv, StdFs, StdProcess};
use crate::runtime::Runtime;

/// Um pedido de `read`.
fn read_call(id: &str, path: &str) -> ProviderEvent {
    ProviderEvent::ToolCall {
        call: CallId::new(id),
        name: "read".to_string(),
        arguments: json!({"path": path}),
    }
}

/// Um pedido de `write`.
fn write_call(id: &str, path: &str) -> ProviderEvent {
    ProviderEvent::ToolCall {
        call: CallId::new(id),
        name: "write".to_string(),
        arguments: json!({"path": path, "content": "x"}),
    }
}

/// A ordem de `ToolCall`/`ToolResult` no log (a sequência que o modelo viu).
fn log_order(runtime: &Runtime<'_>) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut order = Vec::new();
    for message in runtime.messages()? {
        match message {
            Message::ToolCall { call, .. } => order.push(format!("call:{}", call.as_str())),
            Message::ToolResult { call, .. } => order.push(format!("result:{}", call.as_str())),
            _ => {}
        }
    }
    Ok(order)
}

#[test]
fn a_step_with_several_calls_runs_them_all_in_the_model_order()
-> Result<(), Box<dyn std::error::Error>> {
    let root = root("multi-call")?;
    std::fs::write(root.join("a.txt"), "AAA")?;
    std::fs::write(root.join("b.txt"), "BBB")?;
    std::fs::write(root.join("c.txt"), "CCC")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "lê três ficheiros")?;
    let provider: std::sync::Arc<dyn Provider> = std::sync::Arc::new(FakeProvider::new(
        "fake",
        vec![
            Turn {
                events: vec![
                    read_call("r1", "a.txt"),
                    read_call("r2", "b.txt"),
                    read_call("r3", "c.txt"),
                ],
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

    let report = run_turn(
        &mut runtime,
        request(&provider, ports, "lê três ficheiros", &options(4)),
    )?;
    assert_eq!(report.calls, 3, "as três calls do passo correm");
    assert_eq!(
        log_order(&runtime)?,
        vec![
            "call:r1",
            "call:r2",
            "call:r3",
            "result:r1",
            "result:r2",
            "result:r3",
        ],
        "o lote loga os pedidos antes dos resultados, na ordem do modelo"
    );
    runtime.session().verify()?;

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

#[test]
fn an_exclusive_call_is_a_barrier() -> Result<(), Box<dyn std::error::Error>> {
    let root = root("mixed-call")?;
    std::fs::write(root.join("a.txt"), "AAA")?;
    std::fs::write(root.join("b.txt"), "BBB")?;
    let fs = StdFs;
    let clock = FixedClock::new(Timestamp::from_millis(1_000));
    let mut runtime = Runtime::open(&fs, &clock, &root, "lote e barreira")?;
    let provider: std::sync::Arc<dyn Provider> = std::sync::Arc::new(FakeProvider::new(
        "fake",
        vec![
            Turn {
                events: vec![
                    read_call("r1", "a.txt"),
                    read_call("r2", "b.txt"),
                    write_call("w1", "out.txt"),
                    read_call("r3", "a.txt"),
                ],
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

    let report = run_turn(
        &mut runtime,
        request(&provider, ports, "lote e barreira", &options(4)),
    )?;
    assert_eq!(report.calls, 4);
    assert_eq!(
        log_order(&runtime)?,
        vec![
            "call:r1",
            "call:r2",
            "result:r1",
            "result:r2",
            "call:w1",
            "result:w1",
            "call:r3",
            "result:r3",
        ],
        "o lote é esvaziado antes da call exclusiva (barreira)"
    );
    runtime.session().verify()?;

    drop(runtime);
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

/// `ToolUse` de leitura resolvido (para exercitar a tool `read` diretamente).
fn read_use(root: &Path, path: &Path) -> Result<ToolUse, Box<dyn std::error::Error>> {
    Ok(ToolUse {
        name: ToolName::Read,
        args: ToolArgs::Read {
            path: ResolvedPath::from_canonical(path)?,
        },
        resolved_paths: vec![ResolvedPath::from_canonical(path)?],
        argv: None,
        cwd: ResolvedPath::from_canonical(root)?,
    })
}

/// A/B cru de B-01: o **efeito real** da tool `read` (view `summary`), sequencial vs paralelo.
///
/// Mede wall-clock, pelo que é `#[ignore]` (não determinístico): corre-se com
/// `cargo test -p katu --bin katu ab_read_effect -- --ignored --nocapture`. Números em
/// `bench/e18/batch/raw.json`.
#[test]
#[ignore = "mede wall-clock (A/B de B-01); correr com --ignored --nocapture"]
#[allow(
    clippy::disallowed_methods,
    reason = "medição de wall-clock; o teste é `#[ignore]` e não entra no determinismo do CI"
)]
fn ab_read_effect_in_process_parallelism() -> Result<(), Box<dyn std::error::Error>> {
    const REPS: usize = 5;
    const FILES: usize = 8;
    let root = root("read-ab")?;
    let body = "linha de teste com algum comprimento\n".repeat(35_000);
    let fs = StdFs;
    let mut uses = Vec::with_capacity(FILES);
    for index in 0..FILES {
        let path = root.join(format!("f{index}.txt"));
        std::fs::write(&path, &body)?;
        uses.push(read_use(&root, &path)?);
    }
    let tool = ReadTool {
        fs: &fs,
        view: View::Summary,
        range: None,
        symbol: None,
        base: None,
        budget: ReadBudget::default(),
    };
    // Aquece a page-cache: mede-se o paralelismo, não o primeiro toque do disco.
    for use_ in &uses {
        black_box(tool.execute(use_));
    }

    let mut sequential = Vec::with_capacity(REPS);
    let mut parallel = Vec::with_capacity(REPS);
    for _ in 0..REPS {
        let start = Instant::now();
        for use_ in &uses {
            black_box(tool.execute(use_));
        }
        sequential.push(start.elapsed());

        let start = Instant::now();
        let tool_ref = &tool;
        std::thread::scope(|scope| {
            let handles: Vec<_> = uses
                .iter()
                .map(|use_| scope.spawn(move || black_box(tool_ref.execute(use_))))
                .collect();
            for handle in handles {
                drop(handle.join());
            }
        });
        parallel.push(start.elapsed());
    }

    let sequential_p50 = p50(sequential);
    let parallel_p50 = p50(parallel);
    let gain_pct = if sequential_p50 > 0.0 {
        (sequential_p50 - parallel_p50) / sequential_p50 * 100.0
    } else {
        0.0
    };
    emit(&format!(
        "{{\"schema\":\"katu.bench.batch.v1\",\"question\":\"read_effect\",\"files\":{FILES},\"reps\":{REPS},\"sequential_p50_ms\":{sequential_p50:.3},\"parallel_p50_ms\":{parallel_p50:.3},\"gain_pct\":{gain_pct:.1}}}"
    ))?;

    std::fs::remove_dir_all(&root)?;
    Ok(())
}

/// Escreve a medição no caminho indicado por `KATU_BATCH_OUT` (opt-in).
///
/// `check-diag` proíbe `println!`/`eprintln!` (só o sink de diagnóstico escreve): o A/B é manual e
/// deposita o resultado num ficheiro, que é o artefacto publicado.
fn emit(json: &str) -> Result<(), std::io::Error> {
    let Some(path) = std::env::var_os("KATU_BATCH_OUT") else {
        return Ok(());
    };
    std::fs::write(path, format!("{json}\n"))
}

/// Mediana em milissegundos.
fn p50(mut samples: Vec<Duration>) -> f64 {
    samples.sort_unstable();
    samples
        .get(samples.len() / 2)
        .map_or(0.0, |value| value.as_secs_f64() * 1_000.0)
}

/// A/B **end-to-end** de B-01: N leituras num passo (lote) vs N passos sequenciais.
///
/// Afirma que o caminho paralelo correu (senão a medição é inválida). `#[ignore]` porque mede
/// wall-clock; números em `bench/e18/batch/raw.json`.
#[test]
#[ignore = "mede wall-clock (A/B de B-01); correr com --ignored --nocapture"]
#[allow(
    clippy::disallowed_methods,
    reason = "medição de wall-clock; o teste é `#[ignore]` e não entra no determinismo do CI"
)]
#[allow(
    clippy::too_many_lines,
    reason = "o A/B tem três cenários (fixo, lote, sequencial) e a limpeza; partir em helpers só esconderia a medição"
)]
fn ab_end_to_end_batch() -> Result<(), Box<dyn std::error::Error>> {
    const REPS: usize = 5;
    const N: usize = 8;
    let root = root("batch-ab")?;
    let body = "linha de teste com algum comprimento\n".repeat(35_000);
    for index in 0..N {
        std::fs::write(root.join(format!("f{index}.txt")), &body)?;
    }
    let fs = StdFs;
    let process = StdProcess;
    let env = StdEnv;
    let ports = Ports {
        fs: &fs,
        process: &process,
        env: &env,
    };
    for index in 0..N {
        std::fs::read(root.join(format!("f{index}.txt")))?;
    }

    let mut batched = Vec::with_capacity(REPS);
    let mut sequential = Vec::with_capacity(REPS);
    let mut fixed = Vec::with_capacity(REPS);
    for rep in 0..REPS {
        // C — um passo sem tool calls (custo fixo do turno: abertura, log, pedido).
        let clock = FixedClock::new(Timestamp::from_millis(
            1_000 + u64::try_from(rep).unwrap_or(u64::MAX) * 10 + 1,
        ));
        let goal = format!("fixo-{rep}");
        let mut runtime = Runtime::open(&fs, &clock, &root, &goal)?;
        let provider: std::sync::Arc<dyn Provider> =
            std::sync::Arc::new(FakeProvider::new("fake", vec![Turn::text("fim")]));
        let start = Instant::now();
        run_turn(&mut runtime, request(&provider, ports, &goal, &options(16)))?;
        fixed.push(start.elapsed());
        drop(runtime);

        // A — um passo com N reads (lote paralelo).
        let clock = FixedClock::new(Timestamp::from_millis(
            1_000 + u64::try_from(rep).unwrap_or(u64::MAX) * 10,
        ));
        let goal = format!("lote-{rep}");
        let mut runtime = Runtime::open(&fs, &clock, &root, &goal)?;
        let events: Vec<_> = (0..N)
            .map(|index| read_call(&format!("a{index}"), &format!("f{index}.txt")))
            .collect();
        let provider: std::sync::Arc<dyn Provider> = std::sync::Arc::new(FakeProvider::new(
            "fake",
            vec![
                Turn {
                    events,
                    stop: StopReason::ToolCalls,
                },
                Turn::text("fim"),
            ],
        ));
        let before = PARALLEL_BATCHES.load(Ordering::Relaxed);
        let start = Instant::now();
        run_turn(&mut runtime, request(&provider, ports, &goal, &options(16)))?;
        batched.push(start.elapsed());
        let after = PARALLEL_BATCHES.load(Ordering::Relaxed);
        assert!(
            after > before,
            "o lote paralelo não correu (fallback de orçamento?)"
        );
        drop(runtime);

        // B — N passos com uma read cada (sequencial).
        let clock = FixedClock::new(Timestamp::from_millis(
            1_005 + u64::try_from(rep).unwrap_or(u64::MAX) * 10,
        ));
        let goal = format!("sequencial-{rep}");
        let mut runtime = Runtime::open(&fs, &clock, &root, &goal)?;
        let mut turns: Vec<Turn> = (0..N)
            .map(|step| Turn {
                events: vec![read_call(&format!("s{step}"), &format!("f{step}.txt"))],
                stop: StopReason::ToolCalls,
            })
            .collect();
        turns.push(Turn::text("fim"));
        let provider: std::sync::Arc<dyn Provider> =
            std::sync::Arc::new(FakeProvider::new("fake", turns));
        let start = Instant::now();
        run_turn(&mut runtime, request(&provider, ports, &goal, &options(16)))?;
        sequential.push(start.elapsed());
        drop(runtime);
    }

    let batched_p50 = p50(batched);
    let sequential_p50 = p50(sequential);
    let fixed_p50 = p50(fixed);
    let gain_pct = if sequential_p50 > 0.0 {
        (sequential_p50 - batched_p50) / sequential_p50 * 100.0
    } else {
        0.0
    };
    emit(&format!(
        "{{\"schema\":\"katu.bench.batch.v1\",\"question\":\"end_to_end\",\"reads\":{N},\"reps\":{REPS},\"fixed_p50_ms\":{fixed_p50:.3},\"batched_p50_ms\":{batched_p50:.3},\"sequential_p50_ms\":{sequential_p50:.3},\"gain_pct\":{gain_pct:.1}}}"
    ))?;

    std::fs::remove_dir_all(&root)?;
    Ok(())
}
