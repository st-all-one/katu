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
- `derive_messages`/`snapshot` — projeções puras.
- Porta [`memory::Memory`](src/memory.rs) (tipos do katu, DF6), com submódulos:
  - `memory::types` — `NoteType`, `Status`, `Basis`, `NoteRef`, `Anchor`, `Score` (pontos base,
    `0..=10_000`, determinístico — E18-T01);
  - `memory::io` — `PreWriteReq/Outcome`, `PreEditReq/Outcome`, `SessionEndReq/Outcome`,
    `MemoryStatus`, `Health`;
  - `memory::error` — `MemoryError`/`MemoryErrorKind` (`retryable()` só em `Timeout`);
  - `memory::fake` — `FakeMemory` (cenários fixos, sem puxar `knudge-core`).
- Modelo de erro [`error`](src/error.rs) (E01-T06) e ports determinísticos
  [`ports`](src/ports/mod.rs) (`Clock`/`Rng`/`Fs`/`Env` + fakes).
- Diagnóstico transversal [`diag`](src/diag/mod.rs) (DF9/E19): log estruturado + métrica de tempo,
  custo zero por defeito; catálogo de eventos em [`diag::events`](src/diag/events.rs).

## Fronteira

- Depende de `katu-policy`; **não** depende de `katu-tools`/`katu-providers`/`katu-tui` nem de
  `knudge-core` (o adaptador vive no binário, E03).
- Invariante: `Model-visible ⟺ logged`.
- Sem `unsafe`, sem `unwrap`/`expect`/`panic`.
