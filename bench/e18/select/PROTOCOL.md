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

**Limites (o que este A/B não diz).**

- Não mede coerência: a política de utilidade pode manter unidades **não contíguas** e largar o meio da
  conversa (92 de 101 mensagens ficam fora, no cenário). Um humano lê isso como perda de contexto.
- Não mede custo: o greedy é `O(n²·termos)`, contra `O(n)` do sufixo. É trabalho de CPU pago a cada
  turno, e o A/B de CPU fica para o item de performance correspondente.
- Não decide o *default*: `SelectionPolicy::Suffix` mantém-se até haver A/B com o modelo.
