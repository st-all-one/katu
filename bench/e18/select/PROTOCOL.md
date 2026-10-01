# A/B — seleção de contexto por informação (Q-02b/Q-03)

**Pergunta.** Com o **mesmo** orçamento de tokens, a política de utilidade (utilidade submodular +
diversidade MMR + fusão RRF) retém mais informação do que a política histórica (sufixo cru / truncagem
cronológica do digest)?

**Fórmula.** `I_ret(S) = Σ_{t ∈ S} idf(t)`, com `idf(t) = ln(1 + n/df(t))` calculado **do próprio
conjunto** (nenhum corpus externo, nenhum tokenizer); a eficiência é `I_ret / token`. O ganho é
`(eficiência_utilidade − eficiência_sufixo) / eficiência_sufixo`.

**Base.** `inferred` — é um **proxy** de informação, não sucesso de tarefa. O critério do plano (≥ 20 %)
é aplicado ao proxy; a adoção do *default* exige o A/B de sucesso de tarefa com o modelo, que **ainda
não existe** (por isso `behavior.context_selection` continua a valer `suffix`).

**Cenário** (`crates/katu-core/src/context/tests/bench.rs`). Log sintético determinístico, 101
mensagens / 89 unidades: 40 turnos de enchimento repetitivo (`katu passo N katu repetido`), quatro
marcos informativos (termos que só aparecem uma vez) espalhados — incluindo o **fim** do prefixo, que é
exatamente o que a truncagem histórica corta — e um lote de tool a cada 10 turnos (unidades de corrida).
`raw_min = 900` (cabe ~40 % do histórico), `summary_max = 220`.

**Controlo negativo.** O mesmo A/B sobre um log **uniforme** (todas as mensagens trazem termos
próprios, redundância nula): o ganho é **0,0 %**, o que mostra que a métrica não é trivialmente
positiva.

**Como correr.**

```sh
KATU_SELECT_OUT=$PWD/bench/e18/select/raw.json \
  cargo test -p katu-core --lib -- --ignored --nocapture ab_context_selection_by_information
```

O invariante `U(utilidade) ≥ U(sufixo)` e o orçamento exato são asserções de **CI** (sem `#[ignore]`),
pelo que uma regressão aparece no `make check` sem correr o gate manual.

**Resultado** (`raw.json`, 2026-02): seleção **+1037,9 %** de eficiência (864 → 118 tokens) e digest
**+338,0 %** de informação retida (219 → 50 tokens). Ambos muito acima dos 20 % — o que é *esperado*
num cenário com enchimento repetitivo; o número que importa para a adoção é o de uma conversa real, e
esse exige o modelo.

## A1/A2 (W10) — formalizar o proxy: distorção `D ≤ D0` e diversidade medida

O A/B de Q-02b mede **eficiência** (`I_ret` por token). A1 formaliza-o como uma curva
taxa–distorção, e A2 mede a diversidade com o mesmo rigor — o que falta ao primeiro é o
**contrato**, não o número.

**Fórmula (A1).** `U` = histórico inteiro, `S` = selecionado:

```
R(S) = tokens(S) / tokens(U)          taxa
D(S) = 1 − I_ret(S) / I_ret(U)        distorção (fração de informação perdida)
D0    = D(sufixo) na mesma taxa       contrato: D(utilidade) ≤ D0
```

`D0` não é um número mágico: é a distorção da política **histórica** no mesmo orçamento. A
afirmação deixa de ser "ganho > 20 %" (um proxy) e passa a ser "à mesma taxa, não distorce mais
que o histórico" (um contrato, com base zero).

**Resultado** (`raw.json` → `a1_rate_distortion`):

| política | taxa `R` | distorção `D` |
|---|---|---|
| sufixo (histórica) | 804 ‰ | **356 ‰** = `D0` |
| utilidade | 109 ‰ | **0 ‰** |

No cenário sintético a seleção **não perde informação nenhuma** (`I_ret(S) = I_ret(U)`: as unidades
escolhidas cobrem todos os termos distintos do histórico) a uma taxa 7× menor. Contrato cumprido.

**Fórmula (A2).** Diversidade = Jaccard intra-conjunto das unidades escolhidas (média e máximo), com
o teto `sim_max` já versionado nos parâmetros. Resultado (`a2_diversity`):

| política | média | máximo | `sim_max` |
|---|---|---|---|
| sufixo | 624 ‰ | **1000 ‰** (viola o teto) | 700 ‰ |
| utilidade (MMR) | **17 ‰** | **500 ‰** | 700 ‰ |

O MMR em produção satisfaz o critério de aceitação de A2 ("nenhum par acima de `sim_max`"); o
histórico não.

**DPP (A2, parte determinantal): rejeitado com o número.** O DPP maximiza `log det`; à primeira
ordem é penalizar redundância. Mediu-se o quanto há a ganhar (`a2_dpp_decision`): existem **81
trocas** de uma unidade que aumentam a informação dentro do orçamento, com o melhor ganho em
**12 584 µ** de massa. Mas essas trocas são de **utilidade**, não de redundância — a similaridade
média já é 17 ‰, ou seja, não há par redundante para o determinante penalizar. O que fecharia a
lacuna é uma **busca local** do greedy (troca 1‑para‑1), que é outra frente, não um DPP.
*Condição para rever:* com log real, se a similaridade média passar de `sim_max/2`, o determinante
volta a ter trabalho.

**Como correr.** O mesmo comando do A/B acima regrava `raw.json`; os invariantes de CI (sem
`#[ignore]`) são `the_utility_policy_never_distorts_more_than_the_historical_one` (A1) e
`no_chosen_pair_exceeds_the_similarity_ceiling` (A2).

**Limites (o que este A/B não diz).**

- Não mede coerência: a política de utilidade pode manter unidades **não contíguas** e largar o meio da
  conversa (92 de 101 mensagens ficam fora, no cenário). Um humano lê isso como perda de contexto.
- Não mede custo: o greedy é `O(n²·termos)`, contra `O(n)` do sufixo. É trabalho de CPU pago a cada
  turno, e o A/B de CPU fica para o item de performance correspondente.
- Não decide o *default*: `SelectionPolicy::Suffix` mantém-se até haver A/B com o modelo.
