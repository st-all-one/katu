# E09 — Contexto, checkpoint e evidência

> **Fase 5.** Como o contexto é montado com garantias, como o checkpoint é um artefacto tipado, e
> como a evidência e os números viajam com a sua base (DF5).
>
> **Decisões:** DF5, DF7. **Depende de:** E05.
> **Gate do épico:** nenhum número sem base tipada; checkpoint validado contra schema.
>
> **Matemática.** Seleção de contexto submodular (F2) e compactação por informação (F3) em
> [`19-otimizacao-profunda.md`](19-otimizacao-profunda.md) (E18).

---

## 1. Contexto com orçamento (§19, §22)

- **Só o delta chega ao modelo** (§18). Persistir artefatos e trafegar **referências** (ponteiro,
  não corpo — cf. `__sniff` do sniffCSS e os hints do knudge).
- **Mínimo para o cru, teto para o resumido** (orçamento de contexto com garantias).
- **Determinismo de prefixo** (§63): uma transformação de contexto guarda um mapeamento
  **determinístico original→substituto**, senão invalida o cache de prefixo do provider e custa
  mais do que poupa.
- **Um único dono do teto de contexto** (a cicatriz dos 6 donos da compactação no maxima, §49.4).
- **Reaproveitar o `rewind` do knudge?** O núcleo já monta o contexto de início de sessão com
  orçamento aproximado em tokens (`D40`) e `context_id` endereçável para retomar bytes idênticos
  (`D88`). Decidir explicitamente: (a) expor `rewind`/`context_id` pela porta `Memory` e **não**
  construir um segundo montador de contexto — evitando a dualidade de motores do arags (§20); ou
  (b) documentar por que o `assemble` do katu é distinto. Não deixar as duas implementações
  coexistirem por inércia.

## 2. Checkpoint como artefacto de fase (§50.1, §51.8)

O `.STAGING.md` do maxima, promovido de prosa a **campo tipado de estado**:

```json
{ "schema_version": 1, "goal": "…", "state": "…", "pending": ["…"],
  "next_action": "…", "findings": ["…"], "phase": "Implemented" }
```

Escrita atómica (`temp→fsync→rename`); validado por schema zero-dep.

## 3. Evidência tipada (DF5, §62)

```rust
pub enum EvidenceBasis {
    Measured, Inferred, ProviderReported, BenchmarkCounterfactual,
    Observed, Verified, Unpriced,
}
pub struct Metric { pub value: f64, pub basis: EvidenceBasis, pub artifact: Option<ArtifactRef> }
```

Regras travadas por teste: a base não muda numa agregação; `unpriced` ≠ zero; `verified` só com
método de verificação imposto e nomeado; negativos visíveis.

---

## Tarefas

### E09-T01 ☑ Montagem de contexto com orçamento
- **Entregáveis:** `ContextBudget { raw_min, summary_max }`; `assemble(state, budget) -> Context`.
- **Prime (DF12):** o contexto inclui um **prime compacto** que documenta o envelope das tools e a
  gramática **TOON** (default compacto; `--long` para a spec completa) — o modelo é *ensinado* a
  ler a saída; o prime é determinístico e versionado (`PRIME_VERSION`).
- **Estado:** `katu-core::context` implementa `ContextBudget`, `Context`, `assemble(events, budget)`
  (a assinatura usa os **eventos**, não `State`, porque o invariante é `Model-visible ⟺ logged`) e
  `prime()` (`PRIME_VERSION = 1`). A montagem é pura: projeta com `derive_messages` e mantém o
  **sufixo mais recente** que cabe em `raw_min`; a contagem de tokens é estimativa determinística
  (`bytes/4`, base `inferred`). Emite o span `context.build`. O `summary` é preenchido pela
  compactação (E09-T07) e o prime tem variante `--long` (`PrimeMode::Long`/`assemble_with_prime`).
- **Aceite:** nenhuma mensagem sem origem no log (`Model-visible ⟺ logged`); o orçamento é
  respeitado; teste com limite exato e limite+1; o prime aparece uma única vez e é estável.

### E09-T02 ☑ Checkpoint tipado
- **Entregáveis:** tipo `Checkpoint`, schema, validador zero-dep, escrita atómica.
- **Erros (OA19):** o validador devolve `Issue { path, message }` agregado (não `String`),
  reusando o tipo de erro de E06-T02; o percurso aponta o campo exato que falhou.
