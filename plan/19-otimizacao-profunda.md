# E18 — Otimização profunda (matemática, informação e estatística)

> **Transversal — não é fase.** Este épico não adiciona superfície: adiciona **formalismo** onde o
> katu decide sozinho (contexto, transporte, estado, confiança, anomalia, planeamento). O motor de
> memória continua a ser o `knudge`: aqui **consome-se**, não se reimplementa (DF6, G4).
>
> **Decisões:** DF2, DF5, DF8. **Depende de:** E05 (núcleo); idealmente E09 (contexto) e E12
> (providers).
> **Gate do épico:** nenhuma frente sem **fórmula + artefacto + base de evidência tipada (DF5) +
> teste que a trava**; cada frente é **adotada ou revertida** por medição (§0.3); toda a aritmética
> é **determinística** (§0.2).
>
> **Fontes.** Modelos de referência: [`knudge/wiki/specs/matematica.md`](../knudge/wiki/specs/matematica.md)
> (só a *forma* — RRF, Beta/Wilson, FSRS, KL/JS, PageRank/PPR, Louvain, MinHash, PERT). Método:
> [`knudge/plan/implementation/19_performance_reforma_cli.md`](../knudge/plan/implementation/19_performance_reforma_cli.md)
> (medir → A/B → adotar-ou-reverter, bytes idênticos, rejeições registadas). Engenharia:
> [`../.agents/skill/rust/SKILL.md`](../.agents/skill/rust/SKILL.md).

---

## 0. Fronteira, determinismo e método

### 0.1 A fronteira (o katu consome, não reimplementa)

| Modelo | Dono | Papel no katu |
|---|---|---|
| BM25/IDF, dedup, retenção FSRS das **notas**, drift de âncoras | **knudge** | consumido pela porta `Memory`; nunca recalculado |
| PageRank/PPR, Louvain, MinHash/LSH, RRF | **knudge** | o katu **reaproveita** RRF/PPR para *fundir canais de contexto* (F9), com o grafo/índice do knudge |
| Contexto, transporte, estado, confiança, anomalia, planeamento | **katu** | é o escopo deste épico (F2–F9) |

Reimplementar o motor de retrieval no katu é **não-objetivo** (cicatriz dos dois motores, §20).

### 0.2 Contrato de determinismo numérico

Adotado de `matematica.md` §12 e imposto a **todo** o cálculo do katu:

1. **Sem RNG.** Onde a aleatoriedade ajudar, usa-se **hash determinístico** (`fnv1a`/`splitmix64`)
   ou semente fixa.
2. **Ordem canônica** em toda soma acumulada (`BTreeMap`/`BTreeSet`/ordenação total); `HashMap`
   **não** é iterado para produzir saída.
3. **`total_cmp`** (ordem total IEEE-754), `clamp01`, `NaN → 0`.
4. O resultado é **função pura** da entrada: mesma sessão/log → mesmos números.

### 0.3 Método (a herança do exemplo 19)

- **Medir antes**: harness com fixture determinística e baseline **commitado** (E15-T01).
- **A/B com artefacto cru**: micro (`criterion`) **e** e2e; o ganho pode existir só numa escala.
- **Adotar-ou-reverter**: ≥ 20 % no alvo **ou** remoção de complexidade; senão **reverter**. Cada
  rejeição fica escrita com o número.
- **Nada muda bytes** observáveis: `Model-visible ⟺ logged` (E04) é o equivalente dos "goldens
  byte-idênticos".
- **Corta-primeiro (`✂`)**: frentes assim marcadas saem antes das outras se o orçamento apertar.

---

## 1. Mapa das frentes

| # | Frente | Formalismo | Anfitrião | Corta? |
|---|---|---|---|---|
| F1 | Determinismo numérico | contrato (§0.2) | E02/E09 | não |
| F2 | Contexto = mochila submodular | otimização submodular + MMR/DPP | E09-T01 | não |
| F3 | Compactação guiada por informação | entropia/surprisal + JS | E09-T07 | não |
| F4 | Transporte por latência | teoria de filas + *hedging* | E12-T06/T07 | não |
| F5 | Estado persistente e replay | partilha estrutural | E04-T03/E09-T02 | não |
| F6 | Confiança por artefacto | Beta-Bernoulli + Wilson | E02-T04 | não |
| F7 | Anomalia/drift | CUSUM / SPRT | E04-T07 | **✂** |
| F8 | Escalonar o DAG do plano | PERT/CPM | E06 | **✂** |
| F9 | Fundir canais (RRF/PPR) | RRF/PPR (consumo) | E09-T01 | não |

---

## 2. F2 — Contexto como problema de otimização

**Problema.** Montar o contexto (E09-T01) é escolher um subconjunto de candidatos (hits do knudge,
ficheiros, mensagens, diffs) que caiba no orçamento `T` de tokens e maximize a utilidade.

