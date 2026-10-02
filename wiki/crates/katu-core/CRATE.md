# `katu-core` — Kernel

**Épico:** E04 · **Crate:** `crates/katu-core` · **Crate puro** (kernel, sem providers)

O **kernel** do katu: a máquina de estados do agente (DF1). O estado é um **valor**, a transição é
uma **função**, o log é a **fonte da verdade**.

---

## 1. Visão Geral

`katu-core` é o núcleo do agente. Implementa:

- A **máquina de estados** (`State`), a **transição pura** (`step`) e o **log append-only**.
- A **porta `Memory`** (tipos do katu, DF6) e a suíte de conformidade.
- Os **ports determinísticos** (`Clock`/`Rng`/`Fs`/`Env`/`Process`).
- O **modelo de erro** (`Error`/`ToolOutcome`) e o **diagnóstico estruturado** (`diag`).
- A **montagem de contexto** com orçamento e seleção por informação (matemática).
- A **estatística** (resumos, confiança, loop guard) e a **evidência tipada**.
- O **formato TOON** (colunar D39) ao modelo e o **envelope** das tools.
- A **auditoria** densa (índice invertido + Bloom + codec binário).
- A **verificação** determinística, o **plano tipado**, a **contenção soft** e o *taint*.

**Fronteira:**
- Depende de `katu-policy`; **não** depende de `katu-tools`/`katu-providers`/`katu-tui` nem de
  `knudge-core` (o adaptador vive no binário, E03).
- `#![forbid(unsafe_code)]`; sem `unwrap`/`expect`/`panic`; sem `HashMap` iterado (ordem canónica).
- Dependências: `katu-policy`, `serde`, `serde_json`, `thiserror`. Dev: `proptest`.
- Feature `instrument` (off por defeito): instrumentação transversal com **custo zero** compilada
  fora.

**Invariantes duros:**
- **`Model-visible ⟺ logged`** — o que o modelo vê é exatamente o que está no log.
- **Estado é valor** — sem singletons; cada `step` clona o estado.
- **Recusa não altera o estado** (§42) — fail-closed.
- **O agente não assina** — aprovações, overrides e re-enables exigem humano.

---

## 2. Arquitetura

### 2.1 Composição

```
┌──────────────────────────────────────────────────────────────────────────┐
│                            katu-core                                     │
├──────────────────────────────────────────────────────────────────────────┤
│  kernel (máquina de estados)                                             │
│  state │ event │ step │ project │ log │ hash │ budget │ pipeline         │
│  memory_gate │ cost │ guard │ confidence │ checkpoint │ bus │ session    │
├──────────────────────────────────────────────────────────────────────────┤
│  context (orçamento + seleção por informação)                            │
│  context │ select{units,greedy,info} │ compact │ prime │ state           │
├──────────────────────────────────────────────────────────────────────────┤
│  stats (resumos determinísticos)  │  evidence (base tipada)             │
├──────────────────────────────────────────────────────────────────────────┤
│  audit (índice invertido + Bloom + codec)                                │
│  audit{index,bloom,bin,codec,record,store}                               │
├──────────────────────────────────────────────────────────────────────────┤
│  toon (formato colunar ao modelo)  │  report (envelope) │ taint          │
├──────────────────────────────────────────────────────────────────────────┤
│  verify │ plan │ containment │ feedback │ validate │ model │ prompt      │
├──────────────────────────────────────────────────────────────────────────┤
│  memory (porta) │ ports (determinísticos) │ diag │ error │ provider      │
└──────────────────────────────────────────────────────────────────────────┘
                              │
                              ▼
                    katu-policy (motor de política)
```

### 2.2 Módulos

| Módulo | Responsabilidade |
|--------|------------------|
| `kernel/` | Máquina de estados, log, pipeline, custo, guard, sessão |
| `context/` | Orçamento, seleção por informação, compactação, prime |
| `stats` | Resumo determinístico (percentis, IC, MAD) |
| `evidence` | `Metric`/`EvidenceBasis`/`ArtifactRef` (DF5) |
| `audit/` | Auditoria densa pesquisável (índice + Bloom + binário) |
| `toon/` | Formato colunar D39 ao modelo + projeção |
| `report` | Envelope `ToolReport`, paginação, custo |
| `taint` | *Spotlighting* do output de tool (D1) |
| `verify/` | Gate de verificação determinístico |
| `plan` | Plano tipado + merge de escopo |
| `containment` | Contenção soft honesta |
| `feedback` | `CommandRecord`, `Ledger` (head/tail + spill), `redact` |
| `memory` | Porta `Memory` + conformidade + fake |
| `ports` | `Clock`/`Rng`/`Fs`/`Env`/`Process` + fakes |
| `diag` | Log estruturado + métrica de tempo (DF9) |
| `error` | `Error`/`ErrorKind`/`ToolOutcome` |
| `provider` | Porta `Provider` (streaming normalizado) |
| `model` | Projeções model-facing (outcome/erro/verificação) |
| `skill` | Skills do projeto (`.agents/skill{,s}`) |
| `prompt` | Condensação determinística do `AGENTS.md` |
| `validate` | `Issue`/`Issues` (erros que ensinam) |

---

## 3. Fundamentos

### 3.1 O caminho único (DF1)

```
State ──step(Event)──▶ Result<State, Refusal>
```

O estado é um `State` serializável; o log é a sequência de `Event`. `derive_messages` projeta o
histórico do modelo e `state_of` reproduz o estado. Tentativas falhadas ficam no log mas **não**
acrescentam histórico ao modelo.

### 3.2 Ordem §42 de uma tool call

```
ToolCall (logado ANTES) → política (evaluate) → efeito (Tool::execute) → ToolResult (logado)
```

Uma recusa de `step` **ou** de orçamento **não** altera o estado nem o log. O teste prova a negação
**pelo executor**: a `Tool` não é invocada.

### 3.3 Fail-closed em toda a linha

Vocabulário de política desconhecido, transição ilegal, pré-condição não satisfeita, aprovação sem
assinatura, teto de orçamento, `argv` opaco — todos recusam. Nenhuma recusa "corta a evidência".

---

## 4. Kernel em Detalhe

### 4.1 `kernel/state.rs` — o estado como valor

**`State`** (`Clone`/`Eq`/`Serialize`, mapas `BTreeMap`):

