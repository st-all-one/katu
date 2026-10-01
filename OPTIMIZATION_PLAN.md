# OPTIMIZATION_PLAN — qualidade da execução do modelo, performance e simplificação

> **Plano de otimização** do katu, derivado de três fontes: o método do E18
> ([`plan/19`](plan/19-otimizacao-profunda.md)), a bateria do knudge
> ([`19_performance_reforma_cli.md`](crates/knudge/plan/implementation/19_performance_reforma_cli.md),
> [`otimizacoes_performance.md`](crates/knudge/plan/proposals/otimizacoes_performance.md)) e os
> princípios do [`.agents/skill/rust/`](.agents/skill/rust/SKILL.md).
>
> **Evidência de partida** (nenhum número aqui é inventado):
> [`bench/e18/raw.json`](bench/e18/raw.json) (turno), [`bench/e18/atomics/`](bench/e18/atomics/REPORT.md)
> (tempo atómico por função, cobertura 99,4 %), [`bench/mvk/raw.json`](bench/mvk/raw.json) (release),
> [`bench/published.toml`](bench/published.toml) (DF5).
>
> **Autoridade:** os objetivos de [`plan/00b`](plan/00b-objetivos.md) (G1–G9) vencem. Este plano
> **não** abre superfície nova (G7); reduz turnos, ruído e custo.

---

## 0. Tese, hierarquia e método

O katu não é medido por ser "rápido" — é medido por **executar bem o modelo**: dar-lhe o **dado
certo**, uma **superfície de tools que não gasta turnos**, **transparência** do que aconteceu,
**guardrails** que travam o que deve, e **estabilidade** sob falha. Performance bruta é o segundo
eixo; simplificação, o terceiro.

**Hierarquia (em caso de conflito, o de cima vence):**

1. **Q — qualidade da execução do modelo** (dados · superfície · transparência · segurança ·
   guardrails · estabilidade). Cada turno poupado vale mais que qualquer micro-ganho.
2. **P — performance bruta** (latência, fsync, CPU, alocação), sempre com artefacto.
3. **S — simplificação** (menos código, menos superfície, um facto um lar), que serve Q e P.

**Método (herdado do E18 §0.3 e do knudge E15):** cada item exige **fórmula + artefacto +
teste que o trava**; **A/B** com recorte cru; **adotar-ou-reverter** (≥ 20 % no alvo **ou**
remoção de complexidade; senão reverter e escrever a rejeição). Nada muda bytes observáveis
(`Model-visible ⟺ logged`, E04). Determinismo: sem RNG, ordem canônica, `total_cmp`, `clamp01`.

**Restrições inflexíveis:** zero `unwrap/expect/panic`, um ponto de `unsafe` (ADR 0018), ficheiros
`src/` ≤ 300 linhas, `make check` + `msrv` verdes, ids de `diag` no catálogo
([`events.rs`](crates/katu-core/src/diag/events.rs)), superfície só cresce subindo o teto em PR.

---

## 1. Diagnóstico (o que o baseline e a auditoria dizem)

### 1.1 O turno é dominado por dois custos

| Camada | p50 medido | Fonte |
|---|---|---|
| Prompt frio (3265 tokens, `cached=0`) | **~40 s** | `bench/e18/REPORT.md` |
| Prompt quente (`cached≈3254`) | ~0,9 s (llama) | idem |
| `provider.request` total | 20,2 s (dev, frio) | `bench/e18/atomics` §3 |
| **Overhead fora do provider** | **72,3 ms/turno** | `bench/e18/raw.json` |
| ↳ `log.append` (5×, fsync) | 18,3 ms | idem |
| ↳ `fs.write` (7×, `sync_all`+rename) | 25,8 ms | idem |
| ↳ `session.open` | 8,5 ms | idem |

**Leitura:** o provider domina (e é limitado pelo **tamanho do prompt** e pela ausência de cache a
frio); o que o katu controla é (a) o que mete no prompt, (b) os ~72 ms de persistência por turno.

### 1.2 Onde o tempo atómico se gasta (MVK, dev)

`memory.write` 347,7 µs → `memory_gate::enforce_memory_write` 221,9 µs →
`pipeline::dispatch_with` 205,1 µs → `with_estimated_cost` 157,0 µs →
`report::to_toon` 153,7 µs → `toon::colunar::emit` 89,1 µs → `session.open` 64,9 µs.
Em **release** (`bench/mvk`): `kernel.transition` 14,4 µs, `memory.write` 9,9 µs, `log.append`
6,6 µs, `policy.evaluate` 2,4 µs.

**Leitura:** o caminho de **emissão TOON** (`report::to_toon` + `colunar::emit` + `project` ≈
270 µs dev) e o **gate de memória** são os maiores custos de CPU do caminho; a política é barata.

### 1.3 A instrumentação está essencialmente fechada

