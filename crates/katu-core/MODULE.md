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
  - `kernel::event` — `Event`, `CallId`, `Visibility::{User,Agent}` (G3: `UserMessage` humana vs.
    *nudge* do loop), `Event::kind` (`Waiver`, `PlanRecorded`, `CommandRecorded`,
    `WorkspaceSet`, `ApprovalGranted`, `VerificationRecorded`).
  - `kernel::step` — `step(State, Event) -> Result<State, Refusal>` (puro) + pré-condições de fase
    (`Verified` exige relatório não bloqueado, E09-T03).
  - `kernel::log` — `Log`/`LogRecord` append-only JSONL (`session.v1.jsonl`) sobre a porta `Fs`.
    `Durability::{Event,Turn}` (ADR 0024/P-01): barreira por turno (*group commit*, `behavior.durability = turn`, **default**) ou por evento (`event`) — **−78,1 %** no caminho de anexação
    ([`bench/e18/durability`](../../bench/e18/durability/PROTOCOL.md)). Uma **cauda rasgada** (última
    linha sem `\n`, marca de crash) é descartada com aviso; corrupção a meio e saltos de `seq`
    continuam falha-fechado.
  - `kernel::hash` — `fnv1a`/`canonical`: hash determinístico de 64 bits, um só sítio (assinatura de
    chamada no guard Q-12 e integridade do estado no snapshot Q-15).
  - `kernel::session::snapshot` (ADR 0008, Q-15) — `StateSnapshot` com `seq`/`offset`/estado/uso e o
    **hash canónico** do estado (um snapshot que não casa é descartado: replay total, fail-closed).
    Duas fronteiras: `PhaseTransition` e fim de turno com cauda ≥ `MAX_TAIL_BYTES` (128 KiB). O que a
    retomada relê tem teto: 20 000 turnos retomam em **321 µs** contra 23 839 µs de replay total
    ([`bench/e18/resume`](../../bench/e18/resume/PROTOCOL.md)).
  - `kernel::project` — `derive_messages`, `state_of`, `snapshot` (projeções puras); `normalize`
    (L-Q5/G6) pareia `ToolCall`↔`ToolResult`, descarta vazios e resultados fora de ordem e garante
    início em `User`; `wire_messages` (PX-WIRE) agrupa cada passo do assistente (texto + N calls)
    numa `WireMessage::Assistant` — a forma que o wire exige
    ([`PROVIDER_WIRE`](../../wiki/_ref/brainstorm/PROVIDER_WIRE.md)).
  - `State.pending` (Q-15) — o estado guarda **só** as chamadas pendentes: o efeito vive no log e o
    nome concluído em `completed_tools`. Guardar as concluídas fazia o replay ser quadrático (cada
    `step` clona o estado) e recusava `call_0` repetido entre turnos, que é legítimo quando o provider
    não manda id.
  - `kernel::pipeline` — `Tool`, `facts_for`/`facts_from`, `dispatch`/`dispatch_with` (facto →
    política → efeito).
  - `kernel::memory_gate` — `enforce_memory_write` (E05-T01): `pre_write` → capacidade → política.
  - `kernel::budget` — `Budget`/`BudgetCap`/`BudgetGate` (único dono do teto de contexto).
  - `kernel::cost` — `CostGovernor` (E09-T06): camadas `KillSwitch → PerTool → RollingWindow →
    FinancialVelocity → Global`, kill switch com `Reenable` assinado (o agente não assina) e
    reconstrução `from_events`; teto por ferramenta antes do global. Ligado ao `Session`
    (`open_with_cost`/`apply_at` com o relógio de `CallContext`).
  - `kernel::bus` — `EventBus` (observadores + waterfall com a regra "tem de chamar `next`").
  - `kernel::confidence` (Q-11/F6, C5) — `rule_trials`/`tool_trials`/`enforced_verdicts` (e
    `enforced_verdicts_report`, que traz o resumo do controlo FDR da família): a ponte entre
    o log e a estatística de `katu-policy`. O ensaio de uma regra é *recusou ⇒ não correu* (a
    chamada recusada não pode aparecer executada sob o mesmo `CallId`); por tool mede-se o contrato
    de conclusão (uma chamada que expira conta contra a tool). Só regras **declaradas** `Enforced`
    são verificadas: o TOML continua a ser a autoridade e o veredicto diz se o log a **sustenta**
    (`bench/e18/confidence/`; o comando `xtask policy:confidence` corre no `check`).
  - `kernel::guard` (Q-12/F7) — `Fingerprint`/`Call`/`Guard`: deteção de loop por assinatura
    (FNV-1a de nome + argumentos canónicos) com **CUSUM** (fração de repetição, média) e **e-value**
    (razão de verosimilhança `Λ_n`, martingale sob `H0`; corte `log(1/α)`, *anytime-valid* por
    Ville — C1/W8-3). Um passo com chamada **exclusiva** é progresso e reinicia o detector — um
    *polling* legítimo de `bash` não é cortado. Medido em
    [`bench/e18/loop`](../../bench/e18/loop/PROTOCOL.md): **0** falsos positivos em 200 turnos
    normais, alarme no **5.º** passo de um ciclo puro (teto de referência: 12) e erro tipo I sob
    parada opcional **≤ α** (DP exata, sem RNG).
  - `kernel::checkpoint` — `Checkpoint` tipado (schema v1, validador zero-dep `validate`,
    `write_atomic`); erros agregados em `Issue { path, message }` (OA19/E09-T02).
  - `kernel::control` — `Control`/`ControlState` (E12-T10): modelo/pensamento do **utilizador** no
    log (`Event::Control`, audit `kind=control`) e no estado (sobrevive a *resume*); validação pura
    com erro que **ensina** (`ControlError::ReasoningUnsupported`), contra `ModelCapabilities`
    derivadas do catálogo pela borda.
  - `kernel::session` — `Session`/`CallContext` (loop mínimo: valida transição + orçamento antes de
    gravar; `tool_call` e `memory_write` pela ordem §42; `set_workspace`/`record_verification`;
    `approve` (aprovação humana, E07-T05) + `revoke_approval` (aprovação **one-shot**, B-06:
    a capacidade é revogada depois de usada e a próxima escalação exige nova aprovação); `context` (contexto efetivo, E09-T01/T07);
    `verify`/`changed_files`/`recorded_commands` (factos do gate, E09-T03); `messages`/`fork`).
