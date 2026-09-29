# `katu-core`

**Épico:** E04 · **Fase:** 2 (MVK) · **Crate puro** (kernel, sem providers).

O **kernel** do katu: a máquina de estados do agente (DF1). O estado é um valor, a transição é uma
função, o log é a fonte da verdade.

## Responsabilidade

- `State`, `Event`, `Refusal`, pipeline de tool call, log append-only.
  - `kernel::state` — `State`, `CallStatus`, `Refusal`/`RefusalReason`, `next_phase`/`can_transition`,
    `State::waivers` (exceções explícitas); `UnmetPrecondition` (E05-T02/T04).
  - `kernel::event` — `Event`, `CallId`, `Event::kind`.
  - `kernel::step` — `step(State, Event) -> Result<State, Refusal>` (puro) + pré-condições de fase.
  - `kernel::log` — `Log`/`LogRecord` append-only JSONL (`session.v1.jsonl`) sobre a porta `Fs`.
  - `kernel::project` — `derive_messages`, `state_of`, `snapshot` (projeções puras).
  - `kernel::pipeline` — `Tool`, `facts_for`/`facts_from`, `dispatch`/`dispatch_with` (facto →
    política → efeito).
  - `kernel::memory_gate` — `enforce_memory_write` (E05-T01): `pre_write` → capacidade → política.
  - `kernel::budget` — `Budget`/`BudgetCap`/`BudgetGate` (único dono do teto de contexto).
  - `kernel::bus` — `EventBus` (observadores + waterfall com a regra "tem de chamar `next`").
  - `kernel::checkpoint` — `Checkpoint` tipado (schema v1, validador zero-dep, `write_atomic`).
  - `kernel::session` — `Session`/`CallContext` (loop mínimo: valida transição + orçamento antes de
    gravar; `tool_call` e `memory_write` pela ordem §42; `verify`/`messages`/`fork`).
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
  custo zero por defeito; catálogo de eventos em [`diag::events`](src/diag/events.rs); sink
  agregador de percentis em `diag::aggregate` (E19-T02).
- Evidência tipada [`evidence`](src/evidence.rs) (DF5/E09-T05): `Metric`/`EvidenceBasis`/
  `ArtifactRef`; um número sem artefacto não fundamenta decisão; a base não muda numa agregação.
- Formato AI-first [`toon`](src/toon.rs) (DF12/E06-T12): emissor **TOON** canónico (zero deps) para a
  saída das tools ao modelo — sem `null`, vazios omitidos, ordem canónica; JSON é a alternativa.
- Envelope [`report`](src/report.rs) (DF12/E06-T12): `ToolReport`/`Page`/`Cost`, ids
  content-addressed e hash; renderiza em TOON ou JSON. Transportado por `ToolOutput`.

## Fronteira

- Depende de `katu-policy`; **não** depende de `katu-tools`/`katu-providers`/`katu-tui` nem de
  `knudge-core` (o adaptador vive no binário, E03).
- Invariante: `Model-visible ⟺ logged`.
- Sem `unsafe`, sem `unwrap`/`expect`/`panic`.