- Cobertura por função: **99,4 %** das instrumentáveis (**90,2 %** contando `const fn`).
- Catálogo: **114 ids**; só **3 nunca emitidos** (`memory.read`, `memory.compact`, `policy.audit`).
- **Lacuna de método:** a atribuição por função **não** sai no `stderr` (só no
  `AggregatingSink`); as secções por função são `dev` (com overhead do `diag`). Falta o harness
  (`criterion`/`hyperfine`/`dhat`) — E18-T10.

### 1.4 Brechas concretas de qualidade (verificadas no código)

| # | Brecha | Onde | Impacto |
|---|---|---|---|
| a | **Descrições de parâmetro não chegam ao modelo** — `def`/`property` ignoram `param.description`; o JSON Schema só leva `type`/`enum`/`pattern` | [`agent/catalog.rs`](crates/katu/src/agent/catalog.rs) | o modelo adivinha o sentido dos campos → turnos extra |
| b | **Orçamento de tokens é `bytes/4`**, não tokenizer; logo `fit_raw`/`compact`/`gain` ficam `inferred` | [`context.rs`](crates/katu-core/src/context.rs) | decisões de janela erradas; R13 |
| c | **Contexto = sufixo mais recente** (`fit_raw`), sem utilidade/MMR nem fusão de canais (F2/F9) | idem | perde-se o relevante por ser antigo |
| d | **Compactação = excerto de 48 B** por mensagem, sem surprisal/JS (F3) | [`context/compact.rs`](crates/katu-core/src/context/compact.rs) | retém o irrelevante, descarta o denso |
| e | **O modelo não vê o seu estado**: regras ativas, modo, orçamento restante, working set | `prime()` (estático) | não se auto-regula; G6/R13 |
| f | **`edit` é uma substituição por turno**; refactors = muitos turnos (o custo dominante) | [`katu-tools/edit.rs`](crates/katu-tools/src/edit.rs) | turnos × N |
| g | **Negação/erro não ensina a corrigir** (dá `rule_id`+evidência, não "o que passaria") | [`error/mod.rs`](crates/katu-core/src/error/mod.rs) | novas tentativas cegas |
| h | **Sem deteção de loop/anomalia** (F7): só o kill switch global | [`kernel/cost`](crates/katu-core/src/kernel/cost/mod.rs) | queima orçamento devagar |
| i | **`Enforced` não tem confiança medida** (F6) | [`rule.rs`](crates/katu-policy/src/rule.rs) | regra afirmada, não provada |
| j | **A frio, o 1.º turno local custa ~40 s** | — | o pior cliff de UX, na rota crítica |
| k | **Sem hedging/backpressure** no transporte (F4): TTFT remoto p95 4,1 s | [`wire.rs`](crates/katu-providers/src/wire.rs) | cauda lenta e imprevisível |

---

## 2. Eixo Q — qualidade da execução do modelo (prioridade 1)

### Q-A. Dados que o modelo dispõe (contexto)

#### Q-01 · Orçamento de tokens exato e calibrado (F2, pré-requisito)
- **Problema:** `tokens_from_bytes = bytes/4` decide `fit_raw`, `needs_compaction` e `gain`.
- **Proposta:** registar `usage.input` (o provider já o dá, `TokenUsage`) em `provider.request` e
  usá-lo para **calibrar** um rácio `bytes/token` por modelo, `Metric` com base `measured`;
  publicar em `bench/published.toml`. Tokenizer exato só se o A/B provar misbudget > limiar.
- **Teste:** proptest determinístico; desvio `estimado` vs `provider_reported` dentro de X %;
  `context.build` passa a registar `tokens`.
- **Adoção:** reverter se o desvio não melhorar ≥ 20 % face a `bytes/4`.
- **Mapa:** E18-T01/F1 · E09-T05.

#### Q-02 · Contexto por utilidade + fusão de canais (F2 + F9)
- **Problema:** `fit_raw` mantém o **sufixo**, não o **útil**; canais (memória, diffs, ficheiros,
  âncoras) não se fundem.
- **Proposta:** `assemble(state,budget)` com utilidade submodular + MMR (`λ`, `sim_max` como
  **dados** versionados) e fusão **RRF** (`k=60`, pesos dados); desempate `(marginal desc, id asc)`.
- **Teste:** mesmo input → mesmo contexto (proptest); `U(greedy) ≥ U(baseline)`; nenhum par acima
  de `sim_max`; orçamento exato/+1; A/B de sucesso-de-tarefa com **menos** tokens.
- **Adoção:** ≥ 20 % de redução de tokens com a mesma taxa de sucesso, ou reverter.
- **Mapa:** E18-T02/F2 · E18-T09/F9 · risco R13.

#### Q-03 · Compactação guiada por informação (F3)
- **Problema:** o digest retém tudo por igual (`kind`+excerto), sem entropia/surprisal.
- **Proposta:** `H(P)`, `KL`/`JS(P‖Q)` com gatilho `τ_JS` e orçamento de informação; fica o de maior
  **surprisal**; medir `I_ret/token` (base `inferred`).
- **Teste:** original reconstruível (já existe `recover`); mesmo resumo para o mesmo input;
  desligar = comportamento original (já é o default).
- **Mapa:** E18-T03/F3 · E09-T07.

