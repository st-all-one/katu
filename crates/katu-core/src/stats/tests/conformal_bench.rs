//! C2 · sonda de decisão do *split conformal* — **código de bench, não de produção**.
//!
//! O conformal foi medido e **rejeitado para adopção** (ver `bench/e18/conformal/PROTOCOL.md`).
//! Portanto esta fórmula vive **aqui**, dentro do `#[cfg(test)]`, e não em `katu-core/src`:
//! um item rejeitado deixa o **número**, não uma API pública sem consumidores.
//!
//! Se algum dia o `log` real fornecer uma base de calibração não correlacionada por regra, o que
//! se reavalia é este ficheiro — e ele passa a `src/` no acto, não antes.

/// Nível nominal em milésimos (`950` = 95 % de cobertura).
type Level = u32;

/// Nível nominal default (95 %), em milésimos.
const LEVEL: Level = 950;

/// Índice conformal `k = ⌈(n+1)·nível⌉`. `None` se o nível for inválido ou `k == 0`.
fn rank(n: usize, level: Level) -> Option<usize> {
    if level == 0 || level >= 1_000 {
        return None;
    }
    let scaled = u64::try_from(n)
        .ok()?
        .checked_add(1)?
        .checked_mul(u64::from(level))?
        .checked_add(999)?;
    usize::try_from(scaled / 1_000)
        .ok()
        .filter(|value| *value >= 1)
}

/// Quantil conformal: o `k`-ésimo menor score. `None` quando `k > n` (**fail-closed**).
fn quantile(scores: &[u64], level: Level) -> Option<u64> {
    let k = rank(scores.len(), level)?;
    if k > scores.len() {
        return None;
    }
    let mut sorted = scores.to_vec();
    sorted.sort_unstable();
    sorted.get(k.saturating_sub(1)).copied()
}

/// Cobertura empírica em milésimos (1000 = tudo coberto).
fn coverage_milli(hits: &[bool]) -> u64 {
    if hits.is_empty() {
        return 0;
    }
    let hit = u64::try_from(hits.iter().filter(|hit| **hit).count()).unwrap_or(0);
    let total = u64::try_from(hits.len()).unwrap_or(1);
    hit.saturating_mul(1_000).checked_div(total).unwrap_or(0)
}

/// Série sintética determinística (SplitMix64): uniforme em `0..1000`, sem dependências.
fn exchangeable(seed: u64, count: u64) -> Vec<u64> {
    let mut state = seed;
    (0..count)
        .map(|_| {
            state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^= z >> 31;
            z % 1_000
        })
        .collect()
}

/// Resíduo de um forecast ingénuo (média dos `WINDOW` anteriores): tem de ser **previsto**, não
/// nulo por construção — e sai correlacionado, que é precisamente o que o conformal não tolera.
fn residuals(series: &[u64], window: usize) -> Vec<u64> {
    let mut out = Vec::new();
    for (index, observed) in series.iter().enumerate().skip(window) {
        let base = index.saturating_sub(window);
        let Some(past) = series.get(base..index) else {
            continue;
        };
        let sum: u64 = past.iter().copied().fold(0, u64::saturating_add);
        let mean = sum
            .checked_div(u64::try_from(window).unwrap_or(1))
            .unwrap_or(0);
        out.push(observed.abs_diff(mean));
    }
    out
}

/// Cobertura de um regime: calibração e teste.
fn measure(calibration: &[u64], test: &[u64], level: Level, window: usize) -> Option<(u64, u64)> {
    let q = quantile(&residuals(calibration, window), level)?;
    let hits: Vec<bool> = residuals(test, window)
        .iter()
        .map(|residual| *residual <= q)
        .collect();
    Some((coverage_milli(&hits), q))
}

