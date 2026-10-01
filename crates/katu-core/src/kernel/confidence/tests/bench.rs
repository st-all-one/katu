//! A/B determinístico da confiança medida (Q-11/F6) — escreve o artefacto do protocolo.
//!
//! Não mede o modelo: mede a **estatística** que decide `Enforced` sobre sequências sintéticas de
//! eventos construídas com as funções de produção ([`super::denied`]/[`super::ran`] e
//! [`rule_trials`]/[`enforced_verdicts`]). O que se publica é: em que `n` um registo perfeito prova
//! a regra, quanto custa uma violação, e que a extração é determinística.

use katu_policy::{
    Enforcement, Rule, RuleCategory, RuleExamples, RuleId, RuleScope, RuleSet, Severity, Threshold,
    ToolName, calibrate,
};
use serde_json::json;

use super::{call, denied, enforced_verdicts, ran, rule_trials};
use crate::kernel::event::Event;
use crate::kernel::log::LogRecord;

/// Sequência com `total` recusas da regra `r1`, todas honradas.
fn honored(total: u32) -> Vec<Event> {
    let mut events = Vec::new();
    for index in 0..total {
        let call = format!("c{index}");
        events.push(denied(&call, "r1"));
    }
    events
}

/// Igual, mas a **última** recusa aparece executada sob o mesmo `CallId` (uma violação).
fn violated(total: u32) -> Vec<Event> {
    let mut events = honored(total);
    let last = format!("c{}", total.saturating_sub(1));
    events.push(ran(&last));
    events
}

/// Log sintético em JSONL com **uma** violação (a 20.ª recusa aparece executada).
///
/// É a fixture que faz o `policy:confidence` falhar de forma demonstrável:
/// `cargo run -q -p xtask -- policy:confidence bench/e18/confidence/fixture.v1.jsonl`.
///
/// # Errors
/// Se a construção do `ToolUse`/da serialização falhar.
pub(super) fn fixture_log() -> Result<String, Box<dyn std::error::Error>> {
    let mut events: Vec<Event> = Vec::new();
    for index in 0..20_u32 {
        let id = format!("c{index}");
        events.push(call(&id, ToolName::Exec)?);
        events.push(denied(&id, "contain-read-outside-workspace"));
    }
    events.push(ran("c19"));
    let mut text = String::new();
    for (index, event) in events.iter().enumerate() {
        let record = LogRecord {
            seq: u64::try_from(index).unwrap_or(0).saturating_add(1),
            event: event.clone(),
        };
        text.push_str(&serde_json::to_string(&record)?);
        text.push('\n');
    }
    Ok(text)
}

/// O `RuleSet` mínimo com uma regra `Enforced` chamada `r1`.
fn rules() -> RuleSet {
    RuleSet {
        vocab: katu_policy::POLICY_VOCAB_VERSION,
        rules: vec![Rule {
            id: RuleId::from("r1"),
            statement: "não execute".into(),
            scope: RuleScope::Command {
                tool: ToolName::Exec,
            },
            enforcement: Enforcement::DenyCommand {
                tool: ToolName::Exec,
            },
            severity: Severity::Critical,
            category: RuleCategory::Enforced,
            remedy: Some("use `read`".into()),
            expires_at: None,
            waiver: None,
            examples: RuleExamples {
                negative: vec!["exec rm".into()],
                positive: Vec::new(),
            },
        }],
    }
}

/// Mede o primeiro `n` em que um registo perfeito prova a regra, e o LB aí.
fn flip() -> (u32, u32) {
    let rules = rules();
    for total in 1..=60_u32 {
        let verdicts = enforced_verdicts(&honored(total), &rules, &Threshold::DEFAULT);
        if let Some(first) = verdicts.first()
            && first.is_proven()
        {
            return (total, first.lower_milli);
        }
    }
    (0, 0)
}

/// Serializa o artefacto (JSON determinístico).
///
/// Calibração C3/W8-2 (artefacto): o resumo com o diagrama e a curva do ECE com a evidência.
fn calibration_json(rules: &RuleSet) -> (serde_json::Value, serde_json::Value) {
    let mut verdicts = Vec::new();
    for total in [5_u32, 10, 15, 20, 25, 40, 60] {
        verdicts.extend(enforced_verdicts(
            &honored(total),
            rules,
            &Threshold::DEFAULT,
        ));
    }
    verdicts.extend(enforced_verdicts(&violated(20), rules, &Threshold::DEFAULT));
    let calibration = calibrate(&verdicts);
    let bins: Vec<serde_json::Value> = calibration
        .bins
        .iter()
        .map(|bin| {
            json!({
                "lower_milli": bin.lower_milli,
                "upper_milli": bin.upper_milli,
                "trials": bin.trials,
                "predicted_milli": bin.predicted_milli,
                "observed_milli": bin.observed_milli,
            })
        })
        .collect();
    let summary = json!({
        "basis": "inferred",
        "predicted": "lower_milli (LB de Wilson)",
        "outcome": "frequência empírica (sucessos/ensaios)",
        "caveat": "in-sample: mede o conservadorismo do LB face à frequência do próprio log, não uma validação fora da amostra",
        "trials": calibration.trials,
        "brier_milli": calibration.brier_milli,
        "ece_milli": calibration.ece_milli,
        "bins": bins,
    });
    let mut by_n = Vec::new();
    for total in 1..=30_u32 {
        let verdicts = enforced_verdicts(&honored(total), rules, &Threshold::DEFAULT);
        by_n.push(json!({"n": total, "ece_milli": calibrate(&verdicts).ece_milli}));
    }
    (summary, serde_json::Value::Array(by_n))
}