#### Q-04 · O modelo vê o seu estado e o seu orçamento (transparência)
- **Problema:** `prime()` é estático (gramática + catálogo); o modelo não sabe o modo (plano/
  execução), as regras `Enforced` ativas, o orçamento restante (writes/bytes/execs) nem o
  working set.
- **Proposta:** secção `estado` **compacta e determinística** no prime (sobe `PRIME_VERSION`);
  registar `tokens` em `context.build` e `prompt_tokens` em `provider.request`.
- **Teste:** prime com orçamento de bytes; determinismo; `Model-visible ⟺ logged`.
- **Adoção:** manter só se reduzir turnos de auto-correção (A/B).
- **Mapa:** G6 · E09-T01 · E19.

#### Q-05 · Skills e catálogo com relevância (F2/F9)
- **Problema:** o catálogo de skills (8) e o AGENTS.md entram **inteiros**; sem gating por
  relevância.
- **Proposta:** ranquear skills por âncora/working set (mesma máquina de Q-02); incluir as top-k.
- **Teste:** determinismo; nenhuma skill referida por tool/ficheiro fica de fora.
- **Mapa:** E20-T13 · E18-T02.

### Q-B. Superfície das ferramentas

#### Q-06 · Descrições de parâmetro + exemplos no schema (correção imediata)
- **Problema:** `def`/`property` **descartam** `param.description`; não há exemplos.
- **Proposta:** incluir `description` no JSON Schema de cada propriedade (as specs já as têm, em
  português) e um **exemplo trabalhado** por tool no `catalog`/prime.
- **Teste:** `check-schemas` continua verde; golden do catálogo; A/B de turnos até sucesso.
- **Adoção:** é correção, não otimização — sem critério de reversão (só bytes do catálogo).
- **Mapa:** [`schema/specs.rs`](crates/katu-tools/src/schema/specs.rs) · DF12.

#### Q-07 · `edit` multi-bloco atómico (reduz turnos, o custo dominante)
- **Problema:** uma substituição por chamada; refactors custam N turnos × provider.
- **Proposta:** `edit` aceita uma **lista** de pares `old→new` aplicada **atómica** (tudo ou nada,
  `dry_run`); **não** é tool nova (mantém 11, G3), é um parâmetro.
- **Teste:** atomicidade (falha ⇒ nada muda); determinismo; política por caminho intacta;
  A/B: menos turnos por refactor.
- **Adoção:** ≥ 20 % menos turnos numa tarefa de refactor canônica, ou reverter.
- **Mapa:** E06-T03 · `check-surface`.

#### Q-08 · Erro que ensina (negação acionável)
- **Problema:** `Denied`/`Unavailable` dão `rule_id`+evidência, mas não "o que passaria";
  `edit` sem âncora única não sugere alternativa.
- **Proposta:** mapa determinístico `rule → remédio` na evidência; `edit` devolve as âncoras únicas
  mais próximas; `ToolOutcome::Unavailable` nomeia o controlo e como obtê-lo (DF10 já o permite).
- **Teste:** cada regra `Enforced` com teste de mensagem; bytes do caminho de sucesso inalterados.
- **Mapa:** E02-T04 · E13-T03.

### Q-C. Transparência

#### Q-09 · Atribuição por função visível (stderr) + harness (E18-T10)
- **Problema:** o `StderrSink` não imprime `Record.function`; a atribuição por função é `dev` e
  agregada. Sem isto, P não se prioriza.
- **Proposta:** expor `function` no sink de `stderr` (nível `trace`) e instalar o harness do E18-T10
  (`criterion` micro, `hyperfine` startup, `dhat` alocação; ≥ 3 repetições, IC 95 %), com gate.
- **Teste:** regressão acima do limiar falha o job; todo número publicado tem artefacto (DF5).
- **Mapa:** E18-T10 · E15-T01/T02.

#### Q-10 · Fechar os 3 ids órfãos e o *drift* de doc
- **Problema:** `memory.read`, `memory.compact`, `policy.audit` no catálogo e nunca emitidos;
  ids prometidos no `SURFACE_IMPLEMENTATION` §7 sem emissão.
- **Proposta:** emitir os três no ponto real (ou remover do catálogo, subindo/baixando o teto) e
  alinhar o doc. Um facto, um lar.
- **Teste:** `check-diag`/`check-surface` verdes.
- **Mapa:** E19-T03 · E14-T03.

### Q-D. Segurança e guardrails

#### Q-11 · Confiança por artefacto (F6)
- **Problema:** `Enforced` é categoria declarada, não medida; providers só sinalizam.
- **Proposta:** acumuladores **Beta–Bernoulli** + **LB de Wilson** por regra/tool/provider;
  `Enforced` só com `LB ≥ θ` **e** `n ≥ n_min`; senão demove a `Advisory` com evidência.
- **Teste:** regra só `Enforced` com `n`/limiar; **teste de demolição** quando `LB` cai; nenhum
  auto-scale de modelo (DF8).
- **Mapa:** E18-T06/F6 · DF3/DF5.

