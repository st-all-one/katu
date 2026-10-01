# E04 — Kernel: loop possuído, estado e log

> **Fase 2 (MVK).** A máquina de estados do agente (DF1). O estado é um valor, a transição é uma
> função, o log é a fonte da verdade, e toda tool call passa pela política **antes** de existir
> efeito.
>
> **Decisões:** DF1, DF2. **Depende de:** E02, E03.
> **Gate do épico:** replay determinístico + transições ilegais recusadas.

---

## Princípios não negociáveis

1. **Estado como valor.** `Step(State, Event) -> Result<State, Refusal>`. Sem singletons de
   processo (a cicatriz do maxima, §49.3).
2. **Model-visible ⟺ logged** (§42): tudo o que chega ao modelo tem de ser reconstruível a partir
   do log de sessão. Uma invariante de runtime verifica-o.
3. **Toda tool call passa pela política antes do efeito**, e é logada **antes** de executar (§42).
4. **Checkpoint no limite de uma fase**, não por limiar heurístico de NLU (§51.8).

---

## A máquina de estados

```
Task ──► KnowledgeConsulted ──► Planned ──► Implemented ──► Verified ──► Persisted ──► Closed
  ▲                                                                                     │
  └──────────────────────────── refusal / replan ◄──────────────────────────────────────┘
```

Pré-condições verificáveis (a imposição, não a prosa — §51.2):

| Transição | Pré-condição |
|---|---|
| `Task → KnowledgeConsulted` | existe `knowledge_query_id` **ou** um `waiver` explícito |
| `→ Planned` | existe um `Plan` registado (`PlanRecorded`, E06-T06) |
| `→ Implemented` | há um `Plan` com escopo (`allowed_files`/`forbidden_files`) |
| qualquer avanço | o último comando não é ambíguo (`exit_code: null` ⇒ recusa, §31) |
| `→ Verified` | existe `verification_report` válido (gate determinístico, E09) |
| `→ Persisted` | `session_end` da memória devolveu `Ok` |
| `→ Closed` | tarefa sem `outcome` **é recusada** |

> **Imposição (E04/E09).** `→ KnowledgeConsulted`, `→ Planned`, `→ Verified` e `→ Closed` são
> impostas pelo `step` (`State::completed_tools`/`State::plan`/`State::verification`/`outcome`);
> a ambiguidade do último comando (§31) bloqueia **qualquer** avanço. `→ Persisted` (via
> `session_end`) fica para o adaptador de memória (E03).

---

## Tarefas

### E04-T01 ☑ Estado e eventos tipados
- **Entregáveis:** `State`, `Event` (`TurnStart`, `UserMessage`, `ToolCall`, `ToolResult`,
  `AssistantMessage`, `PhaseTransition`, `Waiver`, `PlanRecorded`, `CommandRecorded`,
  `WorkspaceSet`, `TurnEnd`), `Refusal`.
- **Estado:** `kernel/{state,event,step}.rs`; `State` usa `BTreeMap`/`BTreeSet`; a transição
  `step(&State, &Event)` é pura; a forma do caminho único está em `next_phase`/`can_transition`.
- **Aceite:** `State` é `Clone`/`Eq`/`Serialize`; nenhum campo é `HashMap` sem ordem canônica.

### E04-T02 ☑ Log de sessão append-only (`session.vN.jsonl`)
- **Entregáveis:** writer atómico (`temp→fsync→rename`); geração versionada; migrações adjacentes
  `vN→vN+1`; regra "gerações publicadas nunca são renomeadas nem apagadas" (§42).
- **Estado:** `kernel/log.rs` sobre a porta `Fs` (novo `Fs::append`, com `fsync` no adaptador real):
  `session.v1.jsonl`, `LogRecord { seq, event }`, `seq` contíguo desde 1. O **snapshot** usa
  `write_atomic`; a migração `vN→vN+1` fica para quando o esquema mudar.
- **Aceite:** o log é a fonte da verdade; `derive_messages()` projeta o histórico do modelo;
  truncagem/roubo de lock é detetada (salto de `seq`/linha ilegível → `LogError`, fail-closed).

### E04-T03 ☑ Projeções (`derive_messages`, `state_of`, `snapshot`)
- **Entregáveis:** funções puras que derivam o histórico visível, o estado e snapshots do log.
- **Estado:** `kernel/project.rs`; os eventos de controlo (`TurnStart`/`TurnEnd`/`PhaseTransition`)
  **não** entram no histórico do modelo.
- **Aceite:** propriedade: `state_of(replay(events)) == state_at_end`; tentativas falhadas
  retidas **sem** acrescentar histórico (§42).

### E04-T04 ☑ **Gate do épico:** replay e refusals
- **Entregáveis:** teste que conduz o loop e reproduz o estado final a partir do log; testes de
  transição ilegal (ex.: `Closed` sem verificação).
- **Estado:** replay **byte-a-byte** do log (`replay_from_log_is_byte_stable`); transição ilegal
  devolve `Refusal` tipado e **não** muda o estado; `Session::verify` compara o estado corrente com
  a projeção do log e `Session::messages` deriva o histórico visível **só** do log (invariante
  `Model-visible ⟺ logged` verificada em runtime no teste do loop completo).