- **Estado:** `kernel::checkpoint` traz o `Checkpoint` (schema v1, `write_atomic`) e o validador
  zero-dep `validate`; a partir de E09-T02 o erro agregado vive em `katu-core::validate`
  (`Issue { path, message }` + `Issues`), **partilhado** com o linter de schema (E06-T02). O
  validador agrega **todos** os problemas com o caminho exato (`goal`, `pending`, `phase`,
  `surpresa`, …) em vez de parar no primeiro; a versão de esquema presente mas não suportada
  mantém a recusa dedicada `SchemaVersion`. `CheckpointError::Invalid` passa a carregar `Issues`.
- **Aceite:** checkpoint corrompido falha a validação; escrita é atómica sob crash simulado.

### E09-T03 ◐ Gate de verificação determinístico
- **Entregáveis:** `verification_report.json` = função determinística sobre (regras, escopo,
  feedback, diff); zero LLM; um único caminho de relatório; `block` não sobreponível pelo agente —
  só por humano com `override_reason` + `overridden_by`; *coverage floor*; `--strict` promove
  warns a blocks (§31).
- **Estado:** `katu-core::verify` implementa `verify(input) -> VerificationReport` (puro, sem I/O,
  sem LLM): verificações de escopo (`scope.forbidden` bloqueia; `scope.allowed` avisa), feedback
  (`feedback.timeout`/`feedback.ambiguous` bloqueiam; `feedback.exit` avisa) e `coverage` (piso em
  pontos base, E18-T01); `--strict` promove `Warn`→`Block`; `Override::new` exige `reason` e
  `overridden_by` (o agente não assina) e `append_override` regista em `overrides.jsonl`
  (append-only); `save` grava o relatório atomicamente. Emite o span `verify.report`.
  O kernel exige agora um relatório **não bloqueado** para `→ Verified` (`State::verification`,
  `Event::VerificationRecorded`, `Session::record_verification`; teste
  `verified_requires_a_non_blocked_report`). **Falta:** o pedido interativo de override
  (CLI/TUI, E10).
- **Aceite:** o gate nunca chama um LLM; override é assinado e registado (`overrides.jsonl`).

### E09-T04 ◐ Scope contracts e `feature_list`
- **Entregáveis:** `scope_contract.json` com **globs** (não paths), `forbidden_files` obrigatório,
  acceptance, rollback, `time_budget_minutes`, `network_egress`; merge por menor privilégio
  (allowed = interseção; forbidden = união; tempo = mínimo); `feature_list.json` com ≤ 1
  `in_progress` verificado no startup.
- **Estado:** `ScopeContract` ganhou `time_budget_minutes` (`Option<u64>`, `None` = sem teto) e
  `network_egress` (`bool`, default `false`); `validate` rejeita globs não relativos (`/…`, `..`)
  e mantém `forbidden_files`/rollback obrigatórios. `ScopeContract::merge` (`plan/merge.rs`) faz o
  merge por **menor privilégio**: `allowed` = interseção glob-aware (cobertura conservadora:
  `src/**` cobre `src/parser/**`, `src/*.rs` cobre `src/main.rs`), `forbidden` = união, tempo =
  mínimo, rede = `AND`. Se ambos os `allowed` são não vazios e disjuntos, o merge **falha**
  (`MergeError::EmptyAllowedScope`, fail-closed) em vez de conceder "tudo". Emite o span
  `scope.merge`. O `≤ 1 in_progress` é imposto por `Plan::validate` (E06-T06).
- **Falta:** nada — o carregamento de `scope_contract.json`/`feature_list.json` no arranque vive
  em `katu/src/scope.rs` (E09-T04): valida o plano com `Plan::validate` **antes** do turno
  (fail-closed) e liga a tool `plan` ao kernel (`PlanRecorded` §42, `katu/src/agent/plan.rs`).
- **Aceite:** contrato sem `forbidden_files` ou sem rollback **não** é aprovado; merge testado
  (narrowing, união, mínimo, `AND`, conflito disjunto, contrato resultante válido).

### E09-T05 ◐ `Metric` e portão de publicação
- **Entregáveis:** tipo `Metric`; `xtask gate:bench` que exige artefacto por número; checklist de
  7 itens e portão (≥ 6 casos, ≥ 3 repetições, IC 95% todo acima de zero, negativos não
  removíveis) (§62).