#### Q-12 · Anomalia e loop (F7 ✂)
- **Problema:** loop patológico só para no teto global; sem sinal cedo.
- **Proposta:** **CUSUM** (média) + **SPRT** (binário) + novidade por assinatura das últimas tool
  calls; ação = cortar (`kill switch`) ou `NeedsHuman`, **nunca** silencioso.
- **Teste:** falso-positivo **medido**; alarme antes do teto; determinístico.
- **Mapa:** E18-T07/F7 (corta primeiro).

### Q-E. Estabilidade

#### Q-13 · Hedging e backpressure no transporte (F4)
- **Problema:** TTFT remoto p50 2,3 s / p95 4,1 s; sem *hedge*; buffer fixo.
- **Proposta:** pool/backpressure por `ρ` (Little), *hedged requests* acima de `p_hedge`, buffer de
  stream adaptativo ao p95 entre chunks, taxa de acerto do prefix-cache publicada.
- **Teste:** TTFT p95 com artefacto; hedge só acima de `p_hedge`; firewall LLM-free intacta.
- **Mapa:** E18-T04/F4 · E12-T06/T07.

#### Q-14 · Prewarm do prefix-cache a frio (elimina o cliff de ~40 s)
- **Problema:** o 1.º turno local paga 3265 tokens a `cached=0` (~40 s).
- **Proposta:** no arranque/`session.open`, um pedido mínimo com o **prefixo canônico exato**
  (prime + catálogo) para popular o cache; opt-in e policy-gated (custa tokens), **zero** mudança
  model-visible (não entra no log como mensagem).
- **Teste:** TTFT frio com/sem prewarm; artefacto; opt-out explícito; sem alteração de bytes
  observáveis.
- **Adoção:** ≥ 50 % do frio, ou reverter.
- **Mapa:** E12 · G6.

#### Q-15 · Estado persistente e replay O(1) (F5)
- **Problema:** `session.open` relê log/snapshot; custo cresce com a história.
- **Proposta:** partilha estrutural + `checkpoint = snapshot + Δ`; hash canônico do estado.
- **Teste:** snapshot O(1) medido; `state_of(replay) == state_at_end`; replay de deltas byte-a-byte.
- **Mapa:** E18-T05/F5 · E04-T03.

---

## 3. Eixo P — performance bruta (prioridade 2)

#### P-01 · `group-commit` do log e dos snapshots (§42)
- **Problema:** `log.append` faz `sync_data()` por evento e `fs.write` faz `sync_all()` por
  escrita → ~44 ms/turno (escala com tools).
- **Proposta:** `fsync` agrupado na fronteira do turno (opt-in, contrato de durabilidade
  **explícito**), reutilizando `toon_bench`/`session_bench`.
- **Teste:** crash-consistency (sem registo rasgado); `replay == estado`; A/B no overhead.
- **Adoção:** ≥ 20 % no overhead do turno, ou reverter. **Se o contrato de durabilidade mudar →
  ADR.**
- **Mapa:** E18-T05 · §42.

#### P-02 · Emissor TOON de uma passagem
- **Problema:** `report::to_toon`+`colunar::emit`+`project` ≈ 270 µs dev; várias passagens e
  `format!`/`join` em laço.
- **Proposta:** escrever **direto** no buffer (`push_str`/`write!`), `Cow` no `project`/`colunar`,
  capacidade pré-alocada, hoisting de `is_scalar` — o análogo katu do O4.5/O4.6 do knudge.
- **Teste:** TOON **byte-idêntico** (goldens/proptest).
- **Adoção:** ≥ 20 % no `toon.emit`, ou reverter.
- **Mapa:** E15-T05/T10 (knudge O4) · [`toon_bench`](xtask/src/toon_bench.rs).

#### P-03 · Caminho `memory.write` / gate
- **Problema:** `memory_write` 347 µs + gate 221 µs (dev); em release 9,9 µs — falta atribuição
  **release** para saber se é quente com muitas notas.
- **Proposta:** peneira por postings no `pre_write` (knudge O2/O3) e reuso do resultado no gate;
  medir em release (Q-09).
- **Teste:** semântica de dedup (0,92) preservada; proptest.
- **Mapa:** E18-T06 · knudge O2.1/O3.

#### P-04 · Transporte do provider (cliente)
- **Problema:** overhead do cliente no gate ~2,3–3,6 ms p95; cauda.
- **Proposta:** medido em Q-13; buffer/parse sem alocação por chunk (como `sse`/`retry`).
- **Teste:** `gate:provider` p95 < orçamento.
- **Mapa:** E18-T04 · E12-T07.

---

## 4. Eixo S — simplificação (prioridade 3)

- **S-01 · Um só caminho de orçamento.** `fit_raw`, `compact`, `project` e `prime` repetem a
  lógica de tokens; unificar em `assemble` com modos (Q-01/Q-02/Q-03).
- **S-02 · `katu.fn` catch-all.** 522 spans com o mesmo id. Decidir: expor a função real (Q-09) ou
  remover o ruído. Não deixar ambíguo.