| Campo | Tipo | Nota |
|-------|------|------|
| `phase` | `Phase` | Fase do caminho único |
| `turn` | `u32` | Turno corrente |
| `turn_open` | `bool` | Turno aberto |
| `pending` | `BTreeMap<CallId, ToolUse>` | Só as chamadas **pendentes** |
| `completed_tools` | `BTreeSet<ToolName>` | Para `RequireAfter` |
| `waivers` | `BTreeSet<Phase>` | Pré-condições dispensadas (§47) |
| `capabilities` | `Vec<Capability>` | Grants correntes (DF2/DF4) |
| `budget` | `BudgetState` | Consumo da tarefa |
| `plan` | `Option<Plan>` | Exigido para `Planned` |
| `last_command` | `Option<CommandStatus>` | `exit_code: null` bloqueia (§31) |
| `workspace` | `Option<ResolvedPath>` | Raiz (E07-T05) |
| `verification` | `Option<VerificationReport>` | Exigido para `Verified` |
| `control` | `ControlState` | Modelo + pensamento (E12-T10) |

**Decisão de engenharia:** `pending` guarda **só** o intervalo `ToolCall → ToolResult`. Guardar as
concluídas tornava o replay **quadrático** (cada `step` clona o estado) e recusava `call_0` repetido
entre turnos, que é legítimo quando o provider não manda id.

**`RefusalReason`** (9): `TurnAlreadyOpen`, `NoOpenTurn`, `DuplicateCall`, `UnknownCall`,
`TurnMismatch`, `IllegalTransition`, `UnmetPrecondition`, `UnsignedApproval`, `MissingMacKey`.

**Caminho único:** `next_phase` avança um passo (`Task → KnowledgeConsulted → Planned →
Implemented → Verified → Persisted → Closed`); `can_transition` permite **qualquer** fase voltar a
`Task` (recusa/replan, §51.2) e proíbe `from == to`.

### 4.2 `kernel/event.rs` — eventos tipados

**`Event`** (17 variantes, tag `type`): `TurnStart`, `UserMessage`, `AssistantMessage`, `ToolCall`,
`ToolResult`, `PhaseTransition`, `Waiver`, `PlanRecorded`, `CommandRecorded`, `WorkspaceSet`,
`ApprovalGranted`, `ApprovalRevoked`, `VerificationRecorded`, `Control`, `ProjectContext`,
`PromptState`, `TurnEnd`.

- `ToolResult.delta` é o **delta model-visible** (§18/G6): a mesma string que o provider recebe.
- `ApprovalGranted` traz `signature` (MAC, D3): sem chave, recusa (fail-closed).
- `ProjectContext`/`PromptState` fecham `Model-visible ⟺ logged` para o prompt de sistema (estático
  e dinâmico): sem eles, uma retomada releria o `AGENTS.md` do disco e reproduziria um prompt
  diferente do que foi enviado.

### 4.3 `kernel/step/mod.rs` — a transição pura

`step(state, event) -> Result<State, Refusal>`:

- `TurnStart` → abre turno; `TurnEnd` → valida o número e fecha.
- `ToolCall` → recusa duplicado **pendente**; insere em `pending`.
- `ToolResult` → recusa chamada desconhecida; fecha o intervalo; `is_success()` marca
  `completed_tools`.
- `PhaseTransition` → valida forma **e** pré-condição.
- `ApprovalGranted` → recusa sem `reason`/`granted_by`; adiciona a capacidade.
- `ApprovalRevoked` → remove a capacidade (one-shot, B-06).

**Pré-condições** (`satisfies_precondition`): `KnowledgeConsulted` exige `Read` ou `MemoryRecall`
concluído; `Planned` exige plano; `Verified` exige relatório não bloqueado; `Closed` exige evidência
de fecho. Um `waiver` dispensa; um comando **ambíguo** (`exit_code: null`) bloqueia (§31).

### 4.4 `kernel/project.rs` — projeções puras

- **`derive_messages`** — histórico do modelo, **excluindo** eventos de controlo.
- **`state_of`** — reproduz o estado aplicando `step` a cada evento.
- **`snapshot`** — resumo compacto (fase, turno, mensagens, tools concluídas).
- **`Message`** — `User`/`Assistant`/`ToolCall`/`ToolResult`; o `ToolResult` carrega `delta` e
  `tool_name` (erro auto-contido, B-03).

### 4.5 `kernel/log.rs` — log append-only

`session.v1.jsonl`; cada linha é `LogRecord { seq, event }`, com `seq` **contíguo a partir de 1**.
Um salto de `seq` ou uma linha ilegível é corrupção (fail-closed).

**Durabilidade (ADR 0024, P-01):**
- `Durability::Event` (default) — barreira (`fsync`) por evento.
- `Durability::Turn` — escreve sem sincronizar; barreira em `Log::flush` no fim do turno (*group
  commit*). Troca uma janela de perda de um turno por um `fsync` em vez de um por evento
  (**−78,1 %** no caminho de anexação).

**Cauda rasgada:** um crash pode deixar a **última** linha sem `\n`; descarta-se e recupera-se (com
aviso). A marca é **não terminar em `\n`**: um ficheiro terminado é íntegro, pelo que uma linha
inválida aí é corrupção de verdade. Sem esta recuperação, um crash tornaria a sessão irrecuperável.

`read_records_from(offset, first_seq)` lê só a cauda a partir de um offset (retomada incremental).

### 4.6 `kernel/hash.rs` — hash canónico

**FNV-1a de 64 bits:**
```
offset = 0xcbf29ce484222325
prime  = 0x00000100000001b3
h = offset; para cada byte b: h ^= b; h = h.wrapping_mul(prime)
```
`canonical(value)` = FNV-1a de `serde_json::to_vec(value)` (JSON determinístico, chaves ordenadas).
Usado na assinatura de chamada (Q-12) e na integridade do snapshot (Q-15). **Não é criptográfico**:
é um detetor de divergência, não uma defesa contra adversário (ADR 0008).

### 4.7 `kernel/budget/mod.rs` — orçamento

**`Budget`:** `turns`, `tool_calls`, `tokens`, `wall_clock_ms` (soma saturante).
**`Charge`:** `Turn`, `ToolCall`, `Tokens(u64)`, `WallClock(u64)`.
**`BudgetCap`:** tetos opcionais por eixo.

**Regra do teto:** o teto é atingido quando o uso **ultrapassa** o cap (`used > cap`); `used == cap`
ainda é aceite (o teto foi alcançado, não excedido). `check` projeta **sem aplicar** — o uso fica
inalterado mesmo em recusa (§29); `commit` aplica.

### 4.8 `kernel/pipeline/mod.rs` — facto → política → efeito

`dispatch`/`dispatch_with`:
1. `facts_from` monta `Facts` (estado + capacidades explícitas + workspace implícito).
2. `evaluate` (instrumentado **pelo chamador** — firewall S-03).
3. Se `Allow` → `tool.execute(use_)`; senão → `Effect::Skipped`.

**`Tool` (trait):** `Send + Sync` (uma tool `Shared` corre num worker paralelo).
**`Dispatch`:** `decision` + `effect`; `outcome()`/`report()`/`delta()`.
**`with_estimated_cost`:** preenche custo **advisory** (bytes TOON, `tokens_est = bytes/4`) quando a
tool não o fez — nunca decide nada (DF5).