- `derive_messages`/`snapshot` — projeções puras.
- Porta [`memory::Memory`](src/memory.rs) (tipos do katu, DF6), com submódulos:
  - `memory::types` — `NoteType`, `Status`, `Basis`, `NoteRef`, `Anchor`, `Score` (pontos base,
    `0..=10_000`, determinístico — E18-T01);
  - `memory::io` — `PreWriteReq/Outcome`, `PreEditReq/Outcome`, `RecallReq`/`RecallHit`,
    `SessionEndReq/Outcome`, `MemoryStatus`, `Health`;
  - `memory::query` — `QueryReq`/`QueryResult`/`QueryMode`/`QueryOutcome`/`QueryHit`/
    `QueryFilter`/`TagCount`/`Suggestion`/`Cluster` (E20-T06: paridade `memo`/`kd`; a porta
    `Memory::query` tem default fail-closed `Unavailable`);
  - `memory::error` — `MemoryError`/`MemoryErrorKind` (`retryable()` só em `Timeout`);
  - `memory::fake` — `FakeMemory` (cenários fixos, sem puxar `knudge-core`);
  - `memory::conformance` — `assert_contract` (E03-T05): suíte partilhada por backend.
- Modelo de erro [`error`](../katu-policy/src/error.rs) (E01-T06) e ports determinísticos
  [`ports`](src/ports/mod.rs) (`Clock`/`Rng`/`Fs`/`Env`/`Process` + fakes; `Fs::write_atomic_if` =
  CAS para `edit`, OA16; `Fs::remove` = remoção permanente de ficheiro, nunca de diretórios,
  E10-T07; `Process` = execução com timeout (`run`) **e** com output incremental (`run_streaming`,
  P1/PI_GAINS); `Progress`/`NoProgress` = observador **efémero** do output de uma tool). `ToolOutcome::fix()` (B-03) devolve o
  remédio acionável — a negação ensina o modelo a corrigir-se.
- Protocolo do kernel [`api`](src/api/mod.rs) (KERNEL_SURFACE F0): a fronteira entre o kernel
  (autocontido, na sua thread) e as superfícies (CLI/TUI/futuros front-ends). `Command` (incluindo
  `Cancel`/`Approval`/`Steer`/`Continue`/`Shutdown`) e `Event` são `serde`-prontos e não carregam
  `Runtime`/`Session`/`Provider` (K2/K7); `KernelHandle`/`KernelBus`/`Flag` são o transporte em
  memória (fila de comandos limitada com `try_send`, drenagem sem bloquear). Os dados de
  apresentação viajam no protocolo: `Live`, `TrashEntry`, `LoginRequest`, `ApprovalRequest`. O
  resultado estruturado do turno viaja em `Event::Turn(TurnSummary)` (envelope de máquina das
  superfícies) e as falhas em `Event::Failure { kind, message }` (taxonomia `ErrorKind`), pelo que
  o CLI reconstrói o envelope sem possuir o `Runtime` (F2).
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
- Resumo estatístico [`stats`](src/stats.rs) (E18-T10/W7): `Summary { n, p50, p95, mean, ci95_low,
  ci95_high }` determinístico — IC 95 % normal para `n ≥ 30` e *bootstrap* com índices derivados do
  próprio `n` abaixo disso (sem RNG); o intervalo contém sempre a mediana e a média. É a fonte única
  do `diag`, do `measure_mvk` e dos gates `gate:render`/`gate:provider` (que comparam o **limite
  superior** ao orçamento).
