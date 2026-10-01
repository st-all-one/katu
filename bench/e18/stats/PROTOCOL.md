# E18 · C7 — Estatística robusta (mediana/MAD)

## Contexto

O resumo estatístico (W7) usa a **média** para o IC 95 %. A média é sensível a outliers:
um valor extremo (ex.: um arranque a frio) distorce o intervalo e pode fazer o gate
falhar sem que o desempenho real tenha piorado. O **MAD** (Desvio Absoluto Mediano) é
uma medida de dispersão **robusta**: não é afetada por valores extremos.

## Adoptado

`Summary` ganhou:
- `mad` — Desvio Absoluto Mediano: `mediana(|x_i − mediana|)`.
- `robust_ci95_low` / `robust_ci95_high` — IC 95 % robusto: `mediana ± 1,96·MAD/√n`.

O IC robusto é uma **alternativa** ao IC da média, não um substituto. Quando os dois
discordam, a diferença mede o efeito de valores extremos.

## Critérios

| # | Critério | Como medir |
|---|----------|------------|
| 1 | O MAD é resistente a outliers | `mad(with_outlier) ≤ 10` para uma amostra 1..20 + outlier 1M |
| 2 | A média não é resistente | `mean(with_outlier) > 10 × mean(clean)` |
| 3 | O IC robusto contém a mediana | `robust_ci95_low ≤ p50 ≤ robust_ci95_high` |
| 4 | O IC robusto é estável sob outliers | `|robust_high(with_outlier) − robust_high(clean)| < robust_high(clean)/2` |

## Artefacto

`raw.json` — 4 critérios, 4 cumpridos.

## Reversão

Se o MAD não provar a sua utilidade (ex.: o IC robusto nunca divergir do IC da média
na prática), remover os campos `mad`, `robust_ci95_low`, `robust_ci95_high` de `Summary`.