/// A/B do C2: a cobertura é a garantia **ou** o pressuposto? Mede os dois regimes.
///
/// - **trocável**: uma única série partida em calibração + teste (é o que a troca exige);
/// - **não-trocável**: calibração e teste de séries distintas, mesma distribuição marginal.
#[test]
#[ignore = "bench C2: escreve a cobertura empírica em KATU_CONFORMAL_OUT"]
#[allow(
    clippy::disallowed_methods,
    clippy::too_many_lines,
    reason = "bench `#[ignore]`: a varredura é o artefacto; parti-la esconderia a conta"
)]
fn ab_conformal_coverage_by_artifact() -> Result<(), Box<dyn std::error::Error>> {
    const WINDOW: usize = 10;
    const TEST_N: u64 = 2_000;
    let mut rows = Vec::new();
    let mut met = 0_usize;
    let mut published = 0_usize;
    let mut worst_exchangeable = 0_u64;
    let mut worst_other = 0_u64;
    for level in [800_u32, LEVEL, 990] {
        for calibration_n in [19_u64, 49, 99, 199] {
            // Regime trocável: uma série, partida em dois blocos.
            let series = exchangeable(0x5eed, calibration_n + TEST_N);
            let (calibration, test) = series.split_at(usize::try_from(calibration_n).unwrap_or(0));
            match measure(calibration, test, level, WINDOW) {
                Some((covered, q)) => {
                    let ok = covered >= u64::from(level);
                    met += usize::from(ok);
                    published = published.saturating_add(1);
                    worst_exchangeable =
                        worst_exchangeable.max(u64::from(level).saturating_sub(covered));
                    rows.push(serde_json::json!({
                        "regime": "trocavel",
                        "level_milli": level,
                        "calibration_n": calibration_n,
                        "radius_milli": q,
                        "empirical_coverage_milli": covered,
                        "criterion_met": ok,
                    }));
                }
                None => rows.push(serde_json::json!({
                    "regime": "trocavel",
                    "level_milli": level,
                    "calibration_n": calibration_n,
                    "published": false,
                    "reason": "k > n: sem intervalo publicavel (fail-closed)",
                })),
            }
            // Regime não-trocável: séries distintas, mesma marginal.
            let other = measure(
                &exchangeable(0x5eed, calibration_n),
                &exchangeable(0x00c0_ffee, TEST_N),
                level,
                WINDOW,
            );
            if let Some((covered, q)) = other {
                worst_other = worst_other.max(u64::from(level).saturating_sub(covered));
                rows.push(serde_json::json!({
                    "regime": "nao_trocavel",
                    "level_milli": level,
                    "calibration_n": calibration_n,
                    "radius_milli": q,
                    "empirical_coverage_milli": covered,
                    "criterion_met": covered >= u64::from(level),
                }));
            }
        }
    }
    let value = serde_json::json!({
        "schema": "katu.bench.conformal.v1",
        "question": "a cobertura empirica do split conformal atinge o nivel nominal, e o que a faz falhar",
        "rule": "k = ceil((n+1) * nivel); q = k-esimo menor residual de calibracao; cobre se |erro| <= q",
        "forecast": "media dos 10 anteriores (o residual tem de ser previsto, nao nulo por construcao)",
        "test_sample": TEST_N,
        "rows": rows,
        "exchangeable_rows": published,
        "rows_met_in_exchangeable_regime": met,
        "criterion": "no regime TROCÁVEL, cobertura empirica >= nivel nominal em todas as linhas publicadas",
        "criterion_met": met == published,
        "worst_slip_when_exchangeable_milli": worst_exchangeable,
        "worst_slip_when_not_exchangeable_milli": worst_other,
        "caveat": "amostras SINTETICAS (SplitMix64): mede a estatistica do conformal e o peso do pressuposto de troca, nao o modelo nem o log real",
        "decision": "REJEITADO para adocao: com residuos correlacionados (forecast rolante) a cobertura mergulha abaixo do nominal, mesmo no regime trocavel; o conformal so entra quando o log der uma base de calibracao NAO correlacionada (n >= 19 por regra, hoje 0). A formula mora neste bench, nao em src/: item rejeitado deixa o numero, nao uma API publica sem consumidores",
    });
    let text = serde_json::to_string_pretty(&value)?;
    if let Ok(path) = std::env::var("KATU_CONFORMAL_OUT") {
        std::fs::write(&path, format!("{text}\n"))?;
    }
    let parsed: serde_json::Value = serde_json::from_str(&text)?;
    // O artefacto é uma medição, não uma aprovação: o critério é o que é, e a decisão escreve-se.
    assert!(parsed.get("criterion").is_some(), "critério ausente");
    assert!(
        parsed
            .get("exchangeable_rows")
            .and_then(serde_json::Value::as_u64)
            >= Some(6),
        "a varredura trocável correu a menos de 6 linhas"
    );
    Ok(())
}
