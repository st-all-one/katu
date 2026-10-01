# C2 · Predição conformal — protocolo e resultado

## Pergunta

O IC de Wilson (Q-11) e o *bootstrap* (C7) dizem onde está o parâmetro com `n` observações. O
conformal responde a outra pergunta — *dado um caso novo, o intervalo cobre?* — e promete
**cobertura** `P(Y ∈ C(X)) ≥ 1 − α` **sem** assumir a forma da distribuição, desde que as amostras
sejam **trocáveis** (*exchangeability*). Cumpre-se?

## Fórmula (split conformal)

```
calibração: s₁..sₙ = resíduo |erro| de n observações independentes
k          = ⌈(n+1)·nível⌉                      (nível em milésimos; 950 = 95 %)
q          = s₍ₖ₎, o k-ésimo menor
predição:  C(X) = [Ŷ(X) − q, Ŷ(X) + q]
```

**Fail-closed:** quando `k > n` não há intervalo publicável — `None`, nunca um intervalo estreito e
enganador (`too_little_calibration_publishes_nothing`).

## Cenário (determinístico, sem RNG externo)

Série sintética SplitMix64 (uniforme em `0..1000`), forecast ingénuo = média dos 10 anteriores, e
resíduo = erro absoluto. O forecast não é informacional de propósito: o resíduo tem de ser
**previsto**, não nulo por construção.

Dois regimes, porque é a pergunta do item:

- **trocável** — uma série partida em calibração + teste (é o que a troca exige);
- **não-trocável** — calibração e teste de séries distintas, mesma distribuição marginal.

Varredura: níveis `{800, 950, 990} ‰ × n_cal ∈ {19, 49, 99, 199}`, 2 000 observações de teste.

## Resultado (`raw.json`)

Regime **trocável** (a garantia que a troca compra):

| nível | `n_cal` | raio `q` | cobertura | ≥ nominal |
|---|---|---|---|---|
| 800 ‰ | 19 | 480 | 910 ‰ | ✔ |
| 800 ‰ | 49 | 434 | 837 ‰ | ✔ |
| 800 ‰ | 99 | 449 | 872 ‰ | ✔ |
| 800 ‰ | 199 | 439 | 852 ‰ | ✔ |
| 950 ‰ | 19 | — | — | *não publicado* (`k > n`) |
| 950 ‰ | 49 | 489 | **913 ‰** | ✘ (−37 ‰) |
| 950 ‰ | 99 | 523 | **946 ‰** | ✘ (−4 ‰) |
| 950 ‰ | 199 | 523 | **947 ‰** | ✘ (−3 ‰) |
| 990 ‰ | 19/49/99 | — | — | *não publicado* (`k > n`) |
| 990 ‰ | 199 | 643 | 995 ‰ | ✔ |

**5 de 8 linhas publicadas cumprem.** Pior queda: **37 ‰**.

Regime **não-trocável** (séries distintas, mesma marginal), para isolar o papel da troca:

| nível | `n_cal` | cobertura | diferença vs. trocável |
|---|---|---|---|
| 800 ‰ | 19 | 898 ‰ | −12 ‰ |
| 950 ‰ | 49 | 908 ‰ | −5 ‰ |
| 950 ‰ | 99 | 947 ‰ | +1 ‰ |
| 990 ‰ | 199 | 995 ‰ | 0 ‰ |

**A diferença entre os dois regimes é ≤ 12 ‰** — muito menor do que a queda dentro do próprio
regime trocável. Ou seja: **a troca não é o que falha aqui**; a independência dos resíduos é. As
mesmas linhas que falham no regime trocável falham no não-trocável, e a falha não desaparece quando
a troca se verifica.

## Decisão (escrita)

**Rejeitado para adoção, com o número.** O conformal está medido
([`conformal_bench.rs`](../../../crates/katu-core/src/stats/tests/conformal_bench.rs)) e a fórmula
**fica no bench, não em `src/`**: um item rejeitado deixa o número, não uma API pública sem
consumidores.

Duas razões medidas:

1. **Cobertura insuficiente mesmo quando a troca se verifica.** Com resíduos serialmente
   correlacionados (um pico do forecast rolante infla as 10 previsões seguintes), a cobertura a
   95 % fica em 913–947 ‰ contra as 950 ‰ prometidas. A garantia é marginal e exige *scores*
   trocáveis **e** quase independentes; aqui não são.
2. **Não há base.** O `log` real tem **0 ensaios** medidos por regra
   ([`bench/e18/confidence`](../confidence/PROTOCOL.md)); `n_cal ≥ 19` por regra é a condição
   mínima, e nenhuma a tem. As 3 linhas não publicadas mostram que o *fail-closed* já funciona
   (`k > n` ⇒ sem intervalo).

**Condição para rever.** (a) base de calibração real, por regra, com resíduos **não** correlacionados
(calibration set e test set de sessões distintas, não de turnos consecutivos); (b) repetir esta
varredura com essa base e exigir cobertura ≥ nominal em **todas** as linhas publicadas, antes de o
módulo passar a `src/`.

## Como correr

```sh
KATU_CONFORMAL_OUT=$PWD/bench/e18/conformal/raw.json \
  cargo test -q -p katu-core --lib -- --ignored ab_conformal_coverage_by_artifact
```

## Limites

- Amostras **sintéticas**: mede a estatística do conformal e o peso dos pressupostos, não o modelo
  nem o `log` real.
- O forecast rolante de 10 é uma escolha do cenário; um forecast melhor reduz a correlação e
  provavelmente fecha a lacuna. Isso é justamente o que a condição (a) exige medir.
- A queda é um efeito conhecido (scores correlacionados invalidam a troca dos quantis); o
  artefacto **declara-o** em vez de o esconder.
- `quantile` é uma ordenação simples: irrelevante para `n` da ordem das dezenas, que é a escala real.