### 4.9 `kernel/memory_gate.rs` — gate de escrita de memória

`enforce_memory_write`: `pre_write` → capacidade → política → efeito. O adaptador **concede**
`Capability::Command { tool: MemoryWrite }` só quando o `pre_write` não rejeita. Sem a capacidade,
as regras `deny_command` disparam — falha fechada (DF4).

### 4.10 `kernel/cost/` — Cost governor (E09-T06)

Camadas avaliadas do mais específico para o mais global (o primeiro que dispara vence):
```
KillSwitch → PerTool → RollingWindow → FinancialVelocity → Global
```
- `PerTool` antes do `Global`: um loop patológico é cortado na camada específica, com a causa exata.
- Nenhuma recusa altera o uso (§29): `check` verifica, `commit` aplica.
- As camadas temporais só correm quando o débito traz `now_millis` (o log não é reprodutível nesse
  eixo).
- **Kill switch:** só reabre com `Reenable` (motivo + autor obrigatórios; o agente não assina).
- `VELOCITY_WINDOW_MS = 60_000`; `history` guarda `(ms, micros)`; `prune` descarta fora da maior
  janela.
- `from_events` reconstrói uso global e por ferramenta.

### 4.11 `kernel/guard.rs` — deteção de loop (CUSUM + e-value)

**Assinatura de chamada:** `Fingerprint::of(name, arguments)` = FNV-1a de `nome + \x1f + JSON`.
`Call { print, mutates }`: `exclusive` (write/edit/move/bash) **reinicia** o detector.

**CUSUM** sobre a fração de repetição: `S ← max(0, S + x − k)`, alarme se `S ≥ h`; `x = repetição`
(`1000 − novidade`), `k = 500`, `h = 2000`.

**e-value** sobre o binário "passo inteiramente repetido": razão de verosimilhança `Λ_n`; passo
`log(p₁/p₀)` (repetido) ou `log((1−p₁)/(1−p₀))` (não repetido); `p₁ = 600`, `p₀ = 100`. Rejeita
quando `log Λ_n ≥ log(1/α)`; `α = 10` (milésimos) → limiar `ln(100) ≈ 4605`.

**Por Ville**, `P_{H0}(∃n: Λ_n ≥ 1/α) ≤ α` para **qualquer** regra de parada — a rejeição é
*anytime-valid* (parada opcional), ao contrário do SPRT clássico (só garante `α` a `n` fixo).

**Default** (`GuardParams::DEFAULT`): `min_steps = 3`. Com passo repetido, a e-value alarme no
**5.º** passo; o CUSUM precisaria de 5 repetições seguidas. Medido: 0 falsos positivos em 200 turnos
normais.

### 4.12 `kernel/confidence.rs` — ponte log↔estatística

`katu-policy` tem a estatística mas não conhece o log. Aqui a ponte:
- **`rule_trials`** — por regra, *recusou ⇒ não correu*. Uma violação é a chamada recusada a
  aparecer executada sob o **mesmo** `CallId`. `ApprovalGranted` conta como ensaio honrado.
- **`tool_trials`** — por tool: ensaio = chamada; sucesso = não expirou (`Timeout`).
- **`enforced_verdicts`/`enforced_verdicts_report`** — só regras **declaradas** `Enforced`; aplica
  `control_fdr` sobre a família (único sítio onde a família existe).

### 4.13 `kernel/session/snapshot.rs` — snapshot com hash (Q-15)

`StateSnapshot { schema_version=4, seq, offset, budget, per_tool, history, state, hash }`.
- O `hash` é **derivado** do estado no `save` (nunca aceite de fora).
- No `load`, um snapshot cujo hash **não casa** com o seu próprio estado é **descartado** (replay
  total, fail-closed).
- **Duas fronteiras:** transição de fase (contrato) e fim de turno com cauda ≥
  `MAX_TAIL_BYTES = 128 KiB`. É o teto que faz a retomada ser **O(1)** no comprimento da história.
  Medido: 20 000 turnos retomam em ~321 µs contra ~23 839 µs de replay total.

### 4.14 `kernel/session/mod.rs` — sessão e loop mínimo

`Session` abre/reproduz o log (com snapshot), aplica `step` e grava. `apply_at` valida transição
**e custo** antes de gravar:
```
step (transição) → cost.check → log.append → cost.commit → state = next → flush (fim de turno) → maybe_snapshot
```
- `tool_call` faz `begin_call` (§42) → `dispatch` → `settle_call`.
- `begin_call`/`settle_call` expõem as duas metades para lotes `Shared` em paralelo (B-01).
- `can_afford_tool_calls` sonda o lote **sem o alterar** (um débito recusado a meio deixaria
  `ToolCall` sem `ToolResult` no log).
- `memory_write` corre o gate E05.
- Outros: `set_workspace`, `record_verification`, `approve`/`revoke_approval`, `context`, `verify`,
  `changed_files`, `recorded_commands`, `messages`, `fork`.

### 4.15 `kernel/control.rs`, `bus`, `checkpoint`

- **`Control`/`ControlState`** (E12-T10): modelo/pensamento do **utilizador** no log e no estado
  (sobrevive a resume); validação pura com erro que **ensina** (`ReasoningUnsupported`).
- **`EventBus`**: observadores + waterfall com a regra "tem de chamar `next`".
- **`Checkpoint`**: schema v1, validador zero-dep `validate`, `write_atomic`; erros agregados em
  `Issue { path, message }` (OA19/E09-T02).

---

## 5. Contexto (orçamento + seleção por informação)

### 5.1 `context.rs` — um só caminho (S-01)

`assemble_all` faz **uma** derivação do log, **uma** partição em unidades e **um** teto por turno
(antes derivava-se o log três vezes). O que não cabe em `raw_min` é o prefixo; o digest trabalha só
sobre ele.

**Estimativa de tokens:** `tokens_from_bytes(bytes) = ceil(bytes · 1000 / 3631)`, com
`BYTES_PER_TOKEN_MILLI = 3_631` — rácio **medido** com o tokenizer do modelo local (Q-01). Substitui
a estimativa `bytes/4` do E09-T01, que **subestimava** ~9 %. Pública: o gate `gate:prompt` usa o
**mesmo** rácio (uma segunda constante seria *drift*).

**`ContextBudget`:** `raw_min` (mínimo cru), `summary_max` (teto do resumo).
**`AssembleOptions`:** `prime`, `compaction`, `selection`, `params`, `goal`, `state`.
**`Context`:** `prime`, `summary`, `messages`, `raw_tokens`, `tokens`.

### 5.2 `context/select/units.rs` — unidades

