//! A/B determinístico do guard de loop (Q-12/F7) — escreve o artefacto do protocolo.
//!
//! Mede o que o plano exige a um detetor: **falso positivo medido**, **alarme antes do teto** e
//! **determinismo**. As sequências são geradas de forma determinística (nenhum RNG, nenhum relógio)
//! e alimentadas pelo detector de produção ([`Guard::observe`]).
//!
//! O "turno normal" imita um turno real: passos com chamadas **diversas** (caminhos diferentes,
//! leitura + escrita), sem repetição pura. O "loop" repete a mesma chamada de leitura.

use super::{Call, Fingerprint, Guard, GuardParams};
use serde_json::json;

/// Forma do turno sintético.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Turn {
    /// Chamadas diversas, com progresso a cada 5 passos.
    Normal,
    /// A mesma leitura, sempre.
    Loop,
}

/// Passos de um turno **normal** (diverso): uma leitura nova e, a cada 5 passos, uma escrita.
fn normal_step(index: u32) -> Vec<Call> {
    let path = format!("/work/src/module{index}.rs");
    let read = Fingerprint::of("read", &json!({ "path": path }));
    if index % 5 == 4 {
        let write = Fingerprint::of("write", &json!({ "path": path, "content": "x" }));
        vec![Call::shared(read), Call::exclusive(write)]
    } else {
        vec![Call::shared(read)]
    }
}

/// Passos de um turno em **loop**: a mesma leitura, sempre.
fn loop_step() -> Vec<Call> {
    vec![Call::shared(Fingerprint::of(
        "read",
        &json!({ "path": "/work/a.rs" }),
    ))]
}

/// Corre `steps` passos e devolve o passo do primeiro alarme (0 = nenhum).
fn first_alarm(turn: Turn, steps: u32) -> u32 {
    let mut guard = Guard::with_defaults();
    for index in 0..steps {
        let calls = match turn {
            Turn::Loop => loop_step(),
            Turn::Normal => normal_step(index),
        };
        if let Some(alarm) = guard.observe(&calls) {
            return alarm.step;
        }
    }
    0
}

/// Falsos positivos em `turns` turnos normais de `steps` passos.
fn false_positives(turns: u32, steps: u32) -> u32 {
    (0..turns)
        .filter(|_| first_alarm(Turn::Normal, steps) != 0)
        .count()
        .try_into()
        .unwrap_or(u32::MAX)
}

/// Serializa o artefacto (JSON determinístico).
///
/// # Errors
/// Se a serialização falhar.
pub(super) fn measure() -> Result<String, Box<dyn std::error::Error>> {
    let turns = 200_u32;
    let steps = 12_u32;
    let false_positive = false_positives(turns, steps);
    let normal = first_alarm(Turn::Normal, steps);
    let loop_alarm = first_alarm(Turn::Loop, steps);
    let value = json!({
        "schema": "katu.bench.loop.v1",
        "question": "o guard corta o loop antes do teto sem cortar um turno normal",
        "params": {
            "cusum_k_milli": GuardParams::DEFAULT.cusum_k_milli,
            "cusum_h_milli": GuardParams::DEFAULT.cusum_h_milli,
            "sprt_p0_milli": GuardParams::DEFAULT.sprt_p0_milli,
            "sprt_p1_milli": GuardParams::DEFAULT.sprt_p1_milli,
            "sprt_alpha_milli": GuardParams::DEFAULT.sprt_alpha_milli,
            "sprt_beta_milli": GuardParams::DEFAULT.sprt_beta_milli,
            "min_steps": GuardParams::DEFAULT.min_steps,
        },
        "false_positives": {
            "turns": turns,
            "steps_per_turn": steps,
            "count": false_positive,
            "rate_milli": false_positive.saturating_mul(1_000).checked_div(turns).unwrap_or(0),
        },
        "normal_turn_first_alarm_step": normal,
        "loop_turn_first_alarm_step": loop_alarm,
        "global_step_cap": steps,
        "criterion": "falso positivo = 0 em 200 turnos normais e alarme do loop antes do teto de passos",
        "criterion_met": false_positive == 0 && loop_alarm > 0 && loop_alarm < steps,
        "caveat": "sequências sintéticas determinísticas (nenhum modelo local emite tool calls nativas): mede o detector, não o comportamento de um modelo real. O teto global de passos é configurável (`--max-steps`); aqui usa-se 12 como referência",
    });
    Ok(serde_json::to_string_pretty(&value)?)
}

/// A/B de CI: falso positivo zero, alarme cedo e determinismo.
#[test]
fn the_guard_has_no_false_positive_on_normal_turns() {
    assert_eq!(false_positives(200, 12), 0);
    assert_eq!(first_alarm(Turn::Normal, 12), 0);
    assert_eq!(first_alarm(Turn::Loop, 12), 4);
    assert_eq!(
        first_alarm(Turn::Loop, 12),
        first_alarm(Turn::Loop, 12),
        "determinístico"
    );
}

/// A/B manual: `KATU_LOOP_OUT=$PWD/bench/e18/loop/raw.json cargo test -q -p katu-core --lib --
/// --ignored ab_loop_guard`.
#[test]
#[ignore = "bench A/B: escreve o artefacto em KATU_LOOP_OUT"]
#[allow(
    clippy::disallowed_methods,
    reason = "bench `#[ignore]`: escreve o artefacto do protocolo (a via normal é o gate)"
)]
fn ab_loop_guard() -> Result<(), Box<dyn std::error::Error>> {
    let text = measure()?;
    if let Ok(path) = std::env::var("KATU_LOOP_OUT") {
        std::fs::write(&path, format!("{text}\n"))?;
    }
    let value: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(value.get("criterion_met"), Some(&json!(true)));
    Ok(())
}