- **S-03 · Política visível pelo chamador.** `katu-policy` (120 fn) fica fora da cobertura por
  firewall; adicionar spans no **chamador** (`evaluate`/`capability_for`/`audit`) para atribuir o
  custo sem violar camadas.
- **S-04 · Fechar órfãos/drift** (Q-10) e remover nomes mortos (`store.load`/`store.save`, se
  aplicável).
- **S-05 · Duplicação de prime.** Existem três textos (`prime()`, `prime_long()`,
  `cli/prime.rs`); consolidar em **um** gerador versionado, com grupos.

---

## 5. Sequência e dependências

```
W1 (método)      P-00→Q-09  harness + atribuição release  ── desbloqueia P e mede Q
W2 (correções)   Q-06, Q-10, S-01..S-05  (baixo risco, ganho imediato de qualidade)
W3 (dados)       Q-01 → Q-02 → Q-03 → Q-04 → Q-05        (frente F2/F3/F9)
W4 (superfície)  Q-07, Q-08                              (menos turnos)
W5 (segurança)   Q-11, Q-12                              (F6/F7)
W6 (estabilidade)Q-13, Q-14, Q-15, P-01, P-02, P-03, P-04
```

**Regra:** W1 primeiro (sem medir, não se otimiza — a lição do E18 §0.3). W2 são correções que não
esperam. Só depois as frentes formais. Cada PR: A/B + `make check` verde.

---

## 6. Métricas de sucesso (alvos, não promessas)

| Métrica | Hoje | Alvo | Artefacto |
|---|---|---|---|
| Turnos por tarefa canônica | medir (W1) | **−20 %** | `bench/e18/` |
| Tokens de contexto (mesma tarefa) | 3265 | **−20 %** | `raw.json` |
| Overhead fora do provider | 72,3 ms | **≤ 40 ms** | `raw.json` |
| TTFT frio local | ~40 s | **≤ 8 s** (prewarm) | `raw.json` |
| TTFT remoto p95 | 4,1 s | dentro do orçamento | `gate:provider` |
| Regras `Enforced` com `LB`/`n` | 0 | **8/8** | `bench/published.toml` |
| Cobertura de instrumentação | 99,4 % | ≥ 99 % (manter) | `diag:coverage` |

---

## 7. Não-objetivos

- Reimplementar o motor de retrieval do knudge (F2/F9 **consomem**).
- Auto-escalonamento de modelo/pensamento (DF8); as estatísticas só sinalizam.
- Jail de SO (E17, futuro); MCP (futuro); servidor/daemon (G7).
- `criterion`/`rayon`/SIMD como **gate local reflexo** — só por A/B com artefacto (knudge O7).
- Transformar compactação em hot path (é porta, off hot path).

---

## 8. Riscos

| Risco | Mitigação |
|---|---|
| Formalismo vira ornamento | gate: fórmula + artefacto + teste + adotar-ou-reverter |
| Greedy/MMR degrada qualidade | A/B vs baseline; `λ` como dado; reverter sem hesitar |
| Prewarm custa tokens | opt-in, policy-gated, medido, sem efeito model-visible |
| Hedge aumenta custo | razão limitada; só acima de `p_hedge`; contabilizado |
| Multi-edit afrouxa a atomicidade | teste tudo-ou-nada; política por caminho intacta |
| `fsync` agrupado perde durabilidade | contrato explícito + ADR + teste de crash-consistency |
| Estatística com poucas amostras mente | Wilson conservador + `n_min`; SPRT com `α`/`β` declarados |
| Superfície cresce | `check-surface`; teto só em PR (G7) |

---

## 9. Definition of Done

- [ ] Q-01..Q-15 com fórmula, artefacto e teste que os trava; rejeições escritas.
- [ ] W1 instalado: atribuição por função em release + harness e gate de regressão.
- [ ] Cada número publicado em `bench/published.toml` tem base tipada (DF5).
- [ ] Zero alteração de bytes observáveis; `Model-visible ⟺ logged` intacto.
- [ ] `make check` e `msrv` (1.97.0) verdes em cada PR; `OPTIMIZATION_PLAN.md` mantido como lar.

---

## Anexo A — formalismo avançado (propostas mensuráveis)

Formalismos de teoria da informação, estatística e engenharia de modelos de ponta aplicáveis ao
katu. **Distinção:** os que já têm frente no E18 (F1–F9) são marcados `[E18]`; os **aditivos** são
novos. Cada um exige **medida + artefacto + teste** (§0).

### A.1 Entrada — informação e contexto