- **Aceite (gate):** replay byte-a-byte; transição ilegal devolve `Refusal` tipado e não muda o
  estado; invariante `Model-visible ⟺ logged` verificada em runtime.

### E04-T05 ☑ Pipeline de tool call
- **Entregáveis:** ordem explícita (adaptada do §42):
  `tool/call` (logado antes de executar) → `policy.evaluate(Facts)` →
  `Allow|Deny|RequireApproval|NeedsHuman` → execução → `ToolOutcome` → `tool/result`.
- **Estado:** `kernel/pipeline.rs` — `Tool` (trait), `facts_for`, `dispatch`; span
  `policy.evaluate` no caminho. `State` passou a transportar `capabilities`/`budget` (alimentam
  `Facts`).
- **Aceite:** um `Deny` significa que o efeito **não** ocorreu (a `Tool` **não** é invocada —
  contador a zero no teste) e o resultado volta como `ToolOutcome::Denied`; vocabulário inválido
  falha fechado sem efeito (§51.9).

### E04-T06 ☑ Event bus mínimo
- **Entregáveis:** `emit` e `waterfall` (around-middleware com a regra explícita "tem de chamar
  `next()`"); sem contentor de DI geral (§45).
- **Estado:** `kernel/bus.rs` — `EventBus` (observadores + cadeia de middleware + terminal),
  `Observer` (infalível: é assim que as exceções ficam contidas) e `Middleware`/`Next`;
  `HandlerError::{SkippedNext, CalledTwice, Failed}`. O bus deteta um middleware que devolve `Ok`
  sem chamar `next` (observador a fingir-se de middleware) e um `next` chamado duas vezes; um
  curto-circuito legítimo é `Err(..)` explícito.
- **Aceite:** um listener que só observa e não chama `next()` falha o build/teste; exceções de
  callbacks são contidas no dispatcher (§43.5).

### E04-T07 ☑ Orçamento e checkpoint de fase
- **Entregáveis:** `Budget { turns, tool_calls, tokens, wall_clock }`; `BudgetGate` que recusa ao
  atingir o teto; checkpoint tipado no limite de fase; artefacto durável.
- **Estado:** `kernel/budget.rs` — `Budget`/`BudgetCap`/`Charge`/`BudgetGate`; o uso é reconstruído
  do log (`Budget::from_events`) e o `Session` verifica-o **antes** de gravar (recusa = estado e log
  inalterados, sem cortar uso). `kernel/checkpoint.rs` — `Checkpoint` (schema v1,
  `deny_unknown_fields`), validador zero-dep e `write_atomic`; o `Session` expõe
  `write_checkpoint`/`read_checkpoint`. O `BudgetGate` é o **único dono do teto de contexto**
  (§51.8).
- **Aceite:** orçamento excedido = recusa, nunca "corta a evidência" (§29); o checkpoint valida
  contra schema; um único dono do teto de contexto (§51.8).

> **Prior art — skill [`rlm`](../../../.agents/skill/git-daily/SKILL.md).** O `pi-rlm` demonstra guardas de
> recursão que valem para qualquer laço limitado: tetos (`maxDepth`/`maxNodes`/`maxBranching`/
> `concurrency`), deteção de ciclo por linhagem normalizada, degradação graciosa em níveis e
> estado event-sourced para replay/live. O katu **não** adota delegação recursiva a sub-LLMs
> (fora de G3 e de [`00b` §3](00b-objetivos.md)); reusa só o padrão de teto/paragem e de
> replay a partir do log (que já é E04-T02/T04).

### E04-T08 ☑ Loop e sessão
- **Entregáveis:** laço que consome eventos e aplica transições; retoma a partir do log.
- **Estado:** `kernel/session.rs` — `Session` abre/replaya o log, valida transição + orçamento
  **antes** de gravar, executa a ordem §42 (`tool_call`) e expõe `messages`/`verify`/`fork`.
  Testes: negação logada sem efeito; reabertura retoma o estado; recusa não muda estado nem log;
  **loop completo** até `Closed` com `verify()` verde; fork e resume derivam do mesmo prefixo de
  log. O laço fecha com o `FakeMemory`: `crates/katu/tests/loop.rs` conduz `Task → Closed` com
  recall → write → close (gate E05) e `session.verify()`. O **trait de provider fake** fica para
  E12-T05.
- **Aceite:** um teste conduz o loop do início ao fim com um `FakeMemory` e um guião determinístico
  (provider fake); fork/resume derivam do mesmo log.

---

## Definition of Done

- [x] E04-T01…T08 concluídas. *(T08 ☑: `FakeMemory` no laço; o trait de provider fake é E12-T05)*
- [x] Replay determinístico e refusals verdes.
- [x] `Model-visible ⟺ logged` verificada.
- [x] `cargo xtask check` e job `msrv` verdes.

## Não-objetivos

- Nenhum provider real (E12), nenhuma tool de sistema (E06), nenhuma TUI (E10).
- O MVK de enforcement é E05, sobre este kernel.