- **Estado:** `Metric`/`EvidenceBasis`/`ArtifactRef` implementados em `katu_core::evidence` (DF5),
  com agregação que **não muda a base**; `xtask gate:bench` implementado e ligado ao CI. Falta o
  portão estatístico (IC 95 %/repetições) — E18-T10.
- **Aceite:** build falha se um valor publicado não tiver base; a linha negativa do benchmark
  permanece.

### E09-T06 ☑ Cost governor
- **Entregáveis:** camadas (`max_tokens`, orçamento por task, cap por ferramenta, `max_turns`,
  janelas rolantes, velocidade financeira, kill switch com re-enable separado).
- **Estado:** `katu_core::kernel::cost` implementa `CostGovernor` com as camadas avaliadas por
  precedência `KillSwitch → PerTool → RollingWindow → FinancialVelocity → Global` (o teto **por
  ferramenta** dispara **antes** do global). `CostCaps` agrega `global: BudgetCap`
  (tokens/turnos/chamadas/tempo = orçamento por task), `per_tool`, `rolling` e `velocity`;
  `CostCharge` traz `now_millis` (as camadas temporais só correm com relógio). O kill switch
  (`trip`) só reabre com `Reenable` (motivo + autor não vazios — o agente não assina).
  `from_events` reconstrói o uso global e por ferramenta; a recusa nunca altera o uso (§29).
  Ligado ao loop: `Session::open_with_cost`, `Session::cost` e `apply_at` passam o relógio de
  `CallContext` às camadas temporais (teste
  `per_tool_cap_fires_before_the_global_cap_in_the_loop`). Emite
  `cost.check`/`cost.refuse`/`cost.kill`/`cost.reenable`.
- **Aceite:** loop patológico cortado pelo teto por ferramenta **antes** do global; kill switch
  testado (engata → recusa; re-enable separado → reabre). O gatilho automático do kill switch
  (anomalia CUSUM/SPRT) é E18-T07.

---

### E09-T07 ◐ Compactação da conversa como controlo do core
- **Objetivos:** tornar "compactar conversa" (core §1.1 #10) operação de primeira classe — via
  comando do utilizador e/ou gatilho do kernel no limite de fase/orçamento excedido — **nunca**
  inline no hot path.
- **Entregáveis:** porta `katu-context` (`assemble`/`compact`) com **mapeamento determinístico
  original→substituto** (preserva o cache de prefixo do provider, §1 deste épico); um único dono
  do teto de contexto (evita a cicatriz dos 6 donos, §49.4); recuperação obrigatória (o original
  continua endereçável no log); `Metric` do ganho com base `provider_reported`/`inferred`.
- **Estado:** `katu_core::context` expõe `compact(events, budget, mode) -> Option<Compaction>`
  (determinístico, sem LLM): o prefixo que não cabe em `raw_min` é substituído por um **digest**
  (linha por mensagem: tipo + id de conteúdo + excerto) limitado a `summary_max`; o mapeamento
  original→substituto (`content_id` FNV-1a) é estável para o mesmo input (cache de prefixo, §1).
  `CompactionMode::Disabled` (default) devolve `None` — nada silencioso no caminho built-in.
  `recover(events, id)` devolve a mensagem original do log (recuperação obrigatória). O ganho é um
  `Metric` com base `inferred` (E09-T05). Um único dono do teto (`ContextBudget`). Emite
  `context.compact`. Gatilho do kernel: `needs_compaction(events, budget)` e
  `Session::compact_context`.
- **Falta:** comando do utilizador (CLI/TUI, E10).
- **Aceite:** compactar não perde nenhuma mensagem recuperável; determinístico para o mesmo input;
  desligar mantém o `assemble`; nenhuma compactação silenciosa no caminho built-in (E12).

## Definition of Done

- [ ] E09-T01…T07 concluídas.
- [ ] Checkpoint, gate e métricas validados por schema/zero-dep.
- [ ] Nenhum número sem base e artefacto.
- [ ] `cargo xtask check` e job `msrv` verdes.

## Não-objetivos

- Compressão **inline no hot path**: a compactação é capacidade do core (§1.1 #10), mas é **porta**
  (`katu-context`) acionada off hot path, com recuperação obrigatória (E15/incremental).
- `durable execution` completo: começar com snapshot atómico; event log durável só quando a
  retomada multi-sessão for real (§34, tensões).