**Modelo.** Candidato `i` com relevância `rel(i)` (score do knudge, recência, âncora), custo
`cost(i)` (tokens) e similaridade `sim(i,j)`. Reformula-se como **maximização submodular com
penalização de redundância** (surrogado determinístico de MMR/DPP):

```
U(S) = Σ_{i∈S} rel(i) − λ · Σ_{i<j∈S} sim(i,j)
sujeito a  Σ_{i∈S} cost(i) ≤ T
```

Greedy determinístico — em cada passo escolhe

```
i* = argmax_i [ rel(i) − λ · max_{j∈S} sim(i,j) ],   respeitando o custo
```

com desempate `(marginal desc, id asc)`.

- `cost(i)`: **tokenizer exato** do dialeto (não estimativa) + overhead fixo.
- `sim(i,j)`: Jaccard de termos ou MinHash estimado (determinístico), ou cosseno de embedding — a
  **mesma** escolha em todos os turnos.
- `λ` é **dado** (política), versionado; não é constante mágica.
- Greedy atinge `(1−1/e)` do ótimo para submodulares monótonas — o suficiente e, sobretudo,
  **determinístico e testável**.

**Teste.** Mesmo input → mesmo contexto (proptest); `U(greedy) ≥ U(baseline)`; orçamento exato e
`+1`; nenhum par acima de `sim_max` entra.

---

## 3. F3 — Compactação guiada por informação

**Problema.** Decidir **quando** compactar e **o que** manter, sem perder o que importa.

**Modelo.** Sobre a janela recente `P` e a distribuição latente `Q` de termos (`content_terms`,
fold + stem):

```
H(P)      = −Σ_t P(t)·log2 P(t)                       (bits/token)
KL(P‖Q)   = Σ_t P(t)·log2( P(t)/Q(t) )                (com suavização 1e-9)
JS(P‖Q)   = ½·KL(P‖M) + ½·KL(Q‖M),  M = (P+Q)/2       ∈ [0,1] (bits, base 2)
```

Gatilho quando `JS(P_recent‖P_latent) ≥ τ_JS` **ou** a taxa de entropia excede o orçamento de
informação da fase. O que **fica** é o de maior **surprisal** `−log2 P(t)` (maior contribuição de
informação); o resto é resumido. Mede-se a informação retida e a densidade:

```
I_ret = Σ surprisal(span mantido)          razão = I_ret / tokens
```

- Preserva o **prefixo** do provider: mapeamento `original→substituto` determinístico (E09-T07).
- `τ_JS` e o orçamento de informação são **dados** (política), versionados.
- Recuperação obrigatória: o original continua endereçável no log.

**Teste.** Nenhuma mensagem irreconstruível (`Model-visible ⟺ logged`); mesma entrada → mesmo
resumo; `I_ret/token` com base `provider_reported`/`inferred`; desligar a porta mantém o
comportamento original.

---

## 4. F4 — Transporte por latência

**Problema.** O caminho built-in (DF8) é latência-crítico: TTFT e **cauda** dominam a sensação de
velocidade.

**Modelo.** Teoria de filas: `L = λW` (Little); utilização `ρ = λ/μ`; dimensão de pool `c` e
limiar de *backpressure* derivados de um `ρ` alvo. Para a cauda, **hedged requests**
(Tail-at-Scale): se não houver primeiro byte até `p_hedge` (ex.: p95 do TTFT), dispara-se o mesmo
pedido ao segundo endpoint; o custo extra é limitado pela **razão de hedge** `h`.

- **Stream:** buffer adaptativo ao intervalo p95 entre chunks; flush incremental (SSE).
- **Prefix-cache:** prefixo imutável e canônico; mede-se a **taxa de acerto** e a redução de TTFT.
- **Backpressure:** canal *bounded*; acima do limiar, o prefetch para.
- Métricas com base tipada: TTFT p50/p95/p99, tokens/s, `h`, cache-hit.

**Teste.** TTFT p95 com artefacto (E12-T07); hedge só acima de `p_hedge`; firewall LLM-free
intacta; zero bytes observáveis alterados.

---

## 5. F5 — Estado persistente e replay

**Problema.** Replay/undo/checkpoint do kernel (E04) não podem custar O(história) por passo.

**Modelo.** **Partilha estrutural** (estruturas persistentes) para o `State`: um snapshot é uma
raiz imutável; `checkpoint = snapshot + Δ`; o log compacta em **snapshot no limite de fase +
deltas**. Custo de replay `O(deltas desde o snapshot)`; igualdade por **hash canônico** do estado.

**Teste.** Snapshot O(1) medido; `state_of(replay(events)) == state_at_end` (E04-T04); replay de
deltas byte-a-byte; memória limitada pelos snapshots retidos.

