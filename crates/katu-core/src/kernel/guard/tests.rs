//! Testes do guard de loop (Q-12): alarme cedo, progresso reinicia, determinismo, falso positivo.

use serde_json::json;

use super::{Alarm, AlarmKind, Call, Fingerprint, Guard, GuardParams};

mod bench;
mod optional;

/// Uma chamada de leitura com o argumento `path`.
fn read(path: &str) -> Fingerprint {
    Fingerprint::of("read", &json!({ "path": path }))
}

/// Uma chamada de escrita (progresso).
fn write(path: &str) -> Fingerprint {
    Fingerprint::of("write", &json!({ "path": path, "content": "x" }))
}

/// Observa a mesma chamada `total` vezes e devolve o primeiro alarme.
fn repeat(guard: &mut Guard, print: Fingerprint, total: u32) -> Option<Alarm> {
    let mut alarm = None;
    for _ in 0..total {
        if let Some(found) = guard.observe(&[Call::shared(print)]) {
            alarm = Some(found);
            break;
        }
    }
    alarm
}

#[test]
fn a_pure_loop_is_cut_early() {
    let mut guard = Guard::with_defaults();
    let alarm = repeat(&mut guard, read("/work/a.rs"), 10);
    assert!(alarm.is_some(), "um ciclo puro tem de alarmar");
    let Some(alarm) = alarm else {
        return;
    };
    assert_eq!(alarm.kind, AlarmKind::EValue);
    assert_eq!(alarm.step, 5, "alarme no 5.º passo, muito antes do teto");
    assert_eq!(alarm.repeated, 4);
    assert_eq!(alarm.novelty_milli, 0);
    assert!(alarm.reason().contains("loop detectado"));
}

#[test]
fn the_cusum_catches_a_partial_loop() {
    // Cada passo relê **duas** chamadas já vistas e acrescenta uma nova: a novidade nunca é nula
    // (a e-value não dispara) mas a fração de repetição fica acima do alvo `k` e o CUSUM acumula.
    let mut guard = Guard::with_defaults();
    let mut alarm = None;
    let first = read("/work/0.rs");
    let second = read("/work/1.rs");
    for index in 2..30_u32 {
        let current = read(&format!("/work/{index}.rs"));
        let prints = [
            Call::shared(first),
            Call::shared(second),
            Call::shared(current),
        ];
        if let Some(found) = guard.observe(&prints) {
            alarm = Some(found);
            break;
        }
    }
    assert!(
        alarm.is_some(),
        "repetição parcial sustentada tem de alarmar"
    );
    let Some(alarm) = alarm else {
        return;
    };
    assert_eq!(alarm.kind, AlarmKind::Cusum);
    assert!(alarm.novelty_milli > 0, "a e-value não era o detector aqui");
    assert!(alarm.cusum_milli >= GuardParams::DEFAULT.cusum_h_milli);
}

#[test]
fn progress_resets_the_detector() {
    let mut guard = Guard::with_defaults();
    // Duas repetições (abaixo do limiar), progresso, e o ciclo recomeça do zero.
    assert!(guard.observe(&[Call::shared(read("/work/a.rs"))]).is_none());
    assert!(guard.observe(&[Call::shared(read("/work/a.rs"))]).is_none());
    assert!(
        guard
            .observe(&[Call::exclusive(write("/work/a.rs"))])
            .is_none()
    );
    assert_eq!(guard.cusum_milli(), 0, "o progresso zera o CUSUM");
    assert!(
        guard.e_value_log_milli() < 0,
        "o progresso empurra a e-value para a hipótese normal"
    );
    // Um *polling* legítimo de `bash` (progresso) nunca é cortado, mesmo em ciclo puro.
    let mut polling = Guard::with_defaults();
    for _ in 0..50 {
        assert!(
            polling
                .observe(&[Call::exclusive(write("/work/make"))])
                .is_none()
        );
    }
    assert_eq!(polling.steps(), 50);
}

#[test]
fn a_diverse_turn_never_alarms() {
    let mut guard = Guard::with_defaults();
    for index in 0..40_u32 {
        let prints = [Call::shared(read(&format!("/work/{index}.rs")))];
        assert!(
            guard.observe(&prints).is_none(),
            "um turno diverso não pode alarmar"
        );
    }
    assert_eq!(guard.cusum_milli(), 0);
}

#[test]
fn a_short_turn_is_not_cut() {
    let mut guard = Guard::new(GuardParams {
        min_steps: 3,
        ..GuardParams::DEFAULT
    });
    // Um passo inteiramente repetido com `min_steps = 3` não pode alarmar no passo 1 nem 2.
    assert!(guard.observe(&[Call::shared(read("/work/a.rs"))]).is_none());
    assert!(guard.observe(&[Call::shared(read("/work/a.rs"))]).is_none());
    // Passos 3–4: `min_steps` já permite, mas a e-value ainda não cruzou `log(1/α)`.
    assert!(guard.observe(&[Call::shared(read("/work/a.rs"))]).is_none());
    assert!(guard.observe(&[Call::shared(read("/work/a.rs"))]).is_none());
    assert_eq!(
        guard
            .observe(&[Call::shared(read("/work/a.rs"))])
            .map(|a| a.step),
        Some(5)
    );
}

#[test]
fn an_empty_step_is_not_a_loop() {
    let mut guard = Guard::with_defaults();
    for _ in 0..20 {
        assert!(guard.observe(&[]).is_none());
    }
    assert_eq!(guard.cusum_milli(), 0);
}

#[test]
fn the_fingerprint_is_stable_and_argument_sensitive() {
    let first = Fingerprint::of("read", &json!({ "path": "/work/a.rs", "n": 1 }));
    let second = Fingerprint::of("read", &json!({ "n": 1, "path": "/work/a.rs" }));
    assert_eq!(
        first, second,
        "a ordem das chaves não pode mudar a impressão"
    );
    let other = Fingerprint::of("read", &json!({ "path": "/work/b.rs", "n": 1 }));
    assert_ne!(first, other);
    let name = Fingerprint::of("grep", &json!({ "path": "/work/a.rs", "n": 1 }));
    assert_ne!(first, name, "o nome da tool faz parte da impressão");
}

#[test]
fn the_detector_is_deterministic() {
    let mut first = Guard::with_defaults();
    let mut second = Guard::with_defaults();
    for _ in 0..4 {
        first.observe(&[Call::shared(read("/work/a.rs"))]);
        second.observe(&[Call::shared(read("/work/a.rs"))]);
    }
    assert_eq!(first.e_value_log_milli(), second.e_value_log_milli());
    assert_eq!(first.cusum_milli(), second.cusum_milli());
    assert_eq!(first.steps(), second.steps());
}

#[test]
fn a_loop_with_many_calls_per_step_is_still_caught() {
    let mut guard = Guard::with_defaults();
    let prints = [
        Call::shared(read("/work/a.rs")),
        Call::shared(read("/work/b.rs")),
    ];
    let mut alarm = None;
    for _ in 0..10 {
        if let Some(found) = guard.observe(&prints) {
            alarm = Some(found);
            break;
        }
    }
    assert!(
        alarm.is_some(),
        "um ciclo com várias chamadas tem de alarmar"
    );
    let Some(alarm) = alarm else {
        return;
    };
    assert_eq!(alarm.repeated, 4);
}