- Contexto com orçamento [`context`](src/context.rs) (E09-T01/T07, S-01): `ContextBudget`/
  `Context`/`AssembleOptions`/`assemble_all` — **uma** derivação do log, **uma** partição em
  unidades e **um** teto por turno (antes derivava-se o log até três vezes). `prime()`
  (`PRIME_VERSION = 3`) vive em [`context/prime`](src/context/prime.rs); tokens por estimativa
  determinística com o rácio **medido** `BYTES_PER_TOKEN_MILLI` (Q-01; `bench/e18/tokens/`).
  [`context/select`](src/context/select.rs) (Q-02b/Q-03) é a máquina **partilhada** pelo corte cru e
  pelo digest: unidades (corrida maximal de tool ou mensagem isolada), estatística IDF do próprio
  conjunto, escolha greedy submodular com diversidade **MMR** e fusão **RRF**
  ([`context/select/greedy`](src/context/select/greedy.rs)), informação e divergência **JS**
  ([`context/select/info`](src/context/select/info.rs)). Política por omissão: `Suffix` (histórica,
  byte a byte); `Utility` mede-se em `bench/e18/select/` (**+1037,9 %** de `I_ret`/token, controlo
  negativo 0,0 %) mas só se adota com A/B de tarefa. A montagem **pina** a instrução corrente
  (`pin_unit`): a última `Message::User` nunca sai, mesmo quando o sufixo teria de a evictar —
  sem isto o modelo perde a tarefa a meio do turno.
- Compactação [`context/compact`](src/context/compact.rs) (E09-T07, Q-03): determinística e opt-in
  (`CompactionMode`, default `Disabled`); digest do prefixo + mapeamento original→substituto,
  `recover` pelo log, ganho como `Metric` `inferred`. Com a política de utilidade as linhas do digest
  são escolhidas pela mesma máquina e o gatilho `JS(prefixo ‖ sufixo) ≥ τ_JS` evita gastar o resumo
  quando o prefixo é redundante (`I_ret` **+338,0 %** no proxy). `Session::assemble(budget, options)`
  é o **único** ponto que monta contexto e compactação, consumido pelo loop.
- Estado do turno no prompt [`context/state`](src/context/state.rs) (Q-04): secção `estado` compacta e
  determinística (modo, regras que travam, teto de passos, *working set*), com teto de bytes e sem
  duplicados, acrescentada **no fim** do prime. Registada como `Event::PromptState` com o texto exato
  (`Model-visible ⟺ logged`); **ligada** por omissão (`behavior.prompt_state`; desligável pela
  config) — a adoção dos números publicados dependia de A/B de turnos de auto-correção, hoje
  decidida pelo dono. Estável **dentro** do turno, de propósito: um valor que mudasse a cada
  passo destruiria o cache de prefixo do provider.
- Gate de verificação [`verify`](src/verify/mod.rs) (E09-T03): `verify` **puro** (escopo/feedback/
  cobertura, zero LLM), `VerificationReport`/`Check`/`CheckStatus`, `--strict` promove warns a
  blocks; o `diff` são ficheiros **relativos à raiz** derivados do log (`changed_files`, só escritas
  com sucesso) e o `feedback` são os `CommandRecord`; `Override` **assinado**
  (`reason`+`overridden_by`) registado em `overrides.jsonl` (evento `verify.override`).
- Plano tipado [`plan`](src/plan.rs) (E06-T06/E09-T04): `Plan`/`ScopeContract`/`Feature`/
  `FeatureStatus`; `validate` (schema + "≤ 1 `in_progress`" + globs relativos), `allows` (globs;
  proibido vence) e `merge` por menor privilégio ([`plan/merge`](src/plan/merge.rs): `allowed`
  interseção, `forbidden` união, tempo mínimo, rede `AND`; fail-closed se disjuntos). O kernel
  exige um plano registado para `Phase::Planned`.
- Validação com erros que ensinam [`validate`](src/validate.rs) (OA19/E09-T02): `Issue { path,
  message }` + `Issues` agregado (ordem determinística), partilhado pelo validador de checkpoint e
  pelo linter de schema de tools (E06-T02).
- Feedback de comando [`feedback`](src/feedback.rs) (E06-T07): `CommandRecord`/`CommandStatus`,
  `Ledger` (head/tail + *spill* com ponteiro, B-04), `tail` (cauda determinística) e `redact`
  (segredos); `exit_code: null` bloqueia avançar (§31).
