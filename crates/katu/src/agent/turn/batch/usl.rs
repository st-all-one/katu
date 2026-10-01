//! Curva USL do pool de lote (E1): wall-clock vs threads, para justificar o teto de concorrência.
//!
//! **Pergunta.** O teto de `MAX_PARALLEL_CALLS` foi escolhido por um limite de *custo* (PTC), não
//! por medição de escalabilidade. Quantas threads é que o mecanismo `spawn`/`join` realmente
//! aproveita, e a partir de onde a curva satura?
//!
//! **Fórmula.** Speedup de Amdahl e eficiência, com `T` threads e `N` tarefas independentes:
//!
//! ```
//! S(T) = T_wall(1) / T_wall(T)          speedup
//! E(T) = S(T) / T                      eficiência (1 = escala linear)
//! ```
//!
//! **O que se mede.** O *mecanismo* — `std::thread::scope` com spawn eager, o mesmo do
//! `in_parallel` — sobre uma carga sintética de granularidade conhecida. O `dispatch` real é uma
//! avaliação de política de **microssegundos**: medir a escalabilidade dele seria medir o ruído. Por
//! isso a carga é calibrada ([`WORK_PER_TASK`]) e o número publicado é sobre o *pool*, não sobre a
//! tool.
//!
//! **Repetições.** Mediana de [`REPEATS`] com o mesmo trabalho (sem RNG): o número tem de sobreviver
//! ao ruído do agendador.

use std::time::Instant;

use serde_json::json;

use super::MAX_PARALLEL_CALLS;

/// Repetições por ponto da curva (mediana).
pub(super) const REPEATS: usize = 7;

/// Tarefas independentes por ponto (o lote).
pub(super) const TASKS: usize = 64;

/// Carga sintética por tarefa (iterações de um acumulador inteiro — sem ponto flutuante).
const WORK_PER_TASK: u64 = 200_000;

/// Threads sondadas na curva (inclui o teto de produção, 8).
pub(super) const LADDER: [usize; 6] = [1, 2, 3, 4, 6, 8];

/// Uma execução de `tasks` tarefas em `threads` threads; devolve microssegundos.
#[allow(
    clippy::disallowed_methods,
    reason = "bench: o objecto do E1 é medir wall-clock — o port Clock não mede tempo real"
)]
fn run(tasks: usize, threads: usize) -> u64 {
    let _span = katu_core::trace_fn!("batch::usl::run");

    let start = Instant::now();
    let mut slot = 0_u64;
    std::thread::scope(|scope| {
        // Spawn **eager** (a lição do B-01: encadear spawn/join serializa o lote).
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(move || {
                    let mut acc = slot;
                    for task in 0..tasks {
                        acc = burn(acc, task);
                    }
                    acc
                })
            })
            .collect();
        for handle in handles {
            slot = slot.wrapping_add(handle.join().unwrap_or(0));
        }
    });
    std::hint::black_box(slot);
    u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX)
}

/// Carga determinística por tarefa (números de FNV, sem alocação, sem ponto flutuante).
fn burn(acc: u64, task: usize) -> u64 {
    let mut index = acc.wrapping_add(u64::try_from(task).unwrap_or(0));
    for _ in 0..WORK_PER_TASK {
        index = index
            .wrapping_mul(0x0100_0000_01b3)
            .wrapping_add(0x9e37_79b9_7f4a_7c15);
        index ^= index >> 29;
    }
    acc.wrapping_add(index)
}

/// Mediana de [`REPEATS`] (sem alocação: `sort_unstable` sobre um array de tamanho fixo).
fn median(threads: usize) -> u64 {
    let mut samples = [0_u64; REPEATS];
    for sample in &mut samples {
        *sample = run(TASKS, threads);
    }
    samples.sort_unstable();
    samples[REPEATS / 2]
}

/// Divisão em f64, arredondada a milésimos (escala `milli`, como o resto do bench).
#[allow(
    clippy::as_conversions,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "razão de tempos já limitada a [0, 1e6]; o arredondamento em milésimos é o pretendido (precedente: `evidence::from_f64`)"
)]
fn ratio_milli(numerator: u64, denominator: u64) -> u64 {
    if denominator == 0 {
        return 0;
    }
    let quotient = numerator as f64 / denominator as f64;
    (quotient * 1_000.0).round() as u64
}

