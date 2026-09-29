# E05 — MVK: enforcement do protocolo de memória ⭐

> **Fase 2 (MVK) — o gate de decisão do projeto.** É o slice vertical que prova (ou refuta) a tese
> em ~4 semanas. Tudo o resto do `/plan` é condicional ao resultado deste épico.
>
> **Decisões:** DF1, DF2, DF3, DF5, DF6. **Depende de:** E04.
> **Resultado possível:** **passa** (escalar para E06+) ou **para** (investir no knudge + integração
> com agente existente). Ver [`README.md`](README.md) §5.

---

## Por que este épico existe

O `maxima` provou o teto do substrato para o **caminho único genérico** (§48–§49), mas **não**
para o protocolo de memória — que é justamente a família de regras que um hook de `write`/`edit`
consegue interceptar. A honestidade exige testar a hipótese mais barata primeiro:

> **Antes de possuir o loop inteiro, prove que o enforcement da memória é expressável, testável
> pelo caminho real, e sentidamente melhor que `pi + knudge-mcp`.**

Se não for, o projeto para aqui. Isto evita repetir o erro do arags (§20): construir uma
plataforma antes de provar o seu motivo.

---

## Escopo do slice (deliberadamente mínimo)

```
katu (binário) ── katu-core (E04, porta Memory) ── katu-policy (E02) ── in-process knudge (E03)
                     │
                     └── 1 provider (fake ou o mais barato de integrar)
                     └── 2 tools: write (completa) e read (mínima)
```

**Fora do slice:** sandbox real, TUI, plugins, providers múltiplos, `knudge-core` in-process,
compressão de contexto. Nada disso entra antes do gate.

---

## As regras em teste (todas de E02-T07)

1. **Nada de escrever sem buscar.** `write` de nota é `Deny` se não houve `pre_write` na mesma
   fase, ou se o `pre_write` acusar duplicata `≥ 0.92`.
2. **Âncora obrigatória.** Nota sobre código sem `--anchor` é `Deny` (ou `RequireBefore`).
3. **Evidência para fechar tarefa.** `Close` sem `--outcome` é `Refusal`.
4. **Uma afirmação por nota.** `Deny` para nota multi-afirmação.

> **Fonte no `knudge-core` v0.5:** o `≥ 0.92` é o `merge_below` de `[dedup]` (`create_below`
> 0.75) e vive na config, não na política; `close_task` **exige** evidência e infere o `outcome`
> da severidade; `behavior.strict = true` promove avisos *soft* (âncora/slots/claims) a erro. O
> enforcement do katu (DF1) é *defense-in-depth* com o hook `pre_record` do knudge
> (`ports::HookRunner`) — a memória valida o `Draft`, o loop fiscaliza a transição. Ver
> [`knudge/wiki/integration/`](../knudge/wiki/integration/README.md).

---

## Tarefas

### E05-T01 ☑ Ferramenta `write` sujeita à política
- **Entregáveis:** tool `write` que produz `ToolUse` resolvido e passa por `evaluate` antes de
  qualquer efeito; `Deny` devolve `ToolOutcome::Denied{rule_id, evidence}`.
- **Estado:** `ToolOutcome` ganhou `Denied { rule_id, evidence }` e `Unavailable { control }`
  (contrato de E06 antecipado); `katu_core::kernel::memory_gate` (`enforce_memory_write`,
  `memory_write_use`, `MemoryWriteRequest`) liga `pre_write` → `Capability::Command{MemoryWrite}` →
  `dispatch_with`; `katu_tools::write::WriteNoteTool` é o executor. A porta `Memory` ganhou
  `record` (commit por nota, OA8) e o `FakeMemory` conta commits. Testes: duplicata →
  `Denied{mem-no-duplicate}` **sem** executor nem commit; sem recall → `Unavailable`; permitido →
  corre uma vez.
- **Aceite:** com `FakeMemory` a acusar duplicata, a escrita **não** altera o disco e o resultado
  volta ao modelo como erro recuperável com `rule_id`.

### E05-T02 ☑ Ferramenta `read` mínima e fase `KnowledgeConsulted`
- **Entregáveis:** `read` que permite a transição para `KnowledgeConsulted`; ou `waiver` explícito.
- **Estado:** `katu_tools::read::ReadTool` lê um `ResolvedPath` pela porta `Fs`; a pré-condição de
  `step` (`satisfies_precondition`) exige `Read` ou `MemoryRecall` concluído **ou** um
  `Event::Waiver { transition, reason }` (novo estado `State::waivers`). Transição recusada com
  `RefusalReason::UnmetPrecondition { to }`.
- **Aceite:** sem `read`/`waiver`, a transição é recusada; com ela, é permitida.

### E05-T03 ☑ Integração das regras de memória na `policy/memory.toml`
- **Entregáveis:** as 4 regras como `Enforced`, com exemplo negativo cada.
- **Estado:** as 5 regras do protocolo (recall→write, outcome→close, sem duplicata, âncora,
  single-claim) são `Enforced` com exemplo negativo; `Advisory` vazio. Cobertas pelo teste de
  E02-T07 (`memory_policy_is_all_enforced_and_advisory_free`, com `audit::is_clean()`).
- **Aceite:** todas constam na lista `Enforced`; `Advisory` do protocolo = vazio.

### E05-T04 ☑ Fechar tarefa exige `outcome`
- **Entregáveis:** pré-condição de transição `→ Closed` que consulta o `outcome`.
- **Estado:** `Event::PhaseTransition { to, outcome }`; `outcome` (evidência) é gravada no log
  (o próprio evento) e exigida quando `to == Closed`. `Session::memory_write` fecha o caminho do
  gate no loop (E04-T08 ◐), com `SessionError::MemoryWrite`.