Uma unidade é uma mensagem isolada ou uma **corrida maximal** de mensagens de tool (a mesma
fronteira do corte Q-02a: o wire exige que `role: "tool"` responda ao `tool_calls` precedente).
`message_text` devolve `Cow<'_, str>` (empresta quando possível); os bytes contados são os do texto
model-visible (o *delta*, não o `ToolOutcome`). `evidence_of`: `ToolResult=3`, `User=2`,
`Assistant=1`, `ToolCall=0`.

### 5.3 `context/select/greedy.rs` — utilidade submodular + MMR + RRF

**IDF:** `idf(t) = ln(1 + n/df(t))` em milésimos de nat; `df=0` → peso máximo. O `idf` é
**pré-calculado** (`Stats::of`), porque o greedy consulta a massa O(n²) vezes.

**Utilidade:** `I(S) = Σ_{t∈S} idf(t)`. O ganho marginal é a massa dos termos **ainda não cobertos**
— a função é **submodular**, logo o greedy tem a garantia `1 − 1/e` (Nemhauser et al.). Sem
conjuntos temporários no ganho marginal; pertença ao escolhido é um bitmap O(1).

**Score greedy:**
```
utility = marginal · (1000 − λ)
penalty = similarity · λ
score   = utility + rrf − penalty
```
com filtro `similarity > sim_max` e desempate pelo índice (determinismo). `λ = 250`.

**Jaccard** em milésimos, numa só passagem por cursores:
`|A ∩ B| / |A ∪ B| · 1000`.

**RRF** (Reciprocal Rank Fusion) dos 4 canais `["recencia","massa","objetivo","evidencia"]`:
```
score(i) = Σ_c w_c · 1000 / (k + rank_c(i) + 1)
```
`k_rrf = 60`, `w = [1000, 1000, 1000, 500]`. O RRF combina rankings sem calibrar escalas.

### 5.4 `context/select/info.rs` — divergência JS

`JS(P‖Q) = ½ KL(P‖M) + ½ KL(Q‖M)`, `M = (P+Q)/2`, com **suavização de Laplace** (`1/size` no
numerador, `+1.0` no total) para tratar distribuições vazias e evitar `ln 0`. Devolve milésimos de
nat. `JS = 0` ⇒ o sufixo cobre o prefixo.

### 5.5 `context/select.rs` — políticas e termos

**`SelectionPolicy`:** `Suffix` (histórica, default) / `Utility` (Q-02b/Q-03).
**`SelectionParams::DEFAULT`:** `lambda_milli=250`, `sim_max_milli=700`, `k_rrf=60`,
`tau_js_milli=200`, `channel_weights=[1000,1000,1000,500]`.
**Termos:** minúsculas alfanuméricas, `MIN_TERM_CHARS = 3`; sem *stopwords* (o `idf` capta a
redundância).
`chosen_units` fixa a **última** unidade (turno corrente intacto); se nem ela cabe, recua ao sufixo
(nunca a um contexto inválido).

### 5.6 `context/compact.rs` — compactação (E09-T07, Q-03)

Digest **determinístico** (sem LLM) do prefixo: tabela `m` (`kind`, `id`, `text` de uma linha por
mensagem) + mapeamento original→substituto. O original continua endereçável (`recover`).

Com `SelectionPolicy::Utility`, as linhas são escolhidas pela **mesma** máquina da seleção e a
compactação só se aplica se `JS(prefixo ‖ sufixo) ≥ τ_JS` (um resumo que não acrescenta informação
não se paga). `DIGEST_EXCERPT_BYTES = 48`. Ganho como `Metric` base `inferred`.

### 5.7 `context/prime.rs`, `context/state.rs`

- **`prime`** (`PRIME_VERSION = 5`): prime compacto, determinístico e versionado, aparece **uma**
  vez.
- **`context/state`** (Q-04): secção `estado` (modo, regras que travam, teto de passos, *working
  set*), com teto de bytes e sem duplicados, no **fim** do prime. Registada como `Event::PromptState`
  com o texto exato. Estável **dentro** do turno, de propósito: um valor que mudasse a cada passo
  destruiria o cache de prefixo do provider. `MAX_SECTION_BYTES`, `MAX_WORKING_SET`.

---

## 6. Estatística (`stats.rs`) — a matemática do resumo

`Summary` é a **fonte única** do `diag`, do `measure_mvk` e dos gates. **Sem RNG**: mesma amostra →
mesmo resumo.

| Campo | Fórmula |
|-------|---------|
| `p50`, `p95` | Percentil *nearest-rank* |
| `mean` | Média inteira arredondada |
| `ci95_low/high` | Normal (`n ≥ 30`) ou *bootstrap* (`n < 30`) |
| `mad` | Mediana dos desvios absolutos |
| `robust_ci95_low/high` | Mediana ± 1,96·MAD/√n |

**Percentil *nearest-rank*** (`basis_points`: 5000 = p50, 9500 = p95):
```
rank  = ceil(bp · n / 10000)      (mínimo 1)
index = rank − 1
```

**Média inteira:** `sum(u128) + n/2`, depois `/ n` (arredonda ao mais próximo, saturando).

**MAD:** `mediana(|x_i − mediana|)`. Robusto a outliers (ao contrário do desvio padrão).

**IC robusto:** `mediana ± Z95 · MAD / √n`, com `Z95 = 1.959963984540054`.

**IC normal** (`n ≥ 30`): `média ± Z95 · s/√n`, com `s²` a variância amostral (`n−1`).

**Bootstrap determinístico** (`n < 30`): `RESAMPLES = 2000`, quantis `250‰` e `9750‰`; os índices
saem de **SplitMix64** semeado pelo próprio `n`:
```
state = n · 0x9E3779B97F4A7C15 ^ 0xD1B54A32D192ED03
SplitMix64: state += 0x9E3779B97F4A7C15;
            z = state; z = (z^(z>>30))·0xBF58476D1CE4E5B9;
            z = (z^(z>>27))·0x94D049BB133111EB; z ^ (z>>31)
```

**Coerência:** o IC é **alargado** para conter a mediana e a média (um IC que exclui o centro não é
um resumo honesto). `MIN_SAMPLES = 5`; os gates usam `try_from_samples` (recusa abaixo disso).

### 6.1 `stats/tests/conformal_bench.rs` — C2 rejeitado

O *split conformal* foi **medido e rejeitado**; a fórmula vive no `#[cfg(test)]`, não em `src/`:
`k = ⌈(n+1)·nível⌉`; quantil conformal = `k`-ésimo menor resíduo; cobre se `|erro| ≤ q`. Com
resíduos **correlacionados** (forecast rolante) a cobertura mergulha abaixo do nominal mesmo no
regime trocável; o log real não tem base (`n_cal ≥ 19` por regra, hoje 0). *Regra:* item rejeitado
deixa o **número**, não uma API pública sem consumidores.

