# `katu-core`

**Épico:** E04 · **Fase:** 2 (MVK) · **Crate puro** (kernel, sem providers).

O **kernel** do katu: a máquina de estados do agente (DF1). O estado é um valor, a transição é uma
função, o log é a fonte da verdade.

## Responsabilidade

- `State`, `Event`, `Refusal`, pipeline de tool call, log append-only.
  - `kernel::state` — `State`, `CallStatus`, `Refusal`/`RefusalReason`, `next_phase`/`can_transition`,
    `State::waivers` (exceções explícitas), `State::plan` (E06-T06), `State::last_command`
    (E06-T07), `State::workspace` (raiz do workspace, E07-T05) e `State::verification`
    (relatório do gate, E09-T03); `UnmetPrecondition` (E05-T02/T04).
  - `kernel::event` — `Event`, `CallId`, `Event::kind` (`Waiver`, `PlanRecorded`, `CommandRecorded`,
    `WorkspaceSet`, `VerificationRecorded`).
  - `kernel::step` — `step(State, Event) -> Result<State, Refusal>` (puro) + pré-condições de fase
    (`Verified` exige relatório não bloqueado, E09-T03).
  - `kernel::log` — `Log`/`LogRecord` append-only JSONL (`session.v1.jsonl`) sobre a porta `Fs`.
  - `kernel::project` — `derive_messages`, `state_of`, `snapshot` (projeções puras).
  - `kernel::pipeline` — `Tool`, `facts_for`/`facts_from`, `dispatch`/`dispatch_with` (facto →
    política → efeito).
  - `kernel::memory_gate` — `enforce_memory_write` (E05-T01): `pre_write` → capacidade → política.
  - `kernel::budget` — `Budget`/`BudgetCap`/`BudgetGate` (único dono do teto de contexto).
  - `kernel::cost` — `CostGovernor` (E09-T06): camadas `KillSwitch → PerTool → RollingWindow →
    FinancialVelocity → Global`, kill switch com `Reenable` assinado (o agente não assina) e
    reconstrução `from_events`; teto por ferramenta antes do global.
  - `kernel::bus` — `EventBus` (observadores + waterfall com a regra "tem de chamar `next`").
  - `kernel::checkpoint` — `Checkpoint` tipado (schema v1, validador zero-dep `validate`,
    `write_atomic`); erros agregados em `Issue { path, message }` (OA19/E09-T02).
  - `kernel::session` — `Session`/`CallContext` (loop mínimo: valida transição + orçamento antes de
    gravar; `tool_call` e `memory_write` pela ordem §42; `set_workspace`/`record_verification`;
    `verify`/`messages`/`fork`).
- `derive_messages`/`snapshot` — projeções puras.
- Porta [`memory::Memory`](src/memory.rs) (tipos do katu, DF6), com submódulos:
  - `memory::types` — `NoteType`, `Status`, `Basis`, `NoteRef`, `Anchor`, `Score` (pontos base,
    `0..=10_000`, determinístico — E18-T01);
  - `memory::io` — `PreWriteReq/Outcome`, `PreEditReq/Outcome`, `SessionEndReq/Outcome`,
    `MemoryStatus`, `Health`;
  - `memory::error` — `MemoryError`/`MemoryErrorKind` (`retryable()` só em `Timeout`);
  - `memory::fake` — `FakeMemory` (cenários fixos, sem puxar `knudge-core`).
- Modelo de erro [`error`](src/error.rs) (E01-T06) e ports determinísticos
  [`ports`](src/ports/mod.rs) (`Clock`/`Rng`/`Fs`/`Env`/`Process` + fakes; `Fs::write_atomic_if` =
  CAS para `edit`, OA16; `Process` = execução com timeout, E06-T04).
- Diagnóstico transversal [`diag`](src/diag/mod.rs) (DF9/E19): log estruturado + métrica de tempo,
  custo zero por defeito; catálogo de eventos em [`diag::events`](src/diag/events.rs); sink
  agregador de percentis em `diag::aggregate` (E19-T02).
- Contenção determinística [`containment`](src/containment.rs) (E07-T01): `Containment` (`Soft` por
  omissão), `SandboxEnforcement` (sempre `Soft` no MVP), `ContainmentStatus`/`announce`
  (`contain.mode`), `workspace_capabilities` (E07-T05: `Capability::Workspace { root }` — grant
  implícito da raiz, distinto do `ReadPath`/`WritePath` explícito) e o gancho
  `Jail`/`NoJail` (jail futura E17; `Full`/`Partial` ⇒ `Unavailable`).
- Evidência tipada [`evidence`](src/evidence.rs) (DF5/E09-T05): `Metric`/`EvidenceBasis`/
  `ArtifactRef`; um número sem artefacto não fundamenta decisão; a base não muda numa agregação.
- Contexto com orçamento [`context`](src/context.rs) (E09-T01): `ContextBudget`/`Context`/`assemble`
  (prime determinístico + sufixo de mensagens do log; `Model-visible ⟺ logged`) e `prime()`
  (`PRIME_VERSION = 1`); tokens por estimativa determinística (`bytes/4`).
- Gate de verificação [`verify`](src/verify/mod.rs) (E09-T03): `verify` **puro** (escopo/feedback/
  cobertura, zero LLM), `VerificationReport`/`Check`/`CheckStatus`, `--strict` promove warns a
  blocks; `Override` **assinado** (`reason`+`overridden_by`) registado em `overrides.jsonl`.
- Plano tipado [`plan`](src/plan.rs) (E06-T06/E09-T04): `Plan`/`ScopeContract`/`Feature`/
  `FeatureStatus`; `validate` (schema + "≤ 1 `in_progress`" + globs relativos), `allows` (globs;
  proibido vence) e `merge` por menor privilégio ([`plan/merge`](src/plan/merge.rs): `allowed`
  interseção, `forbidden` união, tempo mínimo, rede `AND`; fail-closed se disjuntos). O kernel
  exige um plano registado para `Phase::Planned`.
- Validação com erros que ensinam [`validate`](src/validate.rs) (OA19/E09-T02): `Issue { path,
  message }` + `Issues` agregado (ordem determinística), partilhado pelo validador de checkpoint e
  pelo linter de schema de tools (E06-T02).
- Feedback de comando [`feedback`](src/feedback.rs) (E06-T07): `CommandRecord`/`CommandStatus`,
  `tail` (cauda determinística) e `redact` (segredos); `exit_code: null` bloqueia avançar (§31).
- Formato AI-first [`toon`](src/toon.rs) (DF12/E06-T12): emissor **TOON** canónico (zero deps) para a
  saída das tools ao modelo — sem `null`, vazios omitidos, ordem canónica; JSON é a alternativa.
- Envelope [`report`](src/report.rs) (DF12/E06-T12): `ToolReport`/`Page`/`Cost`, ids
  content-addressed e hash; renderiza em TOON ou JSON. Transportado por `ToolOutput`.

## Fronteira

- Depende de `katu-policy`; **não** depende de `katu-tools`/`katu-providers`/`katu-tui` nem de
  `knudge-core` (o adaptador vive no binário, E03).
- Invariante: `Model-visible ⟺ logged`.
- Sem `unsafe`, sem `unwrap`/`expect`/`panic`.