- **Aceite:** `Close` sem `outcome` é `Refusal`; com `outcome`, passa e loga a evidência.

### E05-T05 ☑ **Teste pelo caminho real (o coração do gate)**
- **Objetivo:** não testar extractores puros; conduzir o **loop real** e assertar a negação.
- **Entregáveis:** teste de integração que inicia o binário/loop, injeta um guião de agente que
  tenta (a) gravar sem buscar, (b) gravar com duplicata, (c) fechar sem `outcome`; asserta
  `Denied`/`Refusal` com `rule_id` e evidência.
- **Estado:** `crates/katu/tests/mvk.rs` (borda: vê `katu-core` + `katu-tools`): conduz
  `Session::open` → turno → `memory_write` (gate real) e `apply(PhaseTransition)`. (a) sem recall →
  `Denied{mem-recall-before-write}` **sem** executor (0 commits); (b) duplicata ≥ 0,92 →
  `Denied{mem-no-duplicate}` sem commit; (c) fecho sem `outcome` →
  `Refusal::UnmetPrecondition{Closed}`. Cada teste tem `session.verify()` (invariante
  `Model-visible ⟺ logged`).
- **Nota de coerência:** a checklist de T07 dizia `Denied` para (a), mas o contrato **fechado** de
  E02 classificava "sem recall" como `RequireAfter` → `RequireApproval`. **Resolvido por DF11**
  (a `severity` decide; `critical` = muro) + DF10 (recusa acionável): "sem recall" é agora
  `Denied` com `rule_id == mem-recall-before-write`.
- **Aceite:** os 3 cenários ficam **vermelhos** se o enforcement for removido — a regressão é
  introduzida, vista vermelha e revertida, por regra (§51.9, "um guard só guarda se a regressão o
  falhar").
- **Evidência red/green (executada):** (a) regra #1 → `advisory` ⇒ `write_without_recall…`
  **FAILED**; (b) `capabilities_for(Reject)` → concede capacidade ⇒ `duplicate_write…` **FAILED**;
  (c) `Phase::Closed => true` ⇒ `close_without_outcome…` **FAILED**; (DF11) `severity_verdict` →
  sempre aprovação ⇒ `critical_require_after_denies` + `write_without_recall…` **FAILED**. Cada
  regressão foi revertida e a suíte voltou a verde (134 testes).

### E05-T06 ☑ Medição honesta do atrito
- **Objetivo:** decidir com números que citam o artefacto que os produziu (DF5).
- **Entregáveis:** um protocolo de medição com base de evidência tipada; casos **positivos,
  negativos e no-op** (não removíveis); comparação com `pi + knudge-mcp` no mesmo cenário;
  relatório com linhas vermelhas mantidas.
- **Estado:** protocolo + artefacto existem. `katu_core::evidence` (DF5) e o sink agregador
  (E19-T02) alimentam o harness `crates/katu/examples/measure_mvk.rs` (feature `profile`), que
  grava `bench/mvk/raw.json` (contadores + percentis, `os`/`arch`). `bench/mvk/REPORT.md` mostra o
  custo (`memory.write` p50 ≈ 9,9 µs) e a **linha vermelha** `mvk.cross_tool.gain_ratio`
  (`unpriced`, 0). A comparação com `pi + knudge-mcp` foi **dispensada por decisão do dono**
  ([ADR 0001](../docs/adr/0001-mvk-gate-aprovado.md)) e permanece visível como dívida.
- **Aceite:** nenhum número publicado sem artefacto commitado; a linha em que o katu **não** ganha
  permanece visível (§62). `xtask gate:bench` (E15-T02) trava o manifesto `bench/published.toml`.

### E05-T07 ☑ **Gate de decisão (a checklist)**
Executar a checklist de [`README.md`](README.md) §5. O épico só fecha com **todos**:

- [x] gravação sem busca → `Denied` com `rule_id` e evidência, pelo loop real (DF11);
- [x] duplicata ≥ 0.92 → `Denied`;
- [x] fecho sem `outcome` → `Refusal`;
- [x] a regressão inverte cada um dos três testes para vermelho;
- [x] 100% das regras do protocolo são `Enforced` (nenhuma `Advisory`);
- [x] o atrito medido está no relatório, com negativos; a comparação com `pi + knudge-mcp` foi
  **dispensada por decisão explícita do dono** ([ADR 0001](../docs/adr/0001-mvk-gate-aprovado.md)),
  registada como `unpriced` — não inventada (DF5).

**Decisão: passa.** O loop possuído (DF1) vira **compromisso**; Onda 4 (E06/E07) desbloqueada.

**Interpretação:**
- **Passa** → escalar para E06 (tools/capacidades), E07 (sandbox) e E09 (contexto/evidência).
- **Falha** → **parar**. Não construir `pi-rs`, não construir as fases 3–7. Redirecionar para
  melhorar o knudge (hooks `pre-record`, mensagens que ensinam) e a sua integração com o agente
  existente. Registar um postmortem (DF7) do que se aprendeu e porquê.

---

## Definition of Done

- [x] E05-T01…T07 concluídas.
- [x] Os 3 testes do caminho real verdes **e** sensíveis à regressão.
- [x] Relatório de medição com bases tipadas e linha negativa visível.
- [x] `cargo xtask check` e job `msrv` verdes.
- [x] Decisão **passa/para** registada como ADR com `## Alternatives considered`
  ([`docs/adr/0001`](../docs/adr/0001-mvk-gate-aprovado.md)).

## Não-objetivos

- Provar que o katu é "melhor agente" — o slice prova **uma** propriedade: o protocolo de memória
  é fiscalizado em runtime.
- Performance de produção, UX, amplitude.