---

## 7. Evidência (`evidence.rs`) — DF5

**`EvidenceBasis`** (7): `Measured`, `Inferred`, `ProviderReported`, `BenchmarkCounterfactual`,
`Observed`, `Verified`, `Unpriced`.
- `requires_artifact()` — `Measured`/`ProviderReported`/`BenchmarkCounterfactual`/`Observed`/
  `Verified`.
- `is_publishable()` — exclui `Inferred`/`Unpriced`.

**`Metric`:** `name`, `value`, `unit`, `basis`, `artifact`. `new` valida (artefacto obrigatório;
`unpriced` só admite zero). `is_publishable` = base publicável **e** artefato presente.
**`Metric::sum`** — só soma bases iguais (a base **não** muda numa agregação).
**`Unit`:** `Nanos`, `Millis`, `Micros`, `Count`, `Ratio`, `Bytes`, `Tokens`, `Unspecified`.

---

## 8. Auditoria (`audit/`) — índice denso pesquisável (ADR 0009)

Histórico completo em `.katu/audit`, local e nunca versionado. Segmentos colunares imutáveis + índice
invertido derivado.

### 8.1 `audit/index.rs` — índice invertido

**`Posting { field, ln, pos }`** — ocorrência de termo. **`Index`** = `BTreeMap<termo, Vec<Posting>>`.
Consulta avalia em `O(candidatos)` via postings; as frases usam as **posições** dentro do mesmo
campo. `Query::parse`: `termo`, `"frase"`, `kind:x`, `tool:x`, `status:x`, `path:<glob>`, `OR`.

### 8.2 `audit/bloom.rs` — filtro de Bloom (ADR 0009)

Determinístico, duplo *hashing* FNV-1a. Sem falsos negativos; um falso positivo custa uma leitura a
mais.
```
bits = max(512, termos · 10).next_power_of_two()
k = 4 hashes
h1 = FNV1a(term);  h2 = FNV1a(h1.to_le_bytes())   (h2 = 0 → 1)
posição_i = (h1 + i·h2) mod bits
```

### 8.3 `audit/bin.rs` — codec binário

Formato `KAI1`: magic, `bloom_len` varint, bits, `term_count` varint, e por termo (ordem canónica)
`term_len`+bytes, `count`, postings com `field` (u8), `ln` em **delta** e `pos` absoluto (varint).
**LEB128** varint; `field` já é 1 byte (sem compressão). Cursor com verificações (sem indexação por
`[]`).

### 8.4 `audit/codec.rs`, `record.rs`, `store.rs`

- `codec`: manifesto (`manifest.json`, versão validada) e tabela `a` colunar (`RS`/`US`).
- `record`: `AuditRecord` (7 campos), `MAX_TEXT_BYTES`.
- `store`: `AuditStore`, `Hit`, `Manifest`, `SegmentInfo`, `AUDIT_SCHEMA_VERSION`,
  `SEGMENT_EVENTS`.

---

## 9. TOON (`toon/`) — o formato ao modelo (DF12/E06-T12)

`Value` é a árvore (mapa ordenado, lista, escalar, `Flow` inline, `Block` literal). O formato ao
modelo é **colunar D39** (ADR 0005): `\x1e` (RS) prefixa tabela, `\x1d` (GS) bloco literal, `\x1f`
(US) separa células, `\n` termina linha. **Sem headers** (o esquema vive no prime) e sem `null`.

### 9.1 `toon/colunar.rs` — emissão numa só alocação + SWAR

- `byte_len` calcula o tamanho **exato** antes de escrever (uma só alocação; teste
  `the_reserved_capacity_is_exact`).
- **SWAR** de 8 bytes para o teste "existe byte `< 0x20`":
  ```
  (word − 0x2020202020202020) & !word & 0x8080808080808080 != 0
  ```
  ~8× menos iterações; o byte mais baixo abaixo do limiar nunca recebe `borrow`, logo não há falsos
  negativos; um falso positivo só custa a passagem lenta (exata).
- `push_sanitized` escreve numa passagem, sem alocar se não houver nada a substituir; o comprimento
  em bytes é **preservado** (substituições de 1 byte), o que torna o corte de `to_delta` exato.
- `Cell<'a>` usa `Cow<'a, str>` (empresta o payload em vez de clonar).

### 9.2 `toon/project.rs` — projeção guiada pelo esquema

Escalares no topo → secção `k` (`k`,`v`); `List<Map>` → secção de linhas com as colunas do registo
(key ausente = vazio); `List<escalar>` → secção de uma coluna (`ref`); `Block`/`List<Str>` → bloco
literal; lista dentro de uma linha → secção-filha `{pai}.{chave}`. As listas aninhadas são
**emprestadas** (`&'a [Value]`), não clonadas.

### 9.3 `toon/schema.rs` — registo do esquema

Fonte única de verdade: a projeção e o prime leem daqui (sem *drift*). `ColumnSpec`, `Mode`
(`Rows`/`Literal`), `TableSpec`; `validate()` rejeita secções sem esquema.

---

## 10. Envelope, Taint, Feedback

### 10.1 `report.rs` — envelope das tools

**`ToolReport`:** `kind`, `id`, `hash`, `data`, `page`, `next`, `cost`.
**`Page { cursor, total, truncated }`**, **`Cost { bytes, ms, tokens_est }`**.

**`MAX_DELTA_BYTES = 8_192`** — teto do **delta** que entra no log e no prompt. `to_delta` corta em
fronteira de linha e deixa um **ponteiro** (`kind`/`id`) para o modelo pedir uma página. O embrulho
de *taint* acontece **aqui** (não no encoder) para manter `Model-visible ⟺ logged`.

**`content_id(prefix, seed)`** = `"{prefix}_{fnv1a(seed):016x}"` (content-addressed).

### 10.2 `taint.rs` — spotlighting (D1)

O delta de tool é **dado não confiável**, nunca instrução. Três invariantes:
1. `spotlight` põe o payload inteiro entre tags, uma vez, e nada mais.
2. `escape` **não altera o comprimento** (`<` que inicia `katu:`/`/katu:` → `[`), logo
   `MAX_DELTA_BYTES` continua a ser um teto **exato**.
3. `inspect` verifica a estrutura (abre no índice 0, fecha no fim, zero tags cruas).

Envelope: `<katu:untrusted kind="…" bytes="0000123">\n…\n</katu:untrusted>`. `BYTES_DIGITS = 7`
(largura fixa ⇒ custo independente do payload). `kind` sanitizado para `[a-z0-9._-]`. **Limite
declarado:** é contenção **estrutural**, não obediência do modelo. Medido: 6/6 ataques bloqueados,
custo 71 B por resultado (0,87 % de 8 KiB).