- Formato AI-first [`toon`](src/toon.rs) (DF12/E06-T12): emissor **TOON** canónico (zero deps) para a
  saída das tools ao modelo — sem `null`, vazios omitidos, ordem canónica; JSON é a alternativa.
  [`toon/colunar`](src/toon/colunar.rs) emite **numa só alocação**: `byte_len` calcula o tamanho exato
  (teste `the_reserved_capacity_is_exact`), a sanitização usa uma guarda **SWAR** de 8 bytes (o caso
  comum — sem delimitadores — é uma cópia) e os inteiros vão por `write!` (um `push_int` manual mediu
  **pior**: +7 %). [`toon/project`](src/toon/project.rs) **empresta** o payload: `Cell<'a>`/
  `Section<'a>` usam `Cow<'a, str>` e as listas aninhadas são `&'a [Value]` (a projeção antiga clonava
  cada string e cada lista). Medido em [`bench/e18/toon`](../../bench/e18/toon/PROTOCOL.md): `emit`
  **−62,0 %** em dev e **−33,3 %** em release (P-02).
- Envelope [`report`](src/report.rs) (DF12/E06-T12): `ToolReport`/`Page`/`Cost`, ids
  content-addressed e hash; renderiza em TOON ou JSON. Transportado por `ToolOutput`.
- Estatística [`stats`](src/stats.rs): resumo determinístico (média, percentis, MAD/IC robusto — C7).
- **C2 (conformal) não vive aqui**: foi medido e **rejeitado**, logo não há `stats::conformal` em
  produção. A fórmula está no bench que produziu o número
  ([`stats/tests/conformal_bench.rs`](src/stats/tests/conformal_bench.rs)) e a decisão em
  [`bench/e18/conformal`](../../bench/e18/conformal/PROTOCOL.md): com resíduos correlacionados a
  cobertura cai até 17 ‰ abaixo do nominal, e o log real não tem base (`n_cal ≥ 19` por regra).
  *Regra:* item rejeitado deixa o número, não uma API pública sem consumidores.
- *Taint*/*spotlighting* do output de tool [`taint`](src/taint.rs) (D1): o delta *model-visible*
  viaja entre `<katu:untrusted kind=… bytes=…>` e `</katu:untrusted>` — dado **não confiável**,
  nunca instrução. `escape` neutraliza (`<` → `[`) qualquer `<` que inicia `katu:`/`/katu:` **sem
  alterar o comprimento**, logo `MAX_DELTA_BYTES` (8 KiB) continua a ser um teto exato sobre o delta
  completo; `inspect` é o teste estrutural do envelope. O embrulho vive em `ToolReport::to_delta`
  (não no encoder) para manter `Model-visible ⟺ logged`. Medido em
  [`bench/e18/taint`](../../bench/e18/taint/PROTOCOL.md): **6/6 ataques bloqueados**, custo
  **71 B** por resultado (0,87 % de um delta de 8 KiB).
- Skills do projeto [`skill`](src/skill.rs) (E20-T13): `Skill` (nome/descrição/caminho),
  `discover`/`parse` (frontmatter `name`/`description`) e `catalog` (texto do prompt de sistema).
  Descoberta **fail-open**: sem `.agents/skill{,s}/` → lista vazia; sem descrição → não carregada.
  O catálogo leva **nome, primeira frase da descrição e caminho relativo à raiz** (Q-05: −73,9 %
  de bytes) e ordena por relevância face ao objetivo, sem omitir skills.
- Condensação do contexto de projeto [`prompt`](src/prompt.rs) (Q-19): `condense` tira a sintaxe
  redundante do markdown do router (`AGENTS.md`), sem perder texto nem alvos — **−29,1 %** de
  bytes. É **pura** (mesma entrada → mesmas bytes) e **sem cache**: uma passagem linear sobre
  ~1,6 KB não paga um `fs.write` com `sync_all` (25,8 ms, `bench/e18/raw.json`).
- Porta `Provider` [`provider`](src/provider.rs) (E12-T01/T02/T03/T10): `Provider::{models, dynamic_models,
  capabilities, model_for_tier}` (`dynamic_models` descobre o catálogo do endpoint, com default =
  catálogo estático; `model_for_tier` escolhe a classe do catálogo, E12-T03),
  `Thinking`/`Tier`/`ModelSpec`/`ModelCapabilities`, `TokenUsage`
  ([`provider/usage`](src/provider/usage.rs)) e `CollectSink`
  ([`provider/collect`](src/provider/collect.rs)). O núcleo compila com ou sem provider ligado
  (firewall LLM-free).

## Fronteira

- Depende de `katu-policy`; **não** depende de `katu-tools`/`katu-providers`/`katu-tui` nem de
  `knudge-core` (o adaptador vive no binário, E03).
- Invariante: `Model-visible ⟺ logged`.
- Sem `unsafe`, sem `unwrap`/`expect`/`panic`.