| # | Formalismo | Onde no katu | Medida (artefacto) | Teste |
|---|---|---|---|---|
| A1 | **Taxa–distorção / Information Bottleneck** (formaliza e melhora F3 `[E18]`) | `context/compact.rs` | bits retidos/token, distorção `D ≤ D0`, sucesso | mesmo input → mesmo `Z`; `I_ret` não cai sem reduzir tokens |
| A2 | **DPP (determinantal) + MMR** (diversidade, estende F2 `[E18]`) | `context::assemble` | similaridade média intra-conjunto, sucesso | determinismo; nenhum par acima de `sim_max` |
| A3 | **Value of Information (VOI)** para retrieval/tool calls | gate de tool no `agent` | tool calls evitadas, sucesso | só chamar se `VOI > custo`; sem perda de irreconstruível |
| A4 | **Retrieval semântico de tools (embedding top-k)** | [`agent/catalog.rs`](crates/katu/src/agent/catalog.rs) | tokens do catálogo, taxa de tool errada | top-k contém sempre a tool correta no conjunto de teste |
| A5 | **Cross-encoder reranking** (o knudge tem `reranking_ann.md`) | hits de memória antes do prime | nDCG/MRR, sucesso | determinismo; base `provider_reported`/`inferred` |
| A6 | **Late interaction (ColBERT)** — multi-vetor | knudge (fronteira §0.1) | recall@k, custo | só como consumo; não reimplementar o motor |

### A.2 Processo — inferência e loop

| # | Formalismo | Onde no katu | Medida (artefacto) | Teste |
|---|---|---|---|---|
| B1 | **Decodificação restrita por gramática (GBNF/JSON Schema)** — maior rácio ganho/risco | request de `provider` (por dialeto) | turnos falhados por JSON inválido → **0**; latência | argumentos válidos por construção; desligar = comportamento atual |
| B2 | **Speculative decoding** (modelo *draft* local) | provider `llama` | tokens/s, TTFT a igualdade de saída | saída idêntica (ou razão de aceitação medida) |
| B3 | **Continuous batching + KV/prefix cache** | servidor local + Q-14 | TTFT, taxa de acerto do prefix-cache | sem mudança model-visible |
| B4 | **Planeamento como POMDP / MCTS-lite com VOI** (estende F8 `[E18]`) | `agent/plan.rs` + plano | passos até resolver, orçamento | caminho crítico estável; prefetch cancelável |
| B5 | **Bandit contextual (Thompson por hash determinístico)** | escolha de tool/provider/estratégia | sucesso/latência por braço | semente/hash fixos → determinístico |
| B6 | **Constrained tool choice / logit bias** | request (por dialeto) | erros de tool (ex.: `write` em modo plano) | regra `Enforced` continua a negar no motor |

### A.3 Resultado — estatística e avaliação

| # | Formalismo | Onde no katu | Medida (artefacto) | Teste |
|---|---|---|---|---|
| C1 | **Sequências de confiança *anytime-valid* / e-values** (melhora F7/SPRT `[E18]`) | monitorização de drift | falso-positivo sob **parada opcional** | cobertura a qualquer `n`; determinístico |
| C2 | **Conformal prediction** para sucesso de tool | F6 `[E18]` | cobertura empírica vs nominal | *exchangeability* das amostras; intervalos reprodutíveis |
| C3 | **Calibração (ECE / Brier + reliability diagram)** | F6 `[E18]` | ECE, Brier | base tipada; nenhuma confiança publicada sem calibração |
| C4 | **Bayes hierárquico (partial pooling)** para tool/provider | F6 `[E18]` | estabilidade do ranking com `n` pequeno | poucos dados por braço não invertem a ordem |
| C5 | **Controlo de múltiplas comparações (Benjamini–Hochberg)** | regras/tools em lote | FDR | rejeições registadas (como no knudge) |
| C6 | **Sketches de streaming: t-digest/HdrHistogram, HyperLogLog** | percentis do provider, novidade (F7) | erro vs exato, memória | limites de erro declarados |
| C7 | **Estatística robusta (mediana/MAD, Theil–Sen, EWMA)** | drift (F7) | falso-positivo com outliers | resistente a um pico isolado |

### A.4 Segurança e integridade

| # | Formalismo | Onde no katu | Medida (artefacto) | Teste |
|---|---|---|---|---|
| D1 | **Information-flow / taint + spotlighting** de output não confiável | tool results antes do modelo | sucesso de injeção numa *suite* red-team | dado untrusted marcado; nunca vira instrução |
| D2 | **Proveniência Merkle + transparency log** (`audit.seal` já no catálogo) | [`audit/`](crates/katu-core/src/audit/mod.rs) | deteção de adulteração | selo recomputável; tamper detetado |
| D3 | **MAC/assinatura de aprovações** (capacidade já existe) | `approval` | aprovações não forjáveis | sem chave → recusa (fail-closed) |

### A.5 Sistemas

| # | Formalismo | Onde no katu | Medida (artefacto) | Teste |
|---|---|---|---|---|
| E1 | **USL / Amdahl** para dimensionar paralelismo | execução de tools, F8 `[E18]` | wall-clock vs threads (curva USL) | teto de concorrência respeitado |
| E2 | **Tail-at-scale hedging + HdrHistogram** | F4 `[E18]` | p99, razão de hedge | só acima de `p_hedge`; custo contabilizado |

### A.6 Prioridade (impacto na qualidade × mensurabilidade × encaixe)

