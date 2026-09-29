# `katu-core`

**Épico:** E04 · **Fase:** 2 (MVK) · **Crate puro** (kernel, sem providers).

O **kernel** do katu: a máquina de estados do agente (DF1). O estado é um valor, a transição é uma
função, o log é a fonte da verdade.

## Responsabilidade

- `State`, `Event`, `Refusal`, pipeline de tool call, log append-only.
  - `kernel::state` — `State`, `CallStatus`, `Refusal`/`RefusalReason`, `next_phase`/`can_transition`.
  - `kernel::event` — `Event`, `CallId`.
  - `kernel::step` — `step(State, Event) -> Result<State, Refusal>` (puro).
  - `kernel::log` — `Log`/`LogRecord` append-only JSONL (`session.v1.jsonl`) sobre a porta `Fs`.
  - `kernel::project` — `derive_messages`, `state_of`, `snapshot` (projeções puras).
  - `kernel::pipeline` — `Tool`, `facts_for`, `dispatch` (facto → política → efeito).
  - `kernel::budget` — `Budget`/`BudgetCap`/`BudgetGate` (único dono do teto de contexto).
  - `kernel::bus` — `EventBus` (observadores + waterfall com a regra "tem de chamar `next`").
  - `kernel::checkpoint` — `Checkpoint` tipado (schema v1, validador zero-dep, `write_atomic`).
  - `kernel::session` — `Session`/`CallContext` (loop mínimo: valida transição + orçamento antes de
    gravar, ordem §42, checkpoint de fase, `messages`/`verify`/`fork`).
- `derive_messages`/`snapshot` — projeções puras.
- Porta [`memory::Memory`](src/memory.rs) (tipos do katu, DF6), com submódulos:
  - `memory::types` — `NoteType`, `Status`, `Basis`, `NoteRef`, `Anchor`, `Score` (pontos base,
    `0..=10_000`, determinístico — E18-T01);
  - `memory::io` — `PreWriteReq/Outcome`, `PreEditReq/Outcome`, `SessionEndReq/Outcome`,
    `MemoryStatus`, `Health`;
  - `memory::error` — `MemoryError`/`MemoryErrorKind` (`retryable()` só em `Timeout`);
  - `memory::fake` — `FakeMemory` (cenários fixos, sem puxar `knudge-core`).
- Modelo de erro [`error`](src/error.rs) (E01-T06) e ports determinísticos
  [`ports`](src/ports/mod.rs) (`Clock`/`Rng`/`Fs`/`Env` + fakes; `Fs::write_atomic_if` = CAS
  para `edit`, OA16).
- Diagnóstico transversal [`diag`](src/diag/mod.rs) (DF9/E19): log estruturado + métrica de tempo,
  custo zero por defeito; catálogo de eventos em [`diag::events`](src/diag/events.rs).

## Fronteira

- Depende de `katu-policy`; **não** depende de `katu-tools`/`katu-providers`/`katu-tui` nem de
  `knudge-core` (o adaptador vive no binário, E03).
- Invariante: `Model-visible ⟺ logged`.
- Sem `unsafe`, sem `unwrap`/`expect`/`panic`.