---

## 6. F6 — Confiança por artefacto (Beta-Bernoulli + Wilson)

**Problema.** Que artefactos do katu merecem `Enforced` (regra) ou confiança (tool/provider)?

**Modelo.** Por artefacto `a` (regra, tool, provider, dialeto), ensaios de Bernoulli `s` sucessos /
`f` falhas (fracionárias admitidas; `partial = 0.5`). Posterior `Beta(1+s, 1+f)`:

```
μ(a)  = (1 + s) / (2 + s + f)
LB(a) = limite inferior de Wilson (z = 1.96), clampado a [0,1]
```

Um artefacto só é `Enforced`/confiável se `LB(a) ≥ θ` **e** `n ≥ n_min`; abaixo disso demove para
`Advisory` com evidência registada (DF3, DF5). **Providers:** as estatísticas **sinalizam**
degradação ao utilizador — **não** auto-escalam modelo (DF8).

**Teste.** Regra só `Enforced` com `n` e limiar; teste de demolição quando `LB` cai; nenhum
auto-scale.

---

## 7. F7 — Anomalia e drift (CUSUM / SPRT) ✂

**Problema.** Detetar loop patológico, drift de violações ou degradação do provider **cedo** e com
poucas amostras.

**Modelo.** **CUSUM** para a média:

```
S_k = max(0, S_{k-1} + (x_k − μ0 − k_slack/2))     alarme se S_k ≥ h
```

**SPRT** para binário (sucesso/fracasso) entre `H0`/`H1`, com fronteiras de Wald
`A = (1−β)/α`, `B = β/(1−α)` — nº esperado de amostras mínimo para dado `α`/`β`. Complemento:
**novidade** = entropia da assinatura das últimas `k` tool calls; novidade baixa → suspeita de
loop.

Ação: **cortar** (kill switch, E09-T06) ou **escalar `NeedsHuman`** — nunca silencioso.

**Teste.** Taxa de falso-positivo **medida**; alarme antes do teto global; mesma sequência → mesma
decisão.

---

## 8. F8 — Escalonar o DAG do plano (PERT/CPM) ✂

**Problema.** Priorizar/prefetch do que a IA vai precisar, a partir do `Plan` (E06).

**Modelo.** **PERT/CPM** sobre o DAG de dependências:

```
L(id) = dur(id) + max_{t∈deps(id)} L(t)        (ciclo ⇒ 0)
caminho crítico = argmax L
desempate: maior total; depois caminho mais longo; depois menor lexicográfico
```

`dur` = estimativa de custo/lead-time (base tipada). O caminho crítico ordena as tool calls; nós
independentes correm em paralelo **dentro** do orçamento de concorrência; entradas do caminho
crítico podem ser **prefetched** (especulativo, cancelável, **sem efeito** antes do veredicto).

**Teste.** Caminho crítico estável; prefetch cancelável e sem efeito; paralelismo respeita o teto.

---

## 9. F9 — Fundir canais de contexto (RRF / PPR)

**Problema.** Fundir canais heterogéneos (hits do knudge, git diff, ficheiros recentes, âncoras)
sem criar um motor novo.

**Modelo.** **RRF** (o mesmo do knudge, agora como *consumo*):

```
RRF(x) = Σ_c  w_c / (k + r_c(x) + 1),    k = 60,  w_c dados
```

e **PPR semeado pelo working set** sobre o grafo do knudge (via porta `Memory`) para *rankear*
ficheiros/notas; `w_ppr = 0` por default. Desempate `(score desc, id asc)`; canal ausente **não**
quebra a soma.

**Teste.** Fusão determinística; canal ausente não parte a soma; pesos são **dados** (política).

---

## Tarefas

### E18-T01 ☐ Contrato de determinismo numérico (F1)
- **Entregáveis:** utilitários `clamp01`/`total_cmp`/hash determinístico; validador e lints;
  proptests de reprodutibilidade.
- **Aceite:** mesma entrada → mesma saída; `HashMap` não iterado para saída (lint); `NaN → 0`;
  `total_cmp` onde há empate.

### E18-T02 ☐ Contexto como escolha submodular (F2)
- **Entregáveis:** `assemble(state, budget)` com utilidade submodular + MMR determinístico;
  tokenizer exato; `λ`/`sim_max` como dados; tie-break `(marginal desc, id asc)`.
- **Aceite:** `U(greedy) ≥ U(baseline)`; orçamento exato/+1; nenhum par acima de `sim_max`;
  proptest de determinismo.

### E18-T03 ☐ Compactação guiada por informação (F3)
- **Entregáveis:** entropia/surprisal + JS sobre `content_terms`; gatilho por `τ_JS`/orçamento;
  seleção por surprisal; `I_ret/token` medido; mapeamento determinístico (E09-T07).