1. **Tier 1 (agora):** **B1** (gramática elimina a classe de falha "JSON inválido" que hoje mata o
   turno em `parse_arguments` → `ProviderError::Decode`), **A4** (menos schema no prompt), **A3**
   (VOI evita tool calls inúteis), **C1** (avaliação contínua honesta), **C3** (confiança calibrada).
2. **Tier 2:** **A1/A2** (formalizam F2/F3), **C2**, **D1**, **C7**.
3. **Tier 3:** **B2**, **B4**, **C4**, **C5**, **E1**, **D2/D3**.

**Nota de fronteira:** A1–A6 tocam o **contexto** (katu); A5/A6 são **consumo** do knudge (§0.1);
nenhum reimplementa o motor de retrieval.

**Definition of Done do Anexo:** cada item adotado traz fórmula, artefacto cru e teste; os
rejeitados ficam escritos com o número que os rejeitou (método §0.3).

---

## Anexo B — DeepSeek Harness / PTC mode (investigação e absorção)

**Fonte:** checkout de referência `_REF/deepseek-harness/` (DeepSeek Harness, "dsh"). Não é
código do katu; nada aqui é copiado, só avaliado. Documentos-chave lidos: Agent Notes
`2026-06-15-ptc.md`, `2026-07-20-ptc-typed-tool-returns.md`,
`2026-09-11-sandboxed-node-ptc-runtime.md`, `2026-07-26-ptc-live-parallel-dispatch.md`,
`2026-08-25-rename-code-mode-to-ptc.md`; snapshot `snapshots/web/ptc-round/` (`system-prompt`,
`tool-schemas`, `session.v4.jsonl`); docs `subsystems/ptc-runtime.md`,
`tool-execution-pipeline.md`.

### B.1 O que é o PTC mode

**PTC = Programmatic Tool Calls** (antes "Code Mode", inspirado no *Code Mode* da Cloudflare;
renomeado para PTC em 2026-08-25). A ideia central: **em vez de emitir uma tool call por passo, o
modelo escreve um programa TypeScript contra um SDK gerado a partir do registo de tools**; o
programa corre num runtime isolado e chama N tools por dentro. Só o que o programa **imprime ou
devolve** volta ao contexto do modelo — os resultados intermédios ficam fora.

Três decisões fundadoras:

1. **É um *modo de apresentação* do registo de tools**, não um acessório: `tools.mode` ∈
   `native` (default) `| ptc | both`. Em `ptc`, o *wire* leva **só** a transport tool reservada
   `run_code` + o `.d.ts` gerado no system prompt; em `native`, as 11+ schemas como hoje.
2. **A execução é uma *capability seam*** (`ctx.ptcRuntime`): recebe programa + *bindings* nomeados
   e devolve `{ value, logs, error? }`. O runtime **não conhece tools**.
3. **Cada programa corre num processo Node novo** (não um REPL persistente), sob **a mesma sandbox
   do Bash**, com ambiente limpo e *bindings* host-owned.

**“Muda como o modelo declara o plano, não o que é permitido.”** Cada chamada aninhada
(`tools.bash(...)`) percorre **o pipeline de tools completo** — pré-execução, guards, aprovação,
política, post-execução — com um id de sub-chamada e o token do pai. As negações voltam ao
programa como rejeição tipada (`ToolCallError` com `toolName`), não como texto.

### B.2 Mecanismos associados (os que importam)

| Mecanismo | O que faz | Porque importa ao katu |
|---|---|---|
| **`run_code` reservado** | única tool diretamente chamável; fora das camadas de restrição | muda a *apresentação*, não a superfície de capacidades |
| **SDK gerado** (`jsonSchemaToTs`) | JSON-Schema → `.d.ts` determinístico; descrições viram JSDoc; nomes exóticos via chave citada | cache de prefixo estável; zero *drift* schema↔código |
| **Typed returns** | binding resolve o **valor canónico JSON** final (pós-política); falha rejeita com `ToolCallError` | composição programática só é possível com valor, não com prosa |
| **Output ledger** | só `logs`/`value`/diagnóstico entram no *ledger* (64 MiB); intermediários **sem cap** | separa fronteira de memória da fronteira de prompt |
| **Concorrência classificada** | `isConcurrencySafe` por tool; pool limitado (`maxParallelSubCalls=10`); chamada exclusiva drena e corre sozinha | o **loop nativo** usa o mesmo contrato (2026-07-10) |
| **Observabilidade** | par `tool/ptc-dispatch-start` / `tool/ptc-dispatch` (log-only), ids `<parent>:ptc:<n>` | sub-chamadas visíveis sem entrar no contexto |
| **Escalação de sandbox** | `sandbox_permissions` + `justification` → **aprovação one-shot**, alargamento **estrito** | padrão de segurança reutilizável (E07) |
| **Taxonomia de falha ortogonal** | `exception ≠ timeout ≠ abort ≠ worker-exit ≠ invalid-output ≠ output-limit ≠ protocol ≠ sandbox-unavailable` | erros que o modelo consegue agir sem ambiguidade |
| **Reconstructabilidade** | programa logado como tool call; intermediários **não** persistidos | honestidade: replay não recria valores intermédios |