### 10.3 `feedback.rs` — feedback de comando

**`Ledger`** (B-04): `head=2048`, `tail=2048`, `spill_threshold=8192`. Renderiza head/tail +
ponteiro de spill; o texto model-visible nunca excede `head + tail`.
**`redact`** — linha a linha, chaves com `SECRET_MARKERS` (`KEY`/`SECRET`/`TOKEN`/`PASSWORD`/
`PASSWD`/`CREDENTIAL`/`AUTHORIZATION`) → `[redacted]`.
**`CommandStatus::is_ambiguous`** — `exit_code: null` bloqueia avançar (§31).
A **rotação** de ficheiros fica deliberadamente de fora (o projeto nunca apaga automaticamente).

---

## 11. Verificação, Plano, Contenção

### 11.1 `verify/` — gate determinístico (E09-T03)

`verify` é **puro** (escopo + feedback + cobertura, zero LLM). `CheckStatus`: `Pass < Warn < Block`
(ordem crescente de gravidade). `--strict` promove `Warn` a `Block`.

**Checks** (`verify/checks.rs`):
- `scope.forbidden` (Block se algum proibido alterado), `scope.allowed` (Warn se fora).
- `feedback.timeout`/`feedback.ambiguous` (Block), `feedback.exit` (Warn).
- `coverage` — `coverage_bps = in_scope/total · 10000` (pontos base); `Warn` se abaixo do piso.

**`Override`** assinado (`reason` + `overridden_by`) registado em `overrides.jsonl` (append-only).
Um `block` **não** é sobreponível pelo agente.

### 11.2 `plan.rs` + `plan/merge.rs` — plano tipado (E06-T06/E09-T04)

**`Plan`:** `scope_contract` + `feature_list`. `validate` exige `forbidden_files` e `rollback_plan`
não vazios, lista não vazia, **≤ 1** `in_progress` e globs **relativos** (sem `/` inicial nem `..`).
`ScopeContract::allows`: **proibido vence**; `allowed` vazio = tudo.

**Merge por menor privilégio:** `allowed` = **interseção** (glob-aware, conservadora), `forbidden` =
**união**, `time_budget_minutes` = **mínimo** (`None` = sem teto), `network_egress` = **AND**,
`acceptance_criteria` = união, `rollback_plan` = primeiro não vazio. Se ambos os `allowed` são não
vazios e a interseção é vazia, o merge **falha** (fail-closed) em vez de conceder "tudo".
`glob_covers(a, b)` prova a inclusão comum (`src/**` cobre `src/parser/**`) e devolve `false` quando
não consegue provar — consequência: escopo mais restrito, nunca mais permissivo.

### 11.3 `containment.rs` — contenção soft honesta

No MVP **não há jail de SO**: o katu corre global de facto. `SandboxEnforcement` é **sempre**
`Soft`; pedir `Full`/`Partial` devolve `ContainmentError::Unavailable` (fail-closed), nunca execução
livre. `workspace_capabilities(root)` deriva `Capability::Workspace { root }` (grant **implícito**,
distinto do `ReadPath`/`WritePath` explícito). Gancho `Jail`/`NoJail` para a jail futura (E17).

---

## 12. Portas, Memória, Provider

### 12.1 `ports/` — determinismo (E01-T02)

O núcleo não conhece o SO, o terminal, o relógio nem o RNG globais: tudo atravessa uma porta.
- `Clock`/`FixedClock`/`Timestamp`
- `Rng`/`SeqRng`
- `Fs`/`FsError`/`MemFs` — `write_atomic_if` (CAS para `edit`, OA16), `remove` (só ficheiros),
  `append`/`append_unsynced`/`sync`, `read_from`.
- `Env`/`FakeEnv`
- `Process`/`MemProcess`/`ExecRequest`/`ExecResult`/`ProcessError` (execução com timeout, E06-T04).

### 12.2 `memory.rs` — porta `Memory` (DF6)

Contrato com **tipos do katu** (nenhum tipo do `knudge-core`): `pre_write`, `pre_edit`, `record`
(commit por nota, OA8), `search`, `query` (default fail-closed `Unavailable`), `session_end`,
`status`. Submódulos: `types` (`NoteType`, `Status`, `Basis`, `NoteRef`, `Anchor`, `Score` — pontos
base `0..=10_000`), `io`, `query`, `error` (`retryable()` só em `Timeout`), `fake` (`FakeMemory`),
`conformance` (`assert_contract`).

### 12.3 `provider.rs` — porta `Provider` (E12)

Contrato de **endpoint de modelo**, streaming normalizado. `Provider::{models, dynamic_models,
capabilities, model_for_tier, stream}`. `Thinking` (Off/Low/Medium/High), `Tier`, `ModelSpec`,
`ModelCapabilities`, `TokenUsage` (base `EvidenceBasis`), `CollectSink`. `ProviderEvent::ToolCall` é
**completa** (acumulada). `ProviderError` mapeia para a taxonomia do katu. O núcleo compila com ou
sem provider ligado (firewall LLM-free).

---

## 13. Diagnóstico, Erro, Modelo

### 13.1 `diag/` — DF9/E19

- **Logs estruturados**: identificador estável (catálogo `events`) + campos tipados; nunca texto
  livre interpolado.
- **Custo zero por defeito**: sem `feature = "instrument"`, `span!`/`event!` são *no-op* e o caminho
  ativo **não existe** no binário. On-demand em runtime (`KATU_INSTRUMENT=1`).
- `Record { level, event, function, kind, duration_nanos, fields }`; `Sink` (porta);
  `Span` (RAII); `fingerprint!` (impressão determinística com comprimento prefixado).
- `diag/aggregate.rs`: `AggregatingSink` agrega por `(event, function)`, percentis calculados no
  *dump*. `EventSummary` traz contagem, total, min, p50, p95, p99, max, IC95.
- `diag/redact.rs` (redação no caminho de diagnóstico), `diag/events.rs` (catálogo).

### 13.2 `error/` — modelo de erro (E01-T06)

**`ErrorKind`** (10) é o contrato de máquina: `as_str()` (envelope) e `exit_code()` (processo, 2–70).
**`Error`** é rico em contexto e encadeável (`#[source]`); todo I/O carrega `path`.
**`ToolOutcome`** (5): `Ok`, `Partial`, `Denied { rule_id, evidence }`, `Timeout`,
`Unavailable { control, rule_id }`.
- `is_success()` = `Ok | Partial`.
- `rule_id()` — DF10.
- `fix()` (B-03) — o remédio acionável: a negação **ensina** a corrigir-se.

### 13.3 `model.rs` — projeções model-facing

`ToolOutcome::to_value`/`summary`, `Error::to_value`, `VerificationReport::to_value` — escalares
explícitos no topo, listas em tabelas; domínios fechados da **mesma** fonte que o prime ensina (sem
*drift*).