- **Aceite:** original reconstruível; mesmo resumo para o mesmo input; desligar = comportamento
  original.

### E18-T04 ☐ Transporte por latência (F4)
- **Entregáveis:** pool/backpressure por `ρ`; hedged requests acima de `p_hedge`; buffer de stream
  adaptativo; prefix-cache e taxa de acerto; métricas com base.
- **Aceite:** TTFT p95 com artefacto; hedge só acima de `p_hedge`; firewall intacta.

### E18-T05 ☐ Estado persistente e checkpoint (F5)
- **Entregáveis:** `State` com partilha estrutural; `checkpoint = snapshot + Δ`; compactação de
  log; hash canônico de estado.
- **Aceite:** snapshot O(1) medido; `state_of(replay) == state_at_end`; replay de deltas
  byte-a-byte.

### E18-T06 ☐ Confiança por artefacto (F6)
- **Entregáveis:** acumuladores Beta-Bernoulli + `LB` de Wilson por regra/tool/provider; limiar
  `θ`/`n_min` como dados; ligação a `Enforced`/`Advisory` (DF3).
- **Aceite:** `Enforced` só com `n` e limiar; demolição quando `LB` cai; nenhum auto-scale de
  modelo.

### E18-T07 ☐ Anomalia/drift (F7) ✂
- **Entregáveis:** CUSUM (média) + SPRT (binário) + novidade por assinatura de tool calls; ação
  corta/escala.
- **Aceite:** falso-positivo medido; alarme antes do teto global; determinístico.

### E18-T08 ☐ PERT/CPM sobre o DAG do plano (F8) ✂
- **Entregáveis:** `L(id)` memoizado; caminho crítico com desempates determinísticos; prefetch
  especulativo cancelável; paralelismo dentro do teto.
- **Aceite:** caminho crítico estável; prefetch sem efeito antes do veredicto.

### E18-T09 ☐ Fusão de canais RRF/PPR (F9)
- **Entregáveis:** fusão RRF (pesos como dados, `k = 60`); PPR semeado pelo working set via
  `Memory`; degradação por canal ausente.
- **Nota (OA17):** se a fusão precisar de "primeiro canal decisivo vence", usar um despacho `bail`
  no event bus — **só** quando este for consumidor real; não antecipar.
- **Aceite:** determinístico; canal ausente não quebra; pesos versionados.

### E18-T10 ☐ Harness estatístico e gate (método, §0.3)
- **Entregáveis:** `criterion` (micro) + `hyperfine` (startup) + `dhat` (alocação); ≥ 3 repetições
  e IC 95 %; gate de regressão no CI; artefacto cru commitado; negativos/no-op visíveis.
- **Extende:** E15-T01/T02, E09-T05. **Aceite:** regressão acima do limiar falha o job; todo número
  publicado tem artefacto e base (DF5).

---

## Definition of Done

- [ ] F1–F9 com fórmula, artefacto e teste que a trava; `✂` respeitado.
- [ ] `criterion`/`hyperfine`/`dhat` no runbook; baseline e IC 95 % commitados.
- [ ] Toda decisão de otimização é **adotar-ou-reverter**, com rejeições escritas.
- [ ] Zero alteração de bytes observáveis; `Model-visible ⟺ logged` intacto.
- [ ] `cargo xtask check` e job `msrv` verdes em Rust 1.97.0.

## Não-objetivos

- Reimplementar o motor de retrieval do knudge (BM25/RRF/PPR/Louvain/MinHash/FSRS das notas).
- Otimizar sem medir; publicação de números sem artefacto (DF5).
- `HashMap`/`HashSet` iterados para saída; RNG em qualquer caminho.
- Auto-escalonamento de modelo/pensamento pelo agente (DF8): as estatísticas só **sinalizam**.
- `rayon`/alocador/SIMD como padrão por reflexo (só por A/B, gated).
- Transformar compactação em hot path (é porta, off hot path — E09-T07).

## Riscos

| Risco | Mitigação |
|---|---|
| Formalismo vira ornamentação | gate do épico: fórmula + artefacto + teste + adotar-ou-reverter |
| Greedy submodular degrada qualidade | A/B vs baseline; `λ` como dado; reverter sem hesitar |
| JS/entropia com janela curta é ruído | `n_min` por tópico; medir falso-positivo (como `MIN_DRIFT_NOTES`) |
| Hedging aumenta custo | razão de hedge limitada; disparar só acima de `p_hedge`; contabilizar |
| Estatística com poucas amostras mente | Wilson (conservador) + `n_min`; SPRT com `α`/`β` declarados |
| Estado persistente infla memória | limitar snapshots retidos; medir com `dhat` |
| Duplicar o knudge | fronteira §0.1; `xtask check-layers` + revisão de superfície (E14) |