**Tools adicionais do harness** (fora das 11 do katu): `todo_write` (≈ `plan`), `skill`, `job_list`
/`job_output`/`job_kill` (background), `subagent`/`subagent_fork`, `ask_user_question`,
`present`, `read_image`, `create_goal`/`get_goal`/`update_goal`. O katu **não** os deve absorver
por G3 (superfície fechada); absorve os **padrões**, não as tools.

### B.3 Ganhos alegados e o que é honesto

O que a nota fundadora afirma e sustenta:

- **Menos round-trips**: multiplos tool calls num só programa, sem uma ida ao modelo por chamada.
- **Menos tokens**: só o que o programa devolve entra no contexto (o resto não é arrastado).
- **Composição**: loop/branch/fan-out sobre resultados — impossível com tools nativas em sequência.
- **Latência**: `Promise.all` sobre chamadas *concurrency-safe* corre em paralelo (até 10).

O que a própria nota **admite** (e devemos respeitar):

- O SDK pode custar tanto como as schemas nativas (pior em `both`); o ganho depende do **cache de
  prefixo** do provider.
- A **paralelismo é limitado pela classificação da tool**, não pelo `Promise.all` do chamador.
- **Deadlines incluem trabalho aninhado** e esperas de aprovação; sem quota de CPU da árvore.
- **Valores intermédios sem cap** podem esgotar memória; `.d.ts` experimental (`stripTypeScriptTypes`).
- **Não há garantia incondicional** de poupança; "measured guidance" fica pós-ship.

### B.4 O que absorver no katu (por custo/risco)

**Camada 1 — absorver agora (sem runtime novo, alinhado com Q/P):**

| # | Elemento | Encaixe | Medida |
|---|---|---|---|
| B-01 | **Concorrência classificada por tool + pool limitado** | `agent/turn.rs` hoje corre `for ... in calls` **sequencial**; introduzir `is_concurrency_safe` fail-closed (default exclusivo) e pool | wall-clock de N leituras independentes; A/B |
| B-02 | **Vários tool calls por passo, com ordem de commit determinística** (já recebemos `step.calls`) | idem | turnos/tarefa |
| B-03 | **Erro tipado que ensina**: `ToolCallError` ↔ enriquecer `ToolOutcome` com `fix`/`toolName` (liga a Q-08) | [`error/mod.rs`](crates/katu-core/src/error/mod.rs) | tentativas cegas ↓ |
| B-04 | **Output ledger unificado** (head/tail + spill) em vez de truncagens ad-hoc | `feedback.rs` + TOON | bytes de output, recuperação |
| B-05 | **Par de eventos de sub-chamada** no catálogo diag (`tool.dispatch.start`/`tool.dispatch.settle`) | `events.rs` | cobertura de instrumentação |
| B-06 | **Escalação de sandbox com justificação + aprovação one-shot** (alargamento estrito) | `approval`/E07 | aprovações forjáveis ↓; falso-negativo |
| B-07 | **Só o delta/devolução reentra no contexto** — tornar explícito no prime o contrato "extrai só o necessário" | `prime` (Q-04) | tokens de turno |

**Camada 2 — avaliar (arquitetural, ADR obrigatório):**

| # | Elemento | Porque hesitar | Caminho |
|---|---|---|---|
| B-08 | **Modo `batch` declarativo** (não Turing-completo): passos sequenciais/paralelos, filtros, condicional simples — captura ~80 % do ganho **sem** motor JS | respeita G3/G7/determinismo; sem dep nova | ADR + proptest de determinismo |
| B-09 | **Runtime programável real** (quickjs/rquickjs/wasmtime/rhai) | quebra binário único/zero-dep e a superfície fechada; `both` custa prompt | só por A/B ≥ 20 % e ADR |
| B-10 | **Background jobs** (o *on-timeout-move-to-background* do `bash`) | nova capacidade (G3); hoje um timeout é ambíguo e falha fechado | avaliar com o kill switch (Q-12) |

**Não absorver (registar):**

- **Node/TypeScript** — contradiz o binário único, zero-dep e G7.
- **REPL persistente** — estado entre `run_code` invisível ao log quebraria `Model-visible ⟺ logged`.
- **Paralelismo sem classificação** — escritas competiriam; o katu é fail-closed.
- **`both` por default** — duplica representações no prompt.

### B.5 Veredicto

A lição mais valiosa do PTC não é o motor JavaScript — é o **contrato**: *a apresentação
(como o modelo declara uma sequência de trabalho) é ortogonal à autoridade (o que pode
fazer)*, e ambos passam pelo **mesmo pipeline** com **erros tipados** e **output curado**. O katu
já tem o pipeline (`kernel/pipeline`, `tool.rs`) e a política fail-closed; falta-lhe (a) **executar
as tool calls de um passo com concorrência classificada** (B-01/B-02, ganho imediato e sem
superfície nova) e (b) decidir, por ADR, se quer um **modo `batch` declarativo** (B-08). A
Camada 1 entra na W4 (superfície) e na W6 (estabilidade) sem alterar G3.