### 13.4 `skill.rs`, `prompt.rs`, `validate.rs`

- **`skill`** (E20-T13): descoberta **fail-open** de `.agents/skill{,s}/*/SKILL.md`; `parse`
  (frontmatter `name`/`description`); `catalog` leva **nome, primeira frase (≤ `MAX_HINT_CHARS = 100`)
  e caminho relativo**, ordenado por relevância (Q-05: −73,9 % de bytes).
- **`prompt`** (Q-19): `condense` tira a sintaxe redundante do markdown (link cujo texto repete o
  alvo, negrito, linhas em branco), **sem perder texto nem alvos** (−29,1 %). Pura, **sem cache**
  (uma passagem linear sobre ~1,6 KB não paga um `fs.write` com `sync_all`).
- **`validate`** (OA19): `Issue { path, message }` + `Issues` agregado, ordem determinística,
  partilhado pelo validador de checkpoint e pelo linter de schema de tools.

---

## 14. Matemática — índice consolidado

| Técnica | Onde | Fórmula |
|---------|------|---------|
| FNV-1a 64 | `hash`, `report`, `bloom` | `h ^= b; h *= 0x100000001b3` |
| Percentil nearest-rank | `stats` | `rank = ceil(bp·n/10000)` |
| Média inteira | `stats` | `(Σ + n/2)/n` |
| MAD | `stats` | `mediana(\|x−mediana\|)` |
| IC robusto | `stats` | `mediana ± 1.96·MAD/√n` |
| IC normal | `stats` | `média ± 1.96·s/√n` |
| Bootstrap SplitMix64 | `stats` | 2000 reamostras, quantis 250/9750‰ |
| IDF | `context/select` | `ln(1 + n/df)` |
| Jaccard | `context/select` | `\|A∩B\|/\|A∪B\|` |
| MMR | `context/select` | `util·(1−λ) − sim·λ` |
| RRF | `context/select` | `Σ w/(k+rank+1)` |
| JS | `context/select` | `½KL(P‖M)+½KL(Q‖M)` |
| CUSUM | `guard` | `S ← max(0, S+x−k)` |
| e-value (Ville) | `guard` | `Λ_n`; corte `log(1/α)` |
| Beta-Bernoulli | `policy`/`confidence` | `(1+s)/(2+n)` |
| Wilson LB | `policy`/`confidence` | unilateral, `z=1645‰` |
| Binomial upper tail | `policy`/`confidence` | `P[X≥s\|n,θ]` |
| Benjamini-Hochberg | `policy`/`confidence` | `p_(i) ≤ q·i/m` |
| ECE / Brier | `policy`/`confidence` | calibração em 10 baldes |
| Bloom | `audit/bloom` | `max(512, 10·termos)`, `k=4` |
| SWAR | `toon/colunar` | `(w−0x2020…) & ~w & 0x8080…` |
| LEB128 | `audit/bin` | varint base-128 |
| Rácio de tokens | `context` | `ceil(bytes·1000/3631)` |
| Cobertura | `verify` | `in_scope/total·10000` bps |
| Conformal (rejeitado) | `stats/tests` | `k = ⌈(n+1)·nível⌉` |

---

## 15. Abordagens de Engenharia

### 15.1 Estado como valor
`step` é uma função pura `(State, Event) -> Result<State, Refusal>`. Sem singletons; o replay é
exato. Guardar só as chamadas **pendentes** evita replay quadrático e permite `call_0` repetido.

### 15.2 Log como fonte da verdade
`Model-visible ⟺ logged`. `derive_messages` projeta o histórico; eventos de controlo não vão ao
modelo. `ProjectContext`/`PromptState` fecham o invariante para o prompt de sistema.

### 15.3 Fail-closed
Vocabulário desconhecido, transição ilegal, pré-condição não satisfeita, aprovação sem assinatura,
teto, `argv` opaco — recusam. Nenhuma recusa altera o estado (§42).

### 15.4 O agente não assina
`ApprovalGranted` (MAC), `Override` (reason + overridden_by) e `Reenable` (motivo + autor) exigem
humano. Capacidades são **one-shot** (B-06): revogadas depois de usadas.

### 15.5 Determinismo
Sem RNG no hot path: SplitMix64 no bootstrap, percentis nearest-rank, `BTreeMap` sempre, ordem
canónica, desempate por índice. O mesmo input dá o mesmo output.

### 15.6 Custo zero por defeito
A instrumentação (`instrument`) compila fora por defeito; o caminho ativo não existe no binário.
On-demand em runtime.

### 15.7 Um só caminho de orçamento (S-01)
Uma derivação do log, uma partição em unidades, um teto. Antes derivava-se o log três vezes.

### 15.8 Seleção submodular
A utilidade `I(S) = Σ idf` é submodular; o greedy tem garantia `1 − 1/e`. IDF pré-calculado,
ganho marginal sem conjuntos temporários, pertença por bitmap.

### 15.9 Anytime-valid (Ville)
A e-value é um martingale sob `H0`: o corte `log(1/α)` é válido para **qualquer** regra de parada,
ao contrário do SPRT clássico.

### 15.10 Confiança medida, não declarada
`Enforced` só com `n ≥ n_min` **e** `LB ≥ θ`; senão demove com a evidência. O limiar é **dado**
(DF8), nunca derivado dos dados.

### 15.11 FDR fail-closed
O BH só pode **tirar** promoções. Evita ~`α·m` promoções falsas.

### 15.12 Evidência viaja com o número
`Metric` carrega base e artefacto; a base não muda numa agregação; `unpriced` é zero.

### 15.13 Um só sítio para cada facto
`hash` (um só FNV-1a), `glob` (a única semântica), `toon::schema` (fonte única do esquema), o rácio
de tokens (usado pelo orçamento **e** pelo gate). Sem *drift*.

### 15.14 Emissão numa só alocação + SWAR
`byte_len` reserva o tamanho exato; a sanitização usa SWAR de 8 bytes; o payload é emprestado
(`Cow`). Medido: `emit` −62 % em dev e −33,3 % em release.

### 15.15 Taint estrutural
O envelope separa dado de instrução; `escape` neutraliza sem alterar comprimento (o teto continua
exato). Contenção **estrutural**, declarada como tal.

### 15.16 Durabilidade configurável (group commit)
`Durability::Event` (fsync por evento) ou `Turn` (fsync por turno) — troca uma janela de perda por
menos `fsync`. A cauda rasgada é recuperada; corrupção a meio continua erro.

### 15.17 Snapshot reconstruível com hash
O snapshot é otimização: se faltar/corromper, replay total. O hash canónico descarta um snapshot
inconsistente (fail-closed). `MAX_TAIL_BYTES` faz a retomada ser O(1).

