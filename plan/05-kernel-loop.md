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
| `→ Implemented` | há um `Plan` com escopo (`allowed_files`/`forbidden_files`) |
| `→ Verified` | existe `verification_report` válido (gate determinístico, E09) |
| `→ Persisted` | `session_end` da memória devolveu `Ok` |
| `→ Closed` | tarefa sem `outcome` **é recusada** |

---

## Tarefas

### E04-T01 ☐ Estado e eventos tipados
- **Entregáveis:** `State`, `Event` (`TurnStart`, `UserMessage`, `ToolCall`, `ToolResult`,
  `AssistantMessage`, `PhaseTransition`, `TurnEnd`), `Refusal`.
- **Aceite:** `State` é `Clone`/`Eq`/`Serialize`; nenhum campo é `HashMap` sem ordem canônica.

### E04-T02 ☐ Log de sessão append-only (`session.vN.jsonl`)
- **Entregáveis:** writer atómico (`temp→fsync→rename`); geração versionada; migrações adjacentes
  `vN→vN+1`; regra "gerações publicadas nunca são renomeadas nem apagadas" (§42).
- **Aceite:** o log é a fonte da verdade; `derive_messages()` projeta o histórico do modelo;
  truncagem de ficheiro/roubo de lock é detetada e falha fechado.

### E04-T03 ☐ Projeções (`derive_messages`, `state_of`, `snapshot`)
- **Entregáveis:** funções puras que derivam o histórico visível, o estado e snapshots do log.
- **Aceite:** propriedade: `state_of(replay(events)) == state_at_end`; tentativas falhadas
  retidas **sem** acrescentar histórico (§42).

### E04-T04 ☐ **Gate do épico:** replay e refusals
- **Entregáveis:** teste que conduz o loop e reproduz o estado final a partir do log; testes de
  transição ilegal (ex.: `Closed` sem verificação).
- **Aceite (gate):** replay byte-a-byte; transição ilegal devolve `Refusal` tipado e não muda o
  estado; invariante `Model-visible ⟺ logged` verificada em runtime.

### E04-T05 ☐ Pipeline de tool call
- **Entregáveis:** ordem explícita (adaptada do §42):
  `tool/call` (logado antes de executar) → `policy.evaluate(Facts)` →
  `Allow|Deny|RequireApproval|NeedsHuman` → execução → `ToolOutcome` → `tool/result`.
- **Aceite:** um `Deny` significa que o efeito **não** ocorreu e o resultado é devolvido ao modelo
  como erro recuperável; teste prova a negação **pelo executor** (§51.9).

### E04-T06 ☐ Event bus mínimo
- **Entregáveis:** `emit` e `waterfall` (around-middleware com a regra explícita "tem de chamar
  `next()`"); sem contentor de DI geral (§45).
- **Aceite:** um listener que só observa e não chama `next()` falha o build/teste; exceções de
  callbacks são contidas no dispatcher (§43.5).

### E04-T07 ☐ Orçamento e checkpoint de fase
- **Entregáveis:** `Budget { turns, tool_calls, tokens, wall_clock }`; `BudgetGate` que recusa ao
  atingir o teto; checkpoint tipado no limite de fase; artefacto durável.
- **Aceite:** orçamento excedido = recusa, nunca "corta a evidência" (§29); o checkpoint valida
  contra schema; um único dono do teto de contexto (§51.8).

> **Prior art — skill [`rlm`](../.agents/skill/rlm/SKILL.md).** O `pi-rlm` demonstra guardas de
> recursão que valem para qualquer laço limitado: tetos (`maxDepth`/`maxNodes`/`maxBranching`/
> `concurrency`), deteção de ciclo por linhagem normalizada, degradação graciosa em níveis e
> estado event-sourced para replay/live. O katu **não** adota delegação recursiva a sub-LLMs
> (fora de G3 e de [`00b` §3](00b-objetivos.md)); reusa só o padrão de teto/paragem e de
> replay a partir do log (que já é E04-T02/T04).

### E04-T08 ☐ Loop e sessão
- **Entregáveis:** laço que consome eventos e aplica transições; retoma a partir do log.
- **Aceite:** um teste conduz o loop do início ao fim com um `FakeMemory` e um provider fake;
  fork/resume derivam do mesmo log.

---

## Definition of Done

- [ ] E04-T01…T08 concluídas.
- [ ] Replay determinístico e refusals verdes.
- [ ] `Model-visible ⟺ logged` verificada.
- [ ] `cargo xtask check` e job `msrv` verdes.

## Não-objetivos

- Nenhum provider real (E12), nenhuma tool de sistema (E06), nenhuma TUI (E10).
- O MVK de enforcement é E05, sobre este kernel.