/// Um ponto da curva.
fn point(threads: usize, wall_us: u64, base_us: u64) -> serde_json::Value {
    let speedup_milli = ratio_milli(base_us, wall_us);
    let threads_milli = 1_000_u64.saturating_mul(u64::try_from(threads).unwrap_or(u64::MAX));
    json!({
        "threads": threads,
        "wall_us": wall_us,
        "speedup_milli": speedup_milli,
        "efficiency_milli": ratio_milli(speedup_milli, threads_milli),
    })
}

/// A/B do E1: escreve `bench/e18/pool/raw.json` em `KATU_POOL_OUT`.
#[test]
#[ignore = "bench E1: escreve a curva USL em KATU_POOL_OUT (a via normal é o gate)"]
#[allow(
    clippy::disallowed_methods,
    clippy::too_many_lines,
    reason = "bench `#[ignore]`: o artefacto é uma tabela; partir a conta escondia-a"
)]
fn ab_pool_usl_by_artifact() -> Result<(), Box<dyn std::error::Error>> {
    let base = median(1);
    let curve: Vec<serde_json::Value> = LADDER
        .iter()
        .map(|&threads| point(threads, median(threads), base))
        .collect();

    // Saturação: o primeiro ponto com eficiência < 500 ‰ (metade do ideal).
    let saturating = curve.iter().find(|point| {
        point
            .get("efficiency_milli")
            .and_then(serde_json::Value::as_u64)
            .is_some_and(|efficiency| efficiency < 500)
    });
    let ceiling = MAX_PARALLEL_CALLS;
    let ceiling_efficiency = curve
        .iter()
        .find(|point| {
            point
                .get("threads")
                .and_then(serde_json::Value::as_u64)
                .is_some_and(|threads| usize::try_from(threads) == Ok(ceiling))
        })
        .and_then(|point| point.get("efficiency_milli").cloned());

    let parallelism = std::thread::available_parallelism().map_or(0, std::num::NonZero::get);
    // Diagnóstico do ambiente: se 2 threads não dão speedup 1,5x, a máquina **não** está a
    // escalar (cgroup/sandbox), e o número descreve o ambiente, não o pool. Registado em vez de
    //publishado como se fosse escalabilidade do katu.
    let speedup_at_two = curve
        .iter()
        .find(|point| point.get("threads").and_then(serde_json::Value::as_u64) == Some(2))
        .and_then(|point| {
            point
                .get("speedup_milli")
                .and_then(serde_json::Value::as_u64)
        })
        .unwrap_or(0);
    let scaling_suspect = speedup_at_two < 1_500;
    let value = json!({
        "schema": "katu.bench.pool.v1",
        "available_parallelism": parallelism,
        "question": "quantas threads o pool de lote aproveita (curva USL)",
        "rule": "S(T) = T_wall(1)/T_wall(T); E(T) = S(T)/T; saturacao = primeiro T com E < 500 per-mil",
        "tasks": TASKS,
        "work_per_task": WORK_PER_TASK,
        "repeats": REPEATS,
        "ceiling_in_production": ceiling,
        "curve": curve,
        "saturating_threads": saturating.and_then(|point| point.get("threads").cloned()),
        "speedup_at_2_threads_milli": speedup_at_two,
        "scaling_suspect": scaling_suspect,
        "environment": if scaling_suspect {
            "a maquina nao escala (2 threads nao dao 1,5x): o numero mede o ambiente, nao o pool do katu"
        } else {
            "a maquina escala: o numero e' atribuivel ao mecanismo spawn/join"
        },
        "criterion": "o teto de producao (8) tem de estar medido na curva",
        "criterion_met": ceiling_efficiency.is_some(),
        "caveat": "mede o mecanismo spawn/join com carga sintetica calibrada, nao o dispatch real (avaliacao de politica de microssegundos)",
        "decision": "",
    });
    let text = serde_json::to_string_pretty(&value)?;
    if let Ok(path) = std::env::var("KATU_POOL_OUT") {
        std::fs::write(&path, format!("{text}\n"))?;
    }
    let parsed: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(parsed.get("criterion_met"), Some(&json!(true)));
    Ok(())
}

/// Invariante de CI: o teto de produção está na curva medida (a medição cobre a produção).
#[test]
fn the_production_ceiling_is_inside_the_measured_ladder() {
    assert!(
        LADDER.contains(&MAX_PARALLEL_CALLS),
        "o teto {MAX_PARALLEL_CALLS} não está na curva medida {LADDER:?}"
    );
}