### 15.18 Índice denso pesquisável
Segmentos colunares imutáveis + índice invertido com posições; Bloom evita abrir segmentos sem o
termo; codec binário delta+varint. A compactação não toca aqui.

### 15.19 Contenção soft honesta
`SandboxEnforcement` sempre `Soft`; pedir `Full`/`Partial` falha. A limitação é declarada, nunca
escondida.

### 15.20 Erros que ensinam
`ToolOutcome::fix` e `ControlError` dizem o que passaria. `Issue` aponta o campo exato. A negação
ensina o modelo a corrigir-se.

### 15.21 Skills fail-open
Sem `.agents/` → lista vazia; sem descrição → não carregada. O arranque nunca quebra.

### 15.22 Condensação sem cache
Passagem linear sobre ~1,6 KB não paga um `fs.write` com `sync_all` (25,8 ms). Rejeitado com o
número.

---

## 16. Gaps, Flags e Pendências

### 16.1 Limitações Declaradas

| Limitação | Descrição |
|-----------|-----------|
| Contenção soft | Sem jail de SO no MVP (E17 futura); a limitação é declarada |
| Taint estrutural | Separa dado de instrução; **não** impede o modelo de ser influenciado pelo conteúdo |
| Snapshot | Otimização reconstruível; divergência → replay total |
| Conformal | Medido e **rejeitado**; a fórmula vive no bench, não em `src/` |
| Calibração | In-sample: mede o conservadorismo do limite, não o acerto do modelo |
| Custo advisory | `tokens_est = bytes/4` no envelope é heurística; nunca decide (DF5) |
| Rotação de logs | Deliberadamente fora: o projeto nunca apaga automaticamente |

### 16.2 Gaps / Dívida

| Item | Descrição |
|------|-----------|
| `Perception` | Categoria existe em `katu-policy`; o motor de política só ignora `Advisory` |
| C2 (conformal) | Rejeitado: só entra se o log der base de calibração não correlacionada (`n ≥ 19`) |
| Provider | O núcleo define só o contrato; a validação ao vivo é dos adaptadores |

### 16.3 Flags / Constantes de Operação

| Constante | Valor | Uso |
|-----------|-------|-----|
| `MIN_SAMPLES` | 5 | Mínimo dos gates |
| `RESAMPLES` | 2 000 | Reamostragens do bootstrap |
| `BYTES_PER_TOKEN_MILLI` | 3 631 | Rácio medido bytes/token (Q-01) |
| `MAX_DELTA_BYTES` | 8 192 | Teto do delta model-visible |
| `MAX_TAIL_BYTES` | 128 KiB | Teto da cauda entre snapshots (Q-15) |
| `SNAPSHOT_SCHEMA_VERSION` | 4 | Versão do snapshot |
| `LOG_SCHEMA_VERSION` | 1 | Versão do log |
| `PRIME_VERSION` | 5 | Versão do prime |
| `SELECTION_SCHEMA_VERSION` | 1 | Versão do esquema de seleção |
| `COMPACTION_SCHEMA_VERSION` | 1 | Versão da compactação |
| `STATE_SCHEMA_VERSION` | 1 | Versão da secção de estado |
| `MAX_SECTION_BYTES` | 512 | Teto da secção `estado` |
| `MAX_WORKING_SET` | 8 | Itens do *working set* |
| `AUDIT_SCHEMA_VERSION` | 1 | Versão da auditoria |
| `SEGMENT_EVENTS` | 256 | Eventos por segmento de auditoria |
| `MAX_TEXT_BYTES` | 512 | Teto do texto de um registo de auditoria |
| `GuardParams::DEFAULT` | k=500, h=2000, p₀=100, p₁=600, α=10, min_steps=3 | Loop guard |
| `SelectionParams::DEFAULT` | λ=250, sim=700, k=60, τ_JS=200 | Seleção |
| `Ledger::DEFAULT` | head/tail 2 KiB, spill 8 KiB | Truncagem de output |
| `VELOCITY_WINDOW_MS` | 60 000 | Velocidade financeira |
| `CALIBRATION_BINS` | 10 | Diagrama de fiabilidade |
| `MAX_HINT_CHARS` | 100 | Catálogo de skills |
| `SKILL_DIRS` | `.agents/skill`, `.agents/skills` | Descoberta |

---

## 17. Testes

### 17.1 Testes Unitários (por módulo)

| Módulo | Cobertura |
|--------|-----------|
| `state`/`step` | Transições de um passo, replan, recusas, pré-condições |
| `project` | `derive_messages` exclui controlo; `state_of` == replay manual; remédio aponta noutro sítio |
| `log` | Durabilidade, cauda rasgada, salto de `seq`, retomada incremental |
| `hash` | Vetores FNV-1a de referência; canónico ordena/estabiliza |
| `budget` | Teto `used > cap`; `check` não altera; `from_events` |
| `guard` | CUSUM/e-value, progresso reinicia, erro tipo I ≤ α |
| `confidence` | `rule_trials`/`tool_trials`/`enforced_verdicts` |
| `cost` | Camadas, kill switch, `Reenable`, reconstrução |
| `stats` | Percentis nearest-rank, MAD, IC, bootstrap |
| `context`/`select` | Unidades, submodular, MMR, RRF, JS |
| `audit` | Índice, Bloom, codec, store |
| `toon` | Emissão exata, SWAR, projeção, esquema |
| `evidence` | Artefacto obrigatório, `unpriced`, soma por base |
| `verify`/`plan` | Checks, cobertura, merge por menor privilégio |
| `taint` | 3 invariantes, red-team |
| `report` | Corte exato, ponteiro, content-id |

### 17.2 Testes de Integração

| Ficheiro | Cobertura |
|----------|-----------|
| `kernel/session/tests/*` | approval, context, control, cost, facts, invariants, replay, resume, workspace |
| `kernel/step/tests/*` | approval, phases, verification |
| `context/tests/*` | selection, rate-distortion |
| `memory/conformance` | `assert_contract` (suíte partilhada por backend) |
| `stats/tests/conformal_bench.rs` | C2 (rejeitado) |
| `bench/e18/*` | stats, tokens, select, loop, resume, durability, toon, taint, atoms |

---

## 18. Referências

- **MODULE.md:** [`crates/katu-core/MODULE.md`](../../../crates/katu-core/MODULE.md)
- **Políticas:** [`policy/`](../../../policy/)
- **Bench:** `bench/e18/` — `stats`, `tokens`, `select`, `loop`, `resume`, `durability`, `toon`,
  `taint`, `conformal`, `confidence`, `prompt`, `atomics`, `raw.json`
- **ADRs:** 0005 (TOON colunar), 0006 (catálogo), 0008 (snapshot), 0009 (auditoria), 0024
  (durabilidade), 0025 (saída estruturada)