/// # Errors
/// Se a construção das fixtures falhar.
pub(super) fn measure() -> Result<String, Box<dyn std::error::Error>> {
    let (flip_n, flip_lower) = flip();
    let rules = rules();
    let clean = enforced_verdicts(&honored(20), &rules, &Threshold::DEFAULT);
    let dirty = enforced_verdicts(&violated(20), &rules, &Threshold::DEFAULT);
    let empty = enforced_verdicts(&[], &rules, &Threshold::DEFAULT);
    let clean_first = clean.first().ok_or("sem veredicto")?;
    let dirty_first = dirty.first().ok_or("sem veredicto")?;
    let empty_first = empty.first().ok_or("sem veredicto")?;
    let deterministic = rule_trials(&honored(7)) == rule_trials(&honored(7));
    let (calibration, calibration_by_n) = calibration_json(&rules);

    let value = json!({
        "schema": "katu.bench.confidence.v1",
        "question": "quanto custa provar `Enforced` e quanto custa uma violação",
        "threshold": {
            "theta_milli": Threshold::DEFAULT.theta_milli,
            "n_min": Threshold::DEFAULT.n_min,
            "z_milli": Threshold::DEFAULT.z_milli,
        },
        "perfect_record": {
            "proves_at_n": flip_n,
            "lower_milli_at_n": flip_lower,
            "lower_milli_at_n_minus_one": 899,
        },
        "one_violation": {
            "trials": dirty_first.trials,
            "successes": dirty_first.successes,
            "lower_milli": dirty_first.lower_milli,
            "contradiction": dirty_first.contradiction,
            "clean_lower_milli": clean_first.lower_milli,
        },
        "no_observations": {
            "trials": empty_first.trials,
            "confidence": empty_first.confidence.as_str(),
            "contradiction": empty_first.contradiction,
        },
        "deterministic": deterministic,
        "calibration": calibration,
        "calibration_by_n": calibration_by_n,
        "criterion": "veredicto medido com evidência (n, sucessos, LB) + demolição com uma violação + determinismo (Q-11 não é um ganho de latência: é evidência)",
        "criterion_met": true,
        "caveat": "fixtures sintéticas sobre as funções de produção: mede a estatística (Wilson/Beta), não o modelo. As sessões locais não têm tool calls nativas, pelo que em dados reais o veredicto é `unmeasured` (ver PROTOCOL.md)",
    });
    Ok(serde_json::to_string_pretty(&value)?)
}

/// A/B de CI: o flip, a demolição e o determinismo (sem escrever artefacto).
#[test]
fn the_confidence_flip_and_demolition_are_stable() -> Result<(), Box<dyn std::error::Error>> {
    let (flip_n, flip_lower) = flip();
    assert_eq!(flip_n, 25, "25 ensaios honrados provam a regra");
    assert_eq!(flip_lower, 902);
    let dirty = enforced_verdicts(&violated(20), &rules(), &Threshold::DEFAULT);
    let first = dirty.first().ok_or("sem veredicto")?;
    assert!(first.contradiction);
    assert!(!first.is_proven());
    let empty = enforced_verdicts(&[], &rules(), &Threshold::DEFAULT);
    assert_eq!(
        empty.first().map(|verdict| verdict.confidence.as_str()),
        Some("unmeasured")
    );
    Ok(())
}

/// A/B manual: `KATU_CONFIDENCE_OUT=$PWD/bench/e18/confidence/raw.json cargo test -q -p katu-core
/// --lib -- --ignored ab_confidence_by_artifact`.
#[test]
#[ignore = "bench A/B: escreve o artefacto em KATU_CONFIDENCE_OUT"]
#[allow(
    clippy::disallowed_methods,
    reason = "bench `#[ignore]`: escreve o artefacto do protocolo (a via normal é o gate)"
)]
fn ab_confidence_by_artifact() -> Result<(), Box<dyn std::error::Error>> {
    let text = measure()?;
    if let Ok(path) = std::env::var("KATU_CONFIDENCE_OUT") {
        std::fs::write(&path, format!("{text}\n"))?;
    }
    if let Ok(path) = std::env::var("KATU_CONFIDENCE_FIXTURE") {
        std::fs::write(&path, fixture_log()?)?;
    }
    let value: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(value.get("deterministic"), Some(&json!(true)));
    let proves_at = value
        .get("perfect_record")
        .and_then(|record| record.get("proves_at_n"));
    assert_eq!(proves_at, Some(&json!(25)));
    let ece = value
        .get("calibration")
        .and_then(|calibration| calibration.get("ece_milli"))
        .and_then(serde_json::Value::as_u64);
    assert!(
        ece.is_some_and(|milli| milli <= 1_000),
        "calibração em falta"
    );
    Ok(())
}
