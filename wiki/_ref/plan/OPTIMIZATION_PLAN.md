# OPTIMIZATION_PLAN — qualidade da execução do modelo, performance e simplificação

> **Plano de otimização** do katu, derivado de três fontes: o método do E18
> ([`plan/19`](19-otimizacao-profunda.md)), a bateria do knudge
> ([`19_performance_reforma_cli.md`](../../../crates/knudge/plan/implementation/19_performance_reforma_cli.md),
> [`otimizacoes_performance.md`](../../../crates/knudge/plan/proposals/otimizacoes_performance.md)) e os
> princípios do [`.agents/skill/rust/`](../../../.agents/skill/git-daily/SKILL.md).
>
> **Evidência de partida** (nenhum número aqui é inventado):
> [`bench/e18/raw.json`](../../../bench/e18/raw.json) (turno), [`bench/e18/atomics/`](../../../bench/e18/atomics/REPORT.md)
> (tempo atómico por função, cobertura 100,0 %), [`bench/mvk/raw.json`](../../../bench/mvk/raw.json) (release),
> [`bench/published.toml`](../../../bench/published.toml) (DF5).
>
> **Autoridade:** os objetivos de [`plan/00b`](00b-objetivos.md) (G1–G9) vencem. Este plano
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
`src/` ≤ 400 linhas (gate `file-length`), `make check` + `msrv` verdes, ids de `diag` no catálogo
([`events.rs`](../../../crates/katu-core/src/diag/events.rs)), superfície só cresce subindo o teto em PR.

---

## 0bis. Estado de execução (o que já foi feito, com artefacto)

| Item | Estado | Evidência |
|---|---|---|
| **Q-06** descrições de parâmetro | ✔ feito | `agent/catalog.rs`; teste `every_parameter_carries_its_description` |
| **Q-10** 3 ids órfãos | ✔ feito | catálogo 114 ids, 0 órfãos; `CATALOG_VERSION = 3` |
| **Q-01** orçamento de tokens calibrado | ✔ feito | rácio medido **3,631 B/token** (`bytes/4` errava 9,2 %); `bench/e18/tokens/`; `context.build{tokens}` |
| **B-02** várias calls por passo | ✔ já existia | testes `calls.rs` (ordem do modelo, barreira) |
| **B-01** lote concorrente `Shared` | ✔ feito | **+76,3 %** no alvo (472 → 112 ms, 8 leituras); `bench/e18/batch/` |
| **Q-16** contexto do projeto no log | ✔ feito | `ProjectContext` em `assemble`; teste `the_project_context_is_logged_for_replay` |
| **Q-02a** corte por unidade (correção) | ✔ feito | sem resultado órfão; teste varre orçamentos `0..=80` (falhava com a política antiga) |
| **Q-21** delta da tool ao modelo (correção) | ✔ feito | §18/G6: `ToolResult.delta` no log e no pedido; testes no loop e no wire |
| **Q-08** erro que ensina | ✔ feito | `remedy` por regra (dado) + `policy:audit` falha sem ele; `edit` devolve as **âncoras únicas mais próximas** (Q-07); catálogo regenerado |
| **Q-05** catálogo de skills curto | ✔ feito | **−73,9 %** (4636 → 1211 B); `katu-core/src/skill.rs` |
| **Q-18** tools por relevância (A4) | ✗ rejeitado | §1.5; ganho máximo seguro **14,0 %** < 20 % |
| **Q-19** `AGENTS.md` condensado | ✔ feito | **−29,1 %** (1590 → 1128 B); cache **rejeitado** (25,8 ms vs. µs) |
| **Q-20** `gate:prompt` | ✔ feito | `xtask gate:prompt`; `bench/e18/prompt/raw.json`; orçamentos travados |
| **S-01** um só caminho de orçamento | ✔ feito | `assemble_all` (1 derivação, 1 partição, 1 teto); testes de invariante em `context/tests/` |
| **Q-02b** contexto por utilidade + MMR + RRF | ✔ feito (**proxy**) | **+1037,9 %** de `I_ret`/token; controlo negativo **0,0 %**; `bench/e18/select/` — *default* fica `suffix` até A/B com o modelo |
| **Q-03** compactação por informação | ✔ feito (**proxy**) | digest por utilidade: **+338,0 %** de `I_ret` com 219 → 50 tokens; gatilho `τ_JS` |
| **Q-04** o modelo vê o estado | ✔ feito (**off**) | secção `estado` + evento `PromptState` (log/replay); ligada por `behavior.prompt_state` |
| **P-02** emissor TOON de uma passagem | ✔ feito | `emit` **−62,0 %** dev / **−33,3 %** release; `to_toon` −39,9 %; `bench/e18/toon/` |
| **Q-07** `edit` multi-bloco atómico | ✔ feito | 5 chamadas → 1 (**−80 %**), payload −19,9 %, atómico (0 B gravados em falha); `bench/e18/edit/` |
| **Q-11** confiança por artefacto | ✔ feito | `Enforced` **medida** no log: prova-se a **n = 25** (LB 902); 1 violação em 20 ⇒ LB 804 + `contradiction`; `bench/e18/confidence/`; comando `policy:confidence` no `check` |
| **P-01** *group commit* do log | ✔ feito | **−78,1 %** no caminho de anexação (384,8 → 84,3 ms/100 eventos, disco real); opt-in `behavior.durability`; ADR 0024; `bench/e18/durability/` |
| **P-04** parser SSE sem alocação por delta | ✔ feito | **−28,5 %** no parser (37 924 → 27 099 ns, mínimo de 1 000 passagens); overhead de cliente p50 0,19 ms; `bench/e18/transport/` |
| **P-03** caminho `memory.write`/gate | ✔ feito | release: gate com 1 000 notas **46 921 → 25 784 µs** (−45,1 %); a atribuição fecha em `Index::from_store` (22,6 ms); `bench/e18/memory/` |
| **Q-15** retomada com cauda limitada | ✔ feito | 20 000 turnos: **321 µs** (−98,7 % vs replay, −85,2 % vs fronteira de fase); cauda ≤ 128 KiB; hash canónico do estado; `bench/e18/resume/` |
| **Q-12** guard de loop/anomalia | ✔ feito | CUSUM + SPRT + assinatura: **0** falsos positivos em 200 turnos normais, alarme no **4.º** passo (teto 12); corte com `agent.loop` + turno fechado; `bench/e18/loop/` |
| **Q-09** atribuição por função no `stderr` | ✔ feito | `StderrSink` imprime `function=…` (nível `trace`); `format_record` puro + 3 testes; `bench/e18/atomics/PROTOCOL.md` atualizado |
| **W7-1** harness estatístico zero-dep | ✔ feito | `katu_core::stats` (`Summary` + IC 95 %: normal para `n ≥ 30`, *bootstrap* determinístico abaixo); `ci95` em `frame.json`/`latency.json`/`atomics`; os gates comparam o **limite superior**; testes `n < 5`/IC contém a mediana/determinismo/orçamento |
| **W7-2** atribuição regerada | ✔ feito | cobertura **1322/1322 (100,0 %)** / **1322/1458 (90,6 %)** com `const fn`; rótulo `policy::evaluate`; `bench/e18/atomics/{raw.json,REPORT.md}` regenerados (`git diff` é a prova) |
| **W7-3** S-04 órfãos | ✔ feito | `context.compact` passou a ser emitido; `check-diag` trava órfãos (`xtask/src/orphans.rs`); **0** órfãos em 114 ids |
| **W8-1** B1 · gramática/schema | ✔ feito | `response_format` `json_schema` derivado das tools do pedido, **opt-in** (`structured_output`) e **fail-open** (400 ⇒ repetição sem o campo); ADR 0025; artefacto `bench/e18/grammar/` (+962 B, p50 153 µs); 4 testes |
| **W8-2** C3 · calibração ECE/Brier | ✔ feito | `katu_policy::calibrate` (10 baldes, base `inferred`); o ECE do registo perfeito desce **730‰ → 83‰** com a evidência; `policy:confidence` publica ECE/Brier; artefacto `bench/e18/confidence/` |
| **W8-3** C1 · e-value anytime-valid | ✔ feito | `kernel::guard` corta por `log(1/α)` (Ville) em vez do SPRT nominal; erro tipo I **≤ α** para qualquer `n` (DP exata, sem RNG); alarme no **5.º** passo; artefacto `bench/e18/loop/` |
| **W8-4** A3 · VOI para tool calls | ✔ feito (decisão escrita) | `agent::turn::voi` (não repetir só-leitura já satisfeita; nunca o irreconstruível); opt-in `behavior.tool_voi` (default **off** até A/B); artefacto `bench/e18/voi/` (2 de 11 evitadas) |

**Nota de método (Q-07).** O `diag:coverage` conta qualquer `fn` de `crates/*/src` fora de
`#[cfg(test)] mod` e fora de ficheiros `*tests*`/`/tests/`: um *bench* em `edit/bench.rs` fez a
cobertura cair para 98,3 % (e cinco `fn` do **fixture** em string eram contados como funções). Movido
para `edit/tests/bench.rs` — a convenção dos outros *benches* (`context/tests/`, `toon/tests/`) — a
cobertura voltou a **99,3 %** sem instrumentar código de teste.

**Nota de método (cobertura, W6).** O número que se lê a seguir ao parêntese (**com `const fn`**)
deslizou para **89,6 %** com as `const fn` novas de Q-15/P-01. Duas correções: (a) o instrumento
contava `crates/*/examples/` (e *benches*) como código de produção — cinco `fn` de exemplo entravam
na conta; (b) faltavam spans em quatro funções de produção e três acessores só usados por testes
passaram a `#[cfg(test)]`. O `check` passa a travar **as duas medidas** em 90 % (antes só a das
instrumentáveis), para o deslize não voltar a passar despercebido.

**Lição de método (B-01).** O primeiro A/B deu −5 % e quase reverteu B-01: a causa era um
*refactor* do `clippy::needless_collect`, que ao encadear `.map(spawn).map(join)` **entrelaça** o
`spawn` e o `join` por call e serializa o lote. O `collect` intermédio é semanticamente necessário.
Um lint de estilo não substitui uma medição.

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
Depois de **P-02** o caminho de emissão ficou em ≈ 139 µs dev (o `emit` sozinho, 153 → 58 µs): o gate
de memória (P-03) passa a ser o maior custo de CPU do dispatch.

### 1.3 A instrumentação está essencialmente fechada

- Cobertura por função: **100 %** das instrumentáveis (**90,5 %** contando `const fn`; o artefacto
  de W7 fixa 90,6 % no seu snapshot); as duas
  medidas são travadas em 90 % pelo `diag:coverage`.
- Catálogo: **114 ids** (`CATALOG_VERSION = 3`), **0 órfãos** (Q-10: `memory.read`/`policy.audit`
  passaram a ser emitidos; `memory.compact` foi removido).
- Atribuição por função **no `stderr`** (Q-09): o `StderrSink` imprime `function=…` (nível
  `trace`), pelo que os *scripts* de `bench/e18/atomics` atribuem o tempo por função também em
  release. O harness `criterion`/`hyperfine`/`dhat` do E18-T10 fica **preterido** (zero-dep, ADR
  0014/E15-T01): repetições e percentis vivem nos *benches* do `xtask` (`render`, `provider`,
  `measure_mvk`, `toon`).

### 1.4 Brechas concretas de qualidade (verificadas no código)

| # | Brecha | Onde | Impacto |
|---|---|---|---|
| a | **Descrições de parâmetro não chegam ao modelo** — `def`/`property` ignoram `param.description`; o JSON Schema só leva `type`/`enum`/`pattern` | [`agent/catalog.rs`](../../../crates/katu/src/agent/catalog.rs) | o modelo adivinha o sentido dos campos → turnos extra |
| b | **Orçamento de tokens é `bytes/4`**, não tokenizer; logo `fit_raw`/`compact`/`gain` ficam `inferred` | [`context.rs`](../../../crates/katu-core/src/context.rs) | decisões de janela erradas; R13 |
| c | **Contexto = sufixo mais recente** (`fit_raw`), sem utilidade/MMR nem fusão de canais (F2/F9) | idem | perde-se o relevante por ser antigo |
| d | **Compactação = excerto de 48 B** por mensagem, sem surprisal/JS (F3) | [`context/compact.rs`](../../../crates/katu-core/src/context/compact.rs) | retém o irrelevante, descarta o denso |
| e | **O modelo não vê o seu estado**: regras ativas, modo, orçamento restante, working set | `prime()` (estático) | não se auto-regula; G6/R13 |
| f | **`edit` é uma substituição por turno**; refactors = muitos turnos (o custo dominante) | [`katu-tools/edit.rs`](../../../crates/katu-tools/src/edit.rs) | turnos × N |
| g | **Negação/erro não ensina a corrigir** (dá `rule_id`+evidência, não "o que passaria") | [`error/mod.rs`](../../../crates/katu-core/src/error/mod.rs) | novas tentativas cegas |
| h | **Sem deteção de loop/anomalia** (F7): só o kill switch global | [`kernel/cost`](../../../crates/katu-core/src/kernel/cost/mod.rs) | queima orçamento devagar |
| i | **`Enforced` não tem confiança medida** (F6) | [`rule.rs`](../../../crates/katu-policy/src/rule.rs) | regra afirmada, não provada |
| j | **A frio, o 1.º turno local custa ~40 s** | — | o pior cliff de UX, na rota crítica |
| k | **Sem hedging/backpressure** no transporte (F4): TTFT remoto p95 4,1 s | [`wire.rs`](../../../crates/katu-providers/src/wire.rs) | cauda lenta e imprevisível |
| l | **O payload da tool não chegava ao modelo** — `Dispatch`/`ToolReport` era descartado em `batch::commit`/`execute_call`; o resultado projetado era só o `ToolOutcome` (`{"Ok":null}`) | [`agent/turn/batch.rs`](../../../crates/katu/src/agent/turn/batch.rs), [`agent/mod.rs`](../../../crates/katu/src/agent/mod.rs) | **o modelo ficava cego** ao que a tool devolveu (viola §18/G6) — a brecha mais grave; corrigida em Q-21 |

### 1.5 A composição do prompt (medida, não estimada)

O `bench/e18/REPORT.md` §121 admitia que a composição dos 3265 tokens **não** estava medida.
Está agora, e é travada por um gate (artefacto: [`bench/e18/prompt/raw.json`](../../../bench/e18/prompt/raw.json);
protocolo: [`PROTOCOL.md`](../../../bench/e18/prompt/PROTOCOL.md)):

| parte | antes | depois | Δ |
|---|---|---|---|
| `tools` JSON Schema (wire) | 5013 B / 1381 tok | 5013 B / 1381 tok | — (contrato do endpoint) |
| catálogo de skills (8) | 4828 B / 1330 tok | **1211 B / 334 tok** | **−73,9 %** (Q-05) |
| `AGENTS.md` | 1591 B / 438 tok | **1128 B / 311 tok** | **−29,1 %** (Q-19) |
| prime (`context::prime()`) | 1340 B / 369 tok | 1340 B / 370 tok | — |
| **`system`** | 7759 B / 2137 tok | **3679 B / 1014 tok** | **−51,4 %** |
| **prompt (system + tools)** | ~3518 tok | **~2394 tok** | **−30,9 %** |
| *os mesmos tools em TOON colunar* | *288 B* | — | (17,4× menor, mas o endpoint exige JSON) |

**Leitura:** o custo não estava no JSON por ser JSON. Estava (a) no **catálogo de skills**, que é
texto nosso e pagava a descrição de parágrafo inteiro de cada skill, e (b) no `AGENTS.md`, que
pagava a sintaxe redundante do router. Os **`tools` (38 %) não descem**: o contrato do endpoint
obriga a JSON Schema e a única forma de cortar ≥ 20 % seria **omitir** tools que o prime instrui o
modelo a usar (Q-18, rejeitado com o número: **14,0 %** omitindo só `move`/`trash`).

**Aprendizagem:** densificar **prosa** com TOON não ajuda (o ADR 0006 mede ganhos em **registos**);
o que ajuda é **encurtar** o texto nosso e **medir** o que sobra.

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
- **Correção (feita, Q-02a):** o corte era **mensagem a mensagem** e podia separar um `ToolCall` do
  seu `ToolResult`. Um lote B-01 loga N pedidos seguidos de N resultados, e o wire OpenAI exige que
  um `role: "tool"` responda a um `tool_calls` precedente — o pedido ficava **inválido**. Verificado:
  com a política antiga, orçamento 4 deixa `ToolResult{c2}` sozinho. Agora o corte é por
  **unidade** (corrida maximal de mensagens de tool, ou mensagem isolada); teste
  `the_cut_never_orphans_a_tool_call_or_result` varre orçamentos `0..=80` e falha com a política
  antiga.
- **Proposta (feita, Q-02b):** `select::chosen_units` com utilidade **submodular** (massa IDF sobre
  termos ainda não cobertos) + diversidade **MMR** (`λ`, `sim_max`) + fusão **RRF** (`k=60`, 4 canais:
  recência, massa, objetivo, evidência) — parâmetros como **dados** versionados
  (`SELECTION_SCHEMA_VERSION`), desempate `(marginal desc, índice asc)`. Fronteira: só ao **prefixo**
  (a última unidade fica intacta) e partilhada com a compactação (Q-03/S-01) — um só mecanismo para o
  mesmo orçamento.
- **Teste:** determinismo (mesmo input → mesma escolha), orçamento exato em todos os valores,
  `U(utilidade) ≥ U(sufixo)`, nenhum par acima de `sim_max`, e o invariante de não-órfão varrido.
- **Resultado (proxy):** **+1037,9 %** de `I_ret`/token no cenário com enchimento repetitivo
  (864 → 118 tokens) e **0,0 %** no controlo negativo uniforme. Artefacto `bench/e18/select/raw.json`;
  métricas `q02b.selection.*`.
- **Adoção:** o critério de 20 % está **cumprido no proxy**, mas `I_ret` não é sucesso de tarefa: o
  *default* continua `SelectionPolicy::Suffix` até haver A/B com o modelo (`behavior.context_selection`
  permite ligar). Custos declarados: o greedy é `O(n²·termos)` e pode largar o meio da conversa.
- **Mapa:** E18-T02/F2 · E18-T09/F9 · risco R13 · Q-03/S-01.

#### Q-21 · O delta da tool chega ao modelo (correção §18/G6)
- **Problema (verificado no código e no log):** a execução de uma tool produzia um `Dispatch` com
  `report`, mas o loop só cometia `outcome` (`batch::commit`, `execute_call`); a projeção
  (`Message::ToolResult`) levava apenas o `ToolOutcome`, e o wire serializava-o — o modelo recebia
  `{"Ok":null}` depois de um `read`. §18 e G6 exigem o contrário ("só o delta chega ao modelo"):
  o agente estava **cego** ao que lia.
- **Proposta:** o **delta** (o TOON do envelope, `ToolReport::to_delta`) entra no evento
  `ToolResult` e é a **mesma** string que o provider recebe (`report::tool_content`) — `Model-visible
  ⟺ logged` exato (E04). Teto `MAX_DELTA_BYTES = 8 KiB` com ponteiro (`kind`/`id`) para o modelo
  pedir uma página; o delta conta para o orçamento de contexto e para o digest da compactação.
- **Teste:** `the_tool_payload_reaches_the_model` (loop real com `FakeProvider`: o conteúdo lido
  está no resultado projetado e no log) e `a_tool_result_carries_the_delta_to_the_wire` (o corpo do
  pedido leva o delta; sem delta, o efeito serializado, como antes).
- **Adoção:** correção — sem critério de reversão.
- **Efeito colateral (S):** os 4 dialetos deixaram de precisar de `Result` em `encode_message`
  (`tool_content` é infalível) — uma simplificação que veio de graça.
- **Mapa:** §18 · G6 · E04-T03 · [`report.rs`](../../../crates/katu-core/src/report.rs) ·
  [`project.rs`](../../../crates/katu-core/src/kernel/project.rs).

#### Q-03 · Compactação guiada por informação (F3)
- **Problema:** o digest retém tudo por igual (`kind`+excerto), sem entropia/surprisal; e a truncagem
  é **cronológica**, pelo que perde a cauda do prefixo (a parte mais próxima do sufixo cru).
- **Proposta (feita):** as linhas do digest são escolhidas pela **mesma** máquina da seleção
  (submodular + MMR + RRF) sob o mesmo teto `summary_max`, e a compactação só se aplica se o prefixo
  disser algo que o sufixo não diz — gatilho `JS(P_prefixo ‖ P_sufixo) ≥ τ_JS` (com `τ_JS` como
  dado). Métricas publicadas: `I_ret` e `JS`, ambas base `inferred`.
- **Teste:** digest por utilidade retém ≥ informação que o cronológico no mesmo teto (com o termo
  raro no **fim** do prefixo, que é o que a cronologia corta); prefixo redundante **não** gasta o
  resumo; determinismo; `recover` continua a recuperar o original.
- **Resultado (proxy):** **+338,0 %** de `I_ret` (219 → 50 tokens). Artefacto
  `bench/e18/select/raw.json`; métricas `q03.digest.*`.
- **Adoção:** igual a Q-02b — critério cumprido no proxy, *default* histórico mantido até A/B com o
  modelo.
- **Mapa:** E18-T03/F3 · E09-T07 · §1.2.

#### Q-04 · O modelo vê o seu estado e o seu orçamento (transparência)
- **Problema:** `prime()` é estático (gramática + catálogo); o modelo não sabe o modo (plano/
  execução), as regras `Enforced` ativas, o teto de passos nem o *working set*.
- **Proposta (feita):** secção `estado` **compacta e determinística** (`context/state.rs`, teto
  `MAX_SECTION_BYTES`, `working set` limitado a 8 caminhos, sem duplicados) acrescentada **no fim** do
  prime — o resto do prompt continua prefixo estável dentro do turno. Regista-se no log como
  `Event::PromptState { turn, text }` com o texto **exato** (`Model-visible ⟺ logged`, E04), pelo que
  uma retomada reproduz o prompt enviado. `context.build` já publica `tokens` e `provider.request`
  `input_tokens` (Q-01/Q-20).
- **Teste:** determinismo, teto de bytes, `-`/`?` quando não há facto, o texto entra no fim do prime,
  o evento está no log, orçamento exato (+bytes da secção), e **desligado por omissão**.
- **Adoção:** a secção está **desligada** (`behavior.prompt_state = false`): a adoção exige A/B de
  turnos de auto-correção com o modelo, que não existe. Custo **medido em execução real**
  (`katu run … --json`, ligada vs. desligada): **+69 tokens de input** (2724 → 2793), com o cache de
  prefixo a funcionar (`cached=812`) e as 8 regras `Enforced` visíveis. O ganho fica por medir.
- **Decisão de desenho:** o estado é **estável dentro do turno** (teto de passos, não o que resta).
  Um valor que muda a cada passo faria o prompt de sistema mudar dentro do turno e destruiria o cache
  de prefixo do provider (§1) — o preço seria maior que a informação acrescentada.
- **Mapa:** G6 · E09-T01 · E19 · §1.1.

#### Q-05 · Skills e catálogo com relevância (F2/F9)
- **Problema:** o catálogo de skills (8) e o AGENTS.md entram **inteiros**; sem gating por
  relevância. Medido (§1.5): **4828 B / ~1330 tokens — 37 % do prompt**, porque o `catalog()`
  emite a descrição **completa** do frontmatter (o `knudge` sozinho custa 1043 B).
- **Proposta:** (a) **descrição de uma linha** no catálogo (primeira frase, teto de caracteres; a
  descrição completa continua no `SKILL.md`, que o modelo lê com `read`); (b) **caminho relativo**
  à raiz (sem o prefixo da máquina: menos bytes e prompt independente do *checkout*); (c)
  **ordem determinística por relevância** face ao objetivo (sobreposição de termos; empate pelo
  nome), **sem omitir** skills — a omissão (top-k) só entra acima de um limiar de catálogo
  **medido**.
- **Teste:** determinismo (mesmo objetivo → mesma ordem); a primeira frase nunca excede o teto;
  uma skill referida pelo objetivo fica à frente; o `SKILL.md` fica intacto.
- **Resultado:** **−73,9 %** (4636 → 1211 B; 1330 → 334 tok) — adotado. Artefacto
  `bench/e18/prompt/raw.json`; o ganho é derivável do artefacto (baseline calculada da mesma
  fonte). A omissão (top-k) fica por fazer: com 8 skills o catálogo já cabe no alvo.
- **Mapa:** E20-T13 · E18-T02 · §1.5.

### Q-B. Superfície das ferramentas

#### Q-06 · Descrições de parâmetro + exemplos no schema (correção imediata)
- **Problema:** `def`/`property` **descartam** `param.description`; não há exemplos.
- **Proposta:** incluir `description` no JSON Schema de cada propriedade (as specs já as têm, em
  português) e um **exemplo trabalhado** por tool no `catalog`/prime.
- **Teste:** `check-schemas` continua verde; golden do catálogo; A/B de turnos até sucesso.
- **Adoção:** é correção, não otimização — sem critério de reversão (só bytes do catálogo).
- **Mapa:** [`schema/specs.rs`](../../../crates/katu-tools/src/schema/specs.rs) · DF12.

#### Q-07 · `edit` multi-bloco atómico (reduz turnos, o custo dominante)
- **Problema:** uma substituição por chamada; refactors custam N turnos × provider.
- **Proposta (feita):** `edit` aceita uma **lista** de pares `old→new` (`Replacement`) aplicada
  **atómica** (tudo ou nada, `dry_run`); **não** é tool nova (mantém 11, G3), é um parâmetro: o par
  `old`/`new` passou a `ListText` no schema e a forma única continua **aceite** pelo roteador (um
  texto = lista de um). A recusa por “não encontrado” devolve as **âncoras únicas mais próximas** e o
  remédio (Q-08).
- **Teste:** atomicidade (a 3.ª falha ⇒ nada muda), ordem (a 2.ª âncora pode depender da 1.ª),
  equivalência do ficheiro final, âncoras determinísticas, `dry_run`, comprimentos diferentes ⇒ erro
  que ensina.
- **Resultado:** 5 chamadas → **1** (**−80 %**, critério ≥ 20 % cumprido no proxy determinístico),
  payload do modelo 763 → **611 B** (**−19,9 %**), eventos de log 10 → 2; com a 3.ª substituição a
  falhar, a forma antiga deixava o ficheiro a meio (375 B) e a atómica **não escreve nada** (383 B =
  original). Artefacto `bench/e18/edit/raw.json`; 3 métricas `q07.edit.*`.
- **Custo:** o wire das tools cresceu **+162 B / +45 tokens** (5 013 → 5 175 B; medido no
  `gate:prompt`, cujo teto subiu de 5 100 para 5 200 B — e que **passou a ser verificado**: até aqui
  o teto das tools era impresso mas não travava nada). Não é parâmetro novo, mas **é** custo de
  prompt.
- **Adoção:** aceite (critério cumprido e a atomicidade é uma correção, não uma preferência). O A/B de
  **turnos** com o modelo continua a não ser executável localmente — o que se publica é o proxy de
  chamadas, com a limitação escrita.
- **Mapa:** E06-T03 · `check-surface` (11 tools intactas) · Q-08.

#### Q-08 · Erro que ensina (negação acionável)
- **Problema:** `Denied`/`Unavailable` dão `rule_id`+evidência, mas não "o que passaria";
  `edit` sem âncora única não sugere alternativa.
- **Proposta:** mapa determinístico `rule → remédio` na evidência; `edit` devolve as âncoras únicas
  mais próximas; `ToolOutcome::Unavailable` nomeia o controlo e como obtê-lo (DF10 já o permite).
- **Teste:** cada regra `Enforced` com teste de mensagem; bytes do caminho de sucesso inalterados.
- **Mapa:** E02-T04 · E13-T03.

### Q-C. Transparência

#### Q-09 · Atribuição por função visível (stderr) + harness (E18-T10) — feito
- **Problema:** o `StderrSink` não imprime `Record.function`; a atribuição por função é `dev` e
  agregada. Sem isto, P não se prioriza.
- **Proposta (feita):** expor `function` no sink de `stderr` (nível `trace`) — o formato passa a
  `level=… kind=… [function=…] event=… dur_ns=… fields=…`, com o campo **só** quando o span traz o
  rótulo (os eventos pontuais mantêm a linha antiga). A formatação foi isolada numa função **pura**
  (`format_record`) com três testes. O harness do E18-T10 é o **zero-dep** já instalado
  (`gate:render`/`gate:provider`/`measure_mvk`/`toon_bench`); `criterion`/`hyperfine`/`dhat` ficam
  preteridos (ADR 0014, E15-T01).
- **Teste:** `a_function_span_exposes_its_label`, `an_event_without_a_label_keeps_the_stable_format`
  e `fields_are_redacted_before_printing` (`cargo test -p katu --features profile`).
- **Efeito colateral (correção):** o build com `--features instrument` não compilava —
  `context.compact` registava `js_milli` com um `f64` (o `Value` não tem `From<f64>`); passou a usar
  `evidence::from_f64`.
- **Mapa:** E18-T10 · E15-T01/T02 · `crates/katu/src/diag.rs`.

#### Q-10 · Fechar os 3 ids órfãos e o *drift* de doc
- **Problema:** `memory.read`, `memory.compact`, `policy.audit` no catálogo e nunca emitidos;
  ids prometidos no `SURFACE_IMPLEMENTATION` §7 sem emissão.
- **Proposta:** emitir os três no ponto real (ou remover do catálogo, subindo/baixando o teto) e
  alinhar o doc. Um facto, um lar.
- **Teste:** `check-diag`/`check-surface` verdes.
- **Mapa:** E19-T03 · E14-T03.

### Q-D. Segurança e guardrails

#### Q-11 · Confiança por artefacto (F6) — feito
- **Problema:** `Enforced` é categoria declarada, não medida; providers só sinalizam.
- **Proposta (feita):** acumuladores **Beta–Bernoulli** (`Trials`) + **LB de Wilson** em
  [`confidence.rs`](../../../crates/katu-policy/src/confidence.rs); a observação vem do log
  ([`kernel/confidence.rs`](../../../crates/katu-core/src/kernel/confidence.rs)): o ensaio de uma regra é
  *recusou ⇒ não correu* (a chamada recusada não aparece executada sob o mesmo `CallId`).
  `Enforced` só com `LB ≥ θ` **e** `n ≥ n_min`; senão o veredicto demove a `Advisory` **com a
  evidência** no motivo. A categoria declarada **não** é reescrita: o TOML continua a autoridade.
- **Teste:** `the_wilson_bound_never_exceeds_the_mean` (varre 820 combinações), monotonia em
  sucessos, **demolição** (`one_violation_demolishes_the_rule`), `n_min`, determinismo, e a
  extração no log (recusa honrada/violada/aprovação/id próprio do retry). Operacional:
  `xtask policy:confidence` (no `check`) e a fixture `bench/e18/confidence/fixture.v1.jsonl`, que
  **falha** com a contradição medida.
- **Resultado:** com registo perfeito a regra prova-se a **n = 25** (LB `902 ≥ 900`); a `n = 24`
  fica em `899`; uma violação em 20 derruba o LB de `881` para `804` e marca `contradiction`. Nas 35
  sessões reais: 8 regras `Enforced`, **0** recusas ⇒ 8 `unmeasured` (o modelo local não emite tool
  calls nativas) — declarado, não escondido.
- **Adoção:** é uma **correção** (categoria afirmada sem prova), não um ganho de latência: o critério
  é ter veredicto medido com evidência + demolição + determinismo.
- **Custos:** zero tokens de prompt (auditoria); uma passagem O(eventos) e dois mapas.
- **Limites:** não mede sucesso de tarefa nem se o remédio ensina (isso é Q-12); LB unilateral a 95 %
  múltiplas comparações tratadas depois em **C5** (Benjamini–Hochberg a `q = 5 ‰`, fail-closed);
  per-tool mede o contrato de conclusão (timeouts), per-provider **não** foi feito (a base já é
  medida pelo `gate:provider`).
- **Mapa:** E18-T06/F6 · DF3/DF5.

#### Q-12 · Anomalia e loop (F7 ✂) — feito
- **Problema:** loop patológico só para no teto global; sem sinal cedo (queima orçamento devagar).
- **Proposta (feita):** **CUSUM** (média, fração de repetição) + **SPRT** (binário, passo
  inteiramente repetido) + novidade por assinatura (FNV-1a de nome + argumentos canónicos) em
  [`kernel/guard.rs`](../../../crates/katu-core/src/kernel/guard.rs). Um passo com chamada **exclusiva** é
  progresso e reinicia o detector (o *polling* legítimo de `bash` não é cortado). Ação = **cortar**
  antes de executar as tools, com `agent.loop` no catálogo, erro `LoopDetected` (categoria
  `conflict`, exit 5) e turno **fechado** — nunca silencioso.
- **Teste:** falso positivo **medido** (`the_guard_has_no_false_positive_on_normal_turns`),
  alarme antes do teto, determinismo, progresso reinicia, `min_steps`, passo vazio, e três testes de
  integração do turno (corte no 4.º passo, turno fechado no corte, turno fechado no teto de passos).
- **Resultado:** **0** falsos positivos em 200 turnos normais de 12 passos; ciclo puro alarme no
  **4.º** passo (SPRT) — **8 passos** antes do teto de 12; o CUSUM, sozinho, precisaria de 5.
- **Adoção:** critério cumprido (falso positivo zero e corte antes do teto).
- **Custos:** zero tokens de prompt; por passo um `BTreeSet` de `u64`, um `String` temporário por
  chamada e dois `ln`.
- **Limites:** A/B sintético (o modelo local não emite tool calls nativas); o *anytime-valid* veio
  depois em **C1** (W8-3: e-value de Ville, `kernel::guard`); não distingue "repetição por falta de
  informação" de "prompt ambíguo".
- **Efeito colateral corrigido:** o turno passa a fechar **sempre** (`TurnEnd`), também em
  `TooManySteps` — antes o log ficava com um turno aberto.
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

### Q-F. Densidade e reconstruibilidade do prompt (F2b)

Nasce da medição de §1.5: o prompt é o gargalo do arranque a frio (~40 s) e o que o katu controla
é texto nosso (skills, AGENTS.md) e a **apresentação** das tools. A ordem é **medir → encurtar →
travar**. Densificar prosa com TOON não entra: o ADR 0006 mede ganhos em registos, não em prosa.

#### Q-16 · Contexto do projeto reconstruível (correção do invariante E04)
- **Problema:** o `AGENTS.md` e o catálogo de skills são lidos em `Runtime::assemble`, injetados
  no `system` e **nunca registados no log**. O `SCOPE_LOAD` cobre o `scope_contract.json`, não o
  `AGENTS.md`. Num `--resume` o log reproduz as mensagens mas o prompt de sistema é relido do
  disco: se o `AGENTS.md` mudou, o replay **diverge** do que o modelo viu — `Model-visible ⟺
  logged` (E04) violado. Um cache de versões densas (Q-19) tornaria a divergência invisível.
- **Proposta:** evento de **controlo** `ProjectContext { agents, skills }` aplicado no `assemble`
  com o **texto exato** que entra no prompt (não a fonte crua, não um hash); a projeção para o
  modelo ignora-o (`project_event` já filtra `_ => None`).
- **Teste:** o log contém o evento com o texto que `system_for` injeta; `derive_messages` não o
  inclui; um replay com `AGENTS.md` alterado reproduz o prompt original.
- **Adoção:** correção — sem critério de reversão (custa ~2 KB por sessão, uma vez).
- **Mapa:** E04-T03 · E20-T13 · [`event.rs`](../../../crates/katu-core/src/kernel/event.rs).

#### Q-18 · Catálogo de tools por relevância (A4, top-k fail-open)
- **Problema:** as 11 schemas JSON (**5013 B / ~1381 tokens — 38 %**) vão em todos os pedidos. O
  contrato do endpoint obriga a JSON Schema, mas **não** obriga a mandar as 11.
- **Proposta:** selecionar o subconjunto relevante ao passo (termos do objetivo + working set,
  determinístico) com **conjunto mínimo sempre presente** e **fail-open** (sem sinal claro →
  todas); a tool correta nunca pode ficar de fora. Base: Anexo A · A4.
- **Teste:** num conjunto canônico de tarefas, a tool correta está sempre no subconjunto; ordem
  canônica; desligar = as 11 (comportamento atual).
- **Adoção:** ≥ 20 % de tokens do catálogo **sem** aumentar turnos nem a taxa de tool errada, ou
  reverter.
- **Resultado: ✗ rejeitado.** A sonda do gate mede o máximo corte **seguro**: omitir `move` e
  `trash` poupa **14,0 %** (5013 → 4310 B) — abaixo do critério de 20 %. Chegar a 20 % exigiria
  omitir também `plan`/`memory`, que o **prime instrui** o modelo a usar (omitir seria incoerente,
  e a taxa de tool errada só se mede com um A/B de sucesso de tarefa, que não existe). O número
  fica reproduzível em `bench/e18/prompt/raw.json` (`probes.tools_core_*`).
- **Mapa:** Anexo A · A4 · [`agent/catalog.rs`](../../../crates/katu/src/agent/catalog.rs) · §1.5.

#### Q-19 · `AGENTS.md` denso + cache validado por *fingerprint*
- **Problema:** o `AGENTS.md` (1591 B) é markdown de router (bullets `- **Label**: alvo — nota`,
  com o alvo em sintaxe de link repetida no texto), e essa sintaxe custa bytes sem informação.
- **Proposta:** transformação **determinística** markdown→compacto (sem IA), **pura** sobre os
  bytes da fonte (`katu_core::prompt::condense`): link cujo texto repete o alvo fica só com o
  texto; fora o negrito; linhas em branco colapsadas. **Sem cache**: ver a rejeição abaixo.
- **Teste:** determinismo (mesma fonte → mesmos bytes); a condensação preserva **todos** os alvos
  (`every_target_survives_the_condensation`); o texto condensado é o que o `ProjectContext` loga
  (Q-16), logo `Model-visible ⟺ logged` mantém-se.
- **Resultado:** **−29,1 %** (1590 → 1128 B) — adotado, aplicado em `read_instructions`.
- **Rejeição do cache (com número):** a condensação é uma passagem linear sobre ~1,6 KB (classe do
  `toon::colunar`: 89 µs em dev, `bench/e18/atomics`), enquanto `fs.write` com `sync_all` custa
  **25,8 ms** (`bench/e18/raw.json`). Um artefacto derivado em `.katu/context/` com validação por
  *fingerprint* pagaria ~300× o custo do cálculo para introduzir risco de *staleness*. O lar seria
  `.katu/` de qualquer forma (`.knudge` não existe neste repo).
- **Mapa:** E20-T13 · `AGENTS.md` (router ≤ 50 linhas) · Q-16 · §1.5.

#### Q-20 · `gate:prompt` — a composição medida e travada
- **Problema:** nada trava a composição do prompt; o §121 do `REPORT.md` mostrou que uma regressão
  passa despercebida.
- **Proposta:** comando `xtask gate:prompt` que reconstrói a composição (AGENTS.md + catálogo de
  skills + prime + schemas JSON das tools) a partir das **funções de produção**, compara com
  orçamentos declarados e publica o artefacto `bench/e18/prompt/raw.json`; a fonte única do JSON
  Schema passa para `katu-tools::schema` (deixa de viver no binário).
- **Teste:** exceder um orçamento falha o gate; o artefacto é reproduzível byte-a-byte.
- **Resultado:** feito — `xtask gate:prompt` (corre no `check`), orçamentos por parte + `system`,
  artefacto `bench/e18/prompt/raw.json`, 6 métricas em `bench/published.toml`, tetos de superfície
  (`xtask_commands` 30, `bench_artifacts` 8). A construção do JSON Schema passou para
  `katu_tools::schema::tool_defs` (o binário deixou de ter uma segunda implementação).
- **Mapa:** E14-T05 (superfície) · E15-T02 (DF5) · §1.5.

---

## 3. Eixo P — performance bruta (prioridade 2)

#### P-01 · `group-commit` do log e dos snapshots (§42) — feito
- **Problema:** `log.append` faz `sync_data()` por evento e `fs.write` faz `sync_all()` por escrita →
  ~44 ms/turno (escala com tools).
- **Proposta (feita):** barreira agrupada na **fronteira do turno**, com o contrato de durabilidade
  explícito em **ADR** ([0024](../adr/0024-durabilidade-do-log.md)) e opt-in por
  `behavior.durability = event|turn` (default `event`: nada muda sem o pedir). A porta `Fs` ganha
  `append_unsynced`/`sync` (default conservador) e o leitor recupera de uma **cauda rasgada**
  (última linha sem `
`), sem deixar de ser fail-closed para corrupção a meio.
- **Teste:** os dois modos escrevem o **mesmo** log (byte a byte), a barreira fecha no `TurnEnd`
  (`the_turn_boundary_closes_the_durability_barrier`), o `flush` é observável (`is_dirty`), a cauda
  rasgada é recuperada e uma linha inválida terminada em `
` continua a ser erro.
- **Resultado (disco real, 100 eventos = 20 turnos × 5):** **384,8 → 84,3 ms** (**−78,1 %**);
  19,2 → 4,2 ms por turno, a fechar com o `log.append` de 18,3 ms/turno do E18 (≈ 3,7 ms por
  barreira).
- **Adoção:** critério (≥ 20 % no overhead do turno) cumprido; artefacto `bench/e18/durability/`.
- **Custos:** a janela de perda passa a ser o turno em curso (escrito na ADR e opt-in); as escritas
  atómicas (notas, snapshots, transcrição) continuam com barreira por escrita.
- **Limites:** o ganho depende do número de eventos por turno e do disco (`tmpfs`/NVMe reduzem a
  diferença — o *bench* recusa `/tmp`).
- **Mapa:** E18-T05 · §42.

#### P-02 · Emissor TOON de uma passagem
- **Problema:** `report::to_toon`+`colunar::emit`+`project` ≈ 270 µs dev; a projeção clonava cada
  string e cada lista aninhada, o buffer crescia por realocação e a sanitização descodificava UTF-8
  `char` a `char` (duas passagens).
- **Proposta (feita):** reserva **exata** do buffer (`byte_len`), guarda **SWAR** de 8 bytes para
  "existe byte de controlo" com passagem lenta exata, `Cow<'a, str>` nas células/secções, listas
  aninhadas emprestadas (`&'a [Value]`), `flatten_into` direto no buffer e nomes emprestados nas
  secções-filho.
- **Teste:** **byte-idêntico** ao emissor anterior (réplica congelada no próprio *bench*, afirmada no
  teste), capacidade reservada **exata** (`byte_len == emitido`), varredura SWAR exaustiva sobre todos
  os pares de bytes + delimitador em cada fronteira de carácter, `int_len` contra `to_string()`.
- **Resultado:** `emit` **−62,0 %** em dev (153,1 → 58,1 µs) e **−33,3 %** em release (5,44 →
  3,63 µs); ponta a ponta `to_toon` **−39,9 %** (conservador; o ganho real em dev é ≈ −49 %).
  Artefacto `bench/e18/toon/{raw.json,raw-release.json}`; 4 métricas `p02.toon.*`.
- **Rejeitado (com o número):** `push_int` manual (dígitos em buffer de pilha em vez de `write!`) —
  parecia poupar o `fmt::Arguments` de ~180 células, mas mediu **+7 %** em `emit` (109,6 → 120,3 µs
  dev). Revertido.
- **Adoção:** critério (≥ 20 % em `toon.emit`) cumprido em dev e em release; a saída é byte-idêntica,
  logo não há risco de prompt.
- **Limite:** em release a projeção é dominada pela **alocação das linhas** (o ganho ponta a ponta cai
  para −6 %); o perfil dev exagera o ganho das alocações eliminadas.
- **Mapa:** E15-T05/T10 (knudge O4) · [`toon_bench`](../../../xtask/src/toon_bench.rs) · §1.2.

#### P-03 · Caminho `memory.write` / gate — feito
- **Problema:** `memory_write` 347 µs + gate 221 µs (dev); em release 9,9 µs — faltava a atribuição
  **release** para saber se o caminho é quente com muitas notas.
- **Proposta (feita):** medir a atribuição em release e **reusar** no commit o índice que o
  `pre_write` do mesmo gate construiu, em vez de o reconstruir (`Knudge::write_context()` →
  `Index::from_store`, que lê e tokeniza todas as notas).
- **Teste:** a escrita continua visível à decisão seguinte (`a_write_is_visible_to_the_next_decision`
  — é a invalidação que torna o reuso seguro), a suíte de conformidade da porta corre contra o
  adaptador, e o *bench* **falha** se a medição não correr o commit (o `decision` publicado é
  `create`).
- **Resultado (release, 1 000 notas):** gate **46 921 → 25 784 µs** (**−45,1 %**; −48,7 % a 100 notas,
  −41,0 % a 500). Atribuição: `Index::from_store` 22,6 ms, `propose` 1,7 ms, escrita da nota 90 µs,
  `open` 60 µs — o custo é reconstruir o índice, e o caminho antigo pagava-o **duas vezes** por gate.
- **Adoção:** critério (≥ 20 %) cumprido; artefacto `bench/e18/memory/raw.json`.
- **Custos:** zero tokens de prompt; o commit é o mesmo (mesma nota, mesmo evento) — muda de onde vem
  o índice consultado, e o índice em cache é o mesmo que a decisão viu (o caminho fica coerente).
- **Limites:** o que resta é `Index::from_store` (ler + tokenizar N notas); reduzi-lo exige índice
  incremental/persistido **no knudge** (`crates/knudge` é submódulo: outro projeto) — registado como
  próximo passo, não como promessa. Base sintética, adaptador in-process (via MCP o custo é do
  servidor).
- **Mapa:** E18-T06 · knudge O2.1/O3.

#### P-04 · Transporte do provider (cliente) — feito
- **Problema:** overhead do cliente no gate ~2,3–3,6 ms p95; cauda.
- **Proposta (feita):** o parser SSE deixou de alocar **duas `String` por delta** (a linha e o
  payload): a linha é interpretada como fatia de `pending` e o payload é emprestado de `self.data`.
- **Teste:** os dois parsers (produção e réplica congelada) entregam exatamente os mesmos eventos
  (`both_parsers_see_the_same_events`, `the_ab_reports_the_same_events_for_both`) e `gate:provider`
  trava o p95 contra o orçamento versionado.
- **Resultado:** **−28,5 %** no parser (37 924 → 27 099 ns no mínimo de 1 000 passagens, ordem
  alternada) para o corpus canónico de 46 912 B; overhead de cliente ponta a ponta p50 **189 689 ns**,
  p95 236 763 ns (orçamento 5 ms).
- **Adoção:** critério (≥ 20 %) cumprido; saída byte-idêntica.
- **Nota de método:** o artefacto de referência do repositório (commit `020e6f3`) media p50 ≈ 1,93 ms
  para o **mesmo** corpus; re-medido o código anterior nesta máquina, hoje, dá ≈ 0,20 ms — aquele
  número foi tomado com a máquina carregada e não é comparável. O A/B é feito na mesma execução.
- **Limites:** o `serde_json` por evento (~512 por turno) continua a dominar o que resta; evitá-lo
  exigiria um decoder incremental (fora do escopo).
- **Mapa:** E18-T04 · E12-T07.

---

## 4. Eixo S — simplificação (prioridade 3)

- **S-01 · Um só caminho de orçamento.** ✔ **feito.** `fit_raw`/`compact`/`needs_compaction`
  derivavam o log e contavam tokens cada um por si (até **três** derivações por turno); agora há
  **uma** derivação, **uma** partição em unidades e **um** teto (`context::assemble_all`, com
  `AssembleOptions`), e a compactação é o que sobra dessa decisão. O sufixo histórico continua a ser
  a política por omissão (byte a byte, com teste que o fixa).
- **S-01b · Micro-otimizações do caminho quente (`.agents/skill/rust`).** ✔ feito, cobertas pelo
  mesmo A/B: o texto *model-visible* sai como `Cow` (nada se clona só para tokenizar — um *delta* de
  vários KiB era copiado a cada turno), o `idf` é pré-calculado (a consulta era um `ln` por termo e o
  greedy consulta a massa `O(n²)` vezes), o ganho marginal não constrói conjuntos temporários, a
  pertença ao escolhido é um **bitmap** (era procura linear na lista), o ranking usa valores
  pré-calculados com `sort_unstable`, os `Vec` têm capacidade pré-alocada e a contagem de caracteres
  de um termo só corre depois do teste barato em bytes.
- **S-02 · `katu.fn` catch-all.** ✔ **decidido:** o id genérico fica e a função real é **exposta**
  (Q-09) — o `StderrSink` imprime `function=…` e o `AggregatingSink` já agrupa por `(event, função)`;
  o rótulo real consta do catálogo (`KATU_FN`) e da doc de `trace_fn!`. Não se adicionam 522 ids.
- **S-03 · Política visível pelo chamador.** ✔ **feito:** spans no chamador com rótulo de função —
  `policy::evaluate` (`kernel::pipeline`), `policy::audit` (`runtime::load_rules`) e
  `policy::capability_for` (`agent::turn`); antes apareciam sem função (`None`) no agregado.
- **S-04 · Fechar órfãos/drift** (Q-10) e remover nomes mortos (`store.load`/`store.save`, se
  aplicável).
- **S-05 · Duplicação de prime.** ✔ **feito:** `context/prime::render` é o **único** gerador
  (`prime`/`prime_with_catalog`/`prime_long` são composições; saída byte-a-byte igual) e o CLI gera
  `name`/`base` da **mesma** lista de grupos (`macro_rules! groups`).

---

## 5. Sequência e dependências

```
W1 (método)      ~~P-00~~→~~Q-09~~  harness + atribuição release  ── desbloqueia P e mede Q
W2 (correções)   Q-06, Q-10, ~~S-01..S-05~~  (baixo risco, ganho imediato de qualidade)
W2b (densidade)  Q-16 → Q-05 → Q-20 → Q-19 → Q-18        (frente F2b; §1.5)
W3 (dados)       Q-01 → Q-02 → Q-03 → Q-04 → Q-05        (frente F2/F3/F9)
W4 (superfície)  ~~Q-07~~, ~~Q-08~~                        (menos turnos)
W5 (segurança)   ~~Q-11~~, ~~Q-12~~                        (F6/F7)
W6 (estabilidade)~~Q-13~~, ~~Q-14~~, ~~Q-15~~, ~~P-01~~, ~~P-02~~, ~~P-03~~, ~~P-04~~
W7 (método)      ~~E18-T10 (harness zero-dep) + regerar atribuição + S-04~~  ✔
W8 (Anexo A T1)  ~~B1 gramática + C3 calibração + C1 e-values + A3 VOI~~ ✔
W9 (Anexo B)     ~~B-03~~ → ~~B-04~~ → ~~B-06~~ → ~~B-07~~ → ~~ADR B-08~~ ✔
W10 (Anexo A T2/3) ~~A1/A2~~ (A1 adotado, DPP rejeitado) · ~~D1~~ · ~~C5~~ · ~~C7~~ · ~~D2/D3~~
                 ~~E1~~ (medido, sem mudança) · ~~C2~~ (medido, rejeitado) ·
                 ~~E18-T05~~ (feito em Q-15) · ~~E18-T08/T09~~ (rejeitados)  ✔
```

**Regra:** W1 primeiro (sem medir, não se otimiza — a lição do E18 §0.3). W2 são correções que não
esperam. Só depois as frentes formais. Cada PR: A/B + `make check` verde.

---

## 6. Métricas de sucesso (alvos, não promessas)

| Métrica | Hoje | Alvo | Artefacto |
|---|---|---|---|
| Turnos por tarefa canônica | medir (W1) | **−20 %** | `bench/e18/` |
| Tokens de contexto (mesma tarefa) | 3265 | **−20 %** | `raw.json` |
| Seleção por utilidade (`I_ret`/token) | — | **+20 %** no proxy ✔ +1037,9 % | `bench/e18/select/raw.json` |
| Controlo negativo da seleção | — | **0 %** (métrica não é trivial) ✔ | idem |
| Digest por informação (`I_ret`) | — | **+20 %** no proxy ✔ +338,0 % | idem |
| Secção `estado` no prime | — | custo medido ✔ / ganho por medir (off) | idem |
| Tokens do prompt (`system` + tools) | 3465 | **≤ 2800** | `bench/e18/prompt/raw.json` |
| Catálogo de skills | 1330 tok | **≤ 400** ✔ 334 | idem |
| `AGENTS.md` no prompt | 438 tok | **≤ 350** ✔ 311 | idem |
| Schemas JSON das tools | 1381 tok | **≤ 1100** ✗ rejeitado (14,0 %) | idem |
| Composição travada por gate | não existe | `gate:prompt` verde ✔ | idem |
| Overhead fora do provider | 72,3 ms | **≤ 40 ms** | `raw.json` |
| TTFT frio local | ~40 s | **≤ 8 s** (prewarm) | `raw.json` |
| TTFT remoto p95 | 4,1 s | dentro do orçamento | `gate:provider` |
| Regras `Enforced` com `LB`/`n` | 0 | **8/8** | `bench/published.toml` |
| Cobertura de instrumentação | 100 % (90,5 % com `const fn`) | ≥ 90 % nas duas medidas ✔ | `diag:coverage` |

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

Verificado item a item em 2026-02; o que mudou desde a última revisão está assinalado.

- [x] Q-01..Q-20 com fórmula, artefacto e teste que os trava; rejeições escritas.
- [x] S-01: uma só derivação/partição/teto por turno (Q-02b/Q-03 partilham a mesma máquina).
- [x] Q-02b/Q-03: critério cumprido no **proxy** (`I_ret`, com controlo negativo a 0 %) e *default*
      histórico mantido até A/B com o modelo — a decisão está escrita, não implícita. *(A1: a
      política de utilidade distorce 0 ‰ contra o piso histórico de 356 ‰.)*
- [x] Q-04: secção `estado` reconstruível do log (`PromptState`) e **desligada** por omissão.
- [x] §18/G6 fechado: o **delta** da tool é o mesmo texto no log e no pedido — há **uma** função
      (`ToolReport::to_delta`), consumida em `kernel/pipeline` (evento) e em `report::tool_content`
      (provider) — e o corte do contexto nunca parte um par `ToolCall`/`ToolResult`
      (`the_cut_never_orphans_a_tool_call_or_result`).
- [x] `Model-visible ⟺ logged` **fechado** também para o contexto do projeto (Q-16): o prompt de
      sistema é reconstruível do log.
- [x] Cada número publicado em `bench/published.toml` tem base tipada (DF5) — `evidence::Metric`
      com `basis`/`artefacto`, e o `gate:bench` falha se faltar um dos dois.
- [x] Bytes observáveis alterados **apenas** pelo que foi medido: o envelope `<katu:untrusted>` do
      D1 (71 B por resultado, 0,87 % num delta de 8 KiB, **teto total inalterado**). Fora isso, zero
      alteração, e `Model-visible ⟺ logged` intacto. *(Reescrito: a versão anterior dizia «zero
      alteração de bytes observáveis», o que passou a ser falso por desenho no D1.)*
- [x] `make check` e `msrv` (1.97.0) verdes em cada PR; `OPTIMIZATION_PLAN.md` mantido como lar.
- [x] W1 instalado: atribuição por função em release + harness e gate de regressão.
- [x] W7 instalado: harness estatístico zero-dep (IC 95 % nos artefactos; gates pelo limite superior) +
      atribuição regerada (`bench/e18/atomics/`) + **0** ids órfãos travados pelo `check-diag`.
- [x] W8-1/W8-2 instalados: decodificação estruturada por JSON Schema (opt-in, fail-open, ADR 0025;
      `bench/e18/grammar/`) e calibração ECE/Brier publicada (`bench/e18/confidence/`).
- [x] W8-3 instalado: e-value *anytime-valid* no guard (corte `log(1/α)`; erro tipo I ≤ α para
      qualquer `n`, DP exata; `bench/e18/loop/` regenerado).
- [x] W8-4 instalado (decisão escrita): gate de VOI em `agent::turn::voi` (opt-in
      `behavior.tool_voi`, default **off** até A/B; `bench/e18/voi/`).
- [x] W10 fechado, item a item com fórmula/artefacto/decisão: **adotados** D1 (taint), C5
      (Benjamini–Hochberg), C7, A1, D2/D3; **medidos sem mudança** E1; **rejeitados com número**
      A2/DPP, C2, E18-T05 (restante), E18-T08, E18-T09 — e nenhum rejeitado deixou código em
      `src/`.

## 10. Próximos passos (W7–W10)

**Ponto de partida.** W1–W7 estão fechadas e W8-1/W8-2/W8-3/W8-4 + W9-1/W9-2 instalados: Q-01..Q-21 e
P-01..P-04 com artefacto em `bench/`, S-01..S-05 fechados (o resíduo S-04 — o id órfão
`context.compact` — fechou em W7-3), o harness estatístico zero-dep de E18-T10 instalado (W7-1), a
decodificação estruturada (B1), a calibração ECE/Brier (C3), a e-value *anytime-valid* (C1), o gate
de VOI (A3, *default* off), o erro tipado que ensina (B-03) e o output ledger unificado (B-04)
publicados. O que falta vem do **Anexo A** (Tier 2–3) e do **Anexo B** (B-06 → B-07 → ADR B-08). A
ordem segue Q > P > S e o método do §0: cada item traz fórmula, artefacto cru e teste que o trava;
adotar-ou-reverter.

### W7 · Fecho de método (pré-requisito de W8)

**W7-1 · Harness estatístico zero-dep (E18-T10).** ✔ feito
- **Onde:** `katu_core::stats` (fonte única — o `measure_mvk` é um `example` do binário e não pode depender do `xtask`) + `xtask/src/stats.rs` (forma JSON dos artefactos), consumido por `render_bench`, `provider_bench` e `measure_mvk`.
- **Problema:** os gates medem p95 sem IC 95 % nem repetições declaradas; Q-09 ficou sem harness.
- **Proposta:** `Summary { n, p50, p95, mean, ci95_low, ci95_high }` com IC 95 % (normal sobre a
  média para `n ≥ 30`; para `n` menor, *bootstrap* **determinístico** — índices derivados do próprio
  `n`, sem RNG); artefactos ganham `"ci95"`; o gate compara o **limite superior** com o orçamento.
  `criterion`/`hyperfine`/`dhat` continuam **preteridos** (ADR 0014); um contador de alocação próprio
  só entra se um A/B o justificar.
- **Medida/artefacto:** `bench/render/frame.json`, `bench/providers/latency.json`,
  `bench/e18/atomics/raw.json` (campos novos; `gate:bench` valida).
- **Teste:** `n < 5` ⇒ erro; o IC contém a mediana; determinismo (mesmo input → mesmo `Summary`);
  regressão acima do orçamento falha.
- **Adoção:** menos falsos alarmes sem esconder regressões.
- **Resultado ✔:** `Summary { n, p50, p95, mean, ci95_low, ci95_high }` em `crates/katu-core/src/stats.rs`; IC 95 % normal (`n ≥ 30`) ou *bootstrap* determinístico (`splitmix64` semeado por `n`, sem RNG); o intervalo é alargado para conter a mediana e a média. `frame.json`/`latency.json`/`atomics` ganham `ci95`; `gate:render`/`gate:provider` falham quando o limite superior excede o orçamento. Testes: `n < 5` ⇒ erro, o IC contém a mediana, determinismo e regressão acima do orçamento.

**W7-2 · Regerar a atribuição (fecha S-03/Q-09).** ✔ feito — regenerar `bench/e18/atomics/raw.json`/`REPORT.md`
com a cobertura atual (**1322/1322 = 100,0 %** instrumentáveis / **1322/1458 = 90,6 %** com `const fn`)
e os rótulos novos (`policy::evaluate`/`policy::audit`/`policy::capability_for`); o `git diff` do
artefacto é a prova. Gate: `diag:coverage` + `gate:bench`. O `bench/mvk/raw.json` versionado é o
baseline **release** (citado em `published.toml`) e **não** foi tocado por uma corrida `dev`.

**W7-3 · S-04 (drift residual).** ✔ feito — a varredura encontrou **um** id órfão (`context.compact`, nunca
emitido); passou a abrir o span da compactação (`fn_span!` em `compact_prefix`) e o `check-diag`
passou a travar órfãos (`xtask/src/orphans.rs`, 114 ids, 0 órfãos). `store.load`/`store.save` estão
vivos. Nenhum teto de `surface.toml` subiu.

### W8 · Anexo A — Tier 1

**W8-1 · B1 · Decodificação restrita por gramática (maior rácio ganho/risco).** ✔ feito
- **Onde:** request do dialeto `chat/completions` em `katu-providers` (o `llama-server` aceita
  `grammar`/`response_format: json_schema`; um endpoint remoto pode recusar).
- **Proposta:** campo opcional no pedido, derivado de `katu_tools::schema`, **fail-open** (sem
  suporte do endpoint ⇒ comportamento atual) e **opt-in** por provider. Não muda o plano de dados:
  muda como o modelo *declara* a chamada.
- **Medida:** turnos falhados por JSON inválido em `parse_arguments` (`ProviderError::Decode`) →
  **0**; latência; artefacto `bench/e18/grammar/`.
- **Teste:** fixture de argumentos malformados passa a válido; desligado = bytes atuais; ADR (a criar)
  regista a capacidade por provider.
- **Adoção:** ≥ 20 % dos turnos falhados evitados; senão reverter e escrever o número.
- **Resultado ✔:** `response_format: {json_schema: {oneOf: […]}}` derivado dos `ToolDef` do pedido
  (`tool_call_schema`), `strict: true`; **opt-in** (`structured_output` / TOML declarativo /
  `ModelEntry`/`LlamaConfig`) e **fail-open** (um `400` repete o pedido sem o campo). Desligado é
  byte a byte o atual. ADR **0025**. Artefacto `bench/e18/grammar/raw.json` (3 tools): corpo
  `970 → 1 932 B` (+962), p50 de codificação `35 → 153 µs` (IC 95 % do harness de W7). Testes:
  `structured_output_is_off_by_default`, `structured_output_adds_a_schema_derived_from_the_tools`,
  `the_schema_excludes_malformed_arguments` e `a_rejected_structured_request_falls_back_to_the_current_bytes`.
  A adoção por omissão exige turnos reais com `Decode` (o modelo local não emite tool calls
  nativas): fica **`unpriced`**, escrita no artefacto — não se inventa.

**W8-2 · C3 · Calibração da confiança (ECE/Brier).** ✔ feito
- **Onde:** `katu-policy::confidence` + `bench/e18/confidence/` (extensão de Q-11).
- **Proposta:** além do LB de Wilson, publicar **ECE** e **Brier** do veredicto `Enforced` face ao
  log; nenhuma confiança publicada sem calibração (base `inferred`).
- **Teste:** ECE decresce com a evidência; determinismo; `policy:confidence` mantém-se no `check`.
- **Adoção:** baixo risco (auditoria pura, zero tokens de prompt).
- **Resultado ✔:** `katu_policy::calibrate` (previsto = LB, desfecho = frequência empírica; 10 baldes
  de 100 milésimos; determinístico, sem RNG) + `Calibration`/`CalibrationBin`; o `policy:confidence`
  passa a imprimir `ECE`/`Brier`. No registo perfeito o ECE desce **730‰ (n = 1) → 83‰ (n = 30)**;
  na amostra mista `ECE = 98‰`, `Brier = 18‰` (195 ensaios). `bench/e18/confidence/raw.json` ganhou
  `calibration` + `calibration_by_n`; `bench/published.toml` publica-os com base `inferred`. Testes:
  `the_ece_decreases_as_the_evidence_accumulates`, `a_violation_worsens_the_calibration`,
  `the_calibration_is_a_pure_function_of_the_verdicts`, `the_reliability_bins_partition_the_trials`.

**W8-3 · C1 · Parada opcional válida (e-values / anytime-valid).** ✔ feito
- **Onde:** `kernel::guard` (F7); artefacto `bench/e18/loop/`.
- **Proposta:** e-value que torna a rejeição válida a qualquer `n`, mantendo o CUSUM como sinal; o
  corte continua `agent.loop`.
- **Teste:** falso-positivo sob parada opcional; determinismo; alarme ≤ teto.
- **Adoção:** zero falso positivo e cobertura a qualquer `n` (senão mantém-se o SPRT).
- **Resultado ✔:** a razão de verosimilhança `Λ_n` (log-LR) é um **martingale não-negativo** sob
  `H0`; o corte passou a ser `log(1/α)` (limiar de Ville) em vez do `log((1−β)/α)` do SPRT, que só
  garante `α` a `n` fixo. Por **Ville**, `P_{H0}(∃n: log Λ_n ≥ log(1/α)) ≤ α` para **qualquer** regra
  de parada — verificado **exatamente** por DP determinística sobre `(passos, repetições)` (sem RNG,
  sem simulação): `4‰ ≤ α = 10‰` em 12 passos, e a fronteira nominal do SPRT (`6‰`) não tem a
  garantia. O CUSUM mantém-se como sinal para a repetição parcial (onde a e-value não avança). O
  alarme num ciclo puro passou do 4.º para o **5.º** passo (o preço da garantia), ainda **7 passos**
  antes do teto de 12. `bench/e18/loop/raw.json` regenerado com `e_value` + `sprt_nominal`;
  `bench/published.toml` publica o erro sob parada opcional. Testes: `the_e_value_covers_any_stopping_time`,
  `the_nominal_sprt_boundary_is_less_conservative`, `the_optional_stopping_error_is_deterministic`,
  `the_error_grows_with_the_horizon`.

**W8-4 · A3 · VOI para tool calls.** ✔ feito (decisão escrita)
- **Onde:** gate de tool em `agent`.
- **Proposta:** regra determinística "não chamar quando o valor esperado < custo", **nunca** saltando
  o irreconstruível. Proxy: tool calls evitadas em cenários canónicos.
- **Adoção:** só com A/B; sem modelo local que emita tool calls nativas, fica como **decisão
  escrita** (precedente Q-02b/Q-03).
- **Resultado ✔:** `agent::turn::voi` — `Voi::decide` mede a informação marginal por impressão
  (FNV-1a de nome + argumentos canónicos): uma só-leitura já satisfeita (sem mutação desde então)
  tem `VOI = 0 < custo` ⇒ **Skip**; o irreconstruível (`write`/`edit`/`move`/`trash`/`bash`/
  `plan`/`memory`) **nunca** é saltado, e a mutação **invalida** a informação cacheada (conservador:
  re-executa em vez de servir obsoleto). Opt-in `behavior.tool_voi` (default **off** até A/B);
  ligado em `drive` antes de `run_calls`, com um `ToolCall`/`ToolResult` sintético para o modelo.
  Artefacto `bench/e18/voi/raw.json`: 5 cenários canónicos, **2 de 11** chamadas evitadas, **0**
  irreconstruíveis saltados. Testes: `a_duplicate_read_is_skipped`, `distinct_reads_are_executed`,
  `an_irreconstructible_call_is_never_skipped`, `a_mutation_invalidates_the_cached_read`,
  `the_decisions_are_deterministic`, `the_gate_skips_a_duplicate_read_when_enabled`,
  `the_gate_is_off_by_default`, `the_gate_avoids_duplicates_and_keeps_the_irreconstructible`.

**A4** mantém-se **rejeitado** (§Q-18, 14,0 %); não reabrir sem sinal novo.

**W9-1 · B-03 · Erro tipado que ensina.** ✔ feito
- **Onde:** `error`/`model` (projeções model-facing).
- **Proposta:** `ToolOutcome` com `fix` + `toolName`; teste de "tentativas cegas".
- **Resultado ✔:** `ToolOutcome::fix()` devolve o remédio — para `Denied` é o `evidence.remedy`
  (Q-08), para `Unavailable` é derivado do controlo (`approval` → "forneça uma aprovação humana…",
  `budget` → "o orçamento esgotou…"). O `summary()` inclui o `fix` (`"denied <regra> <arg> —
  fix: <remédio>"`), e o `Message::ToolResult` carrega o `tool_name` (projetado do `ToolCall`
  correlacionado) para o erro ser **auto-contido**. O `to_value()` TOON também inclui o `fix`.
  Testes: `denied_fix_comes_from_the_evidence_remedy`, `denied_fix_is_absent_when_the_rule_declares_no_remedy`,
  `unavailable_fix_teaches_the_control_path`, `success_outcomes_have_no_fix`,
  `denied_summary_includes_the_fix`, `denied_summary_without_remedy_has_no_fix_suffix`,
  `tool_result_summary_includes_the_tool_name`, `tool_result_without_tool_name_falls_back_to_the_summary`,
  `tool_result_carries_the_tool_name_from_the_call`, `a_blind_retry_gets_a_fix_that_points_elsewhere`.

**W9-2 · B-04 · Output ledger unificado.** ✔ feito
- **Onde:** `feedback` (Ledger) + `exec` (spill).
- **Proposta:** um só mecanismo head/tail + *spill* com ponteiro; artefacto `bench/e18/ledger/`.
- **Resultado ✔:** `feedback::Ledger` (H=2048, T=2048, S=8192) substitui a truncagem ad-hoc
  (`tail()`) no `exec`: o texto model-visible é head + marcador + cauda, e acima de S o output
  inteiro é vertido para `.katu/spill/<id>.<stream>` com um ponteiro. O `CommandRecord` ganhou
  `stdout_spill`/`stderr_spill`; o `report()` inclui os ponteiros. Artefacto `bench/e18/ledger/raw.json`:
  3 cenários, 26 008 → 8 284 B model-visible (31,8 %), teto ~4 153 B independentemente do input.
  Testes: `ledger_keeps_the_full_text_when_it_fits`, `ledger_keeps_head_and_tail_with_a_marker`,
  `ledger_spill_adds_a_pointer_when_the_output_exceeds_the_threshold`,
  `ledger_without_spill_path_has_no_pointer`, `ledger_respects_char_boundaries_in_head_and_tail`,
  `large_output_is_spilled_with_a_pointer`, `small_output_is_not_spilled`.

### W9 · Anexo B — Camada 1 residual e Camada 2

- **W9-1 · B-03 · Erro tipado que ensina.** ✔ feito. `ToolOutcome::fix()` (remédio da evidência
  ou do controlo) + `tool_name` no `Message::ToolResult`; o `summary()` inclui o `fix` — a negação
  ensina a corrigir-se. Testes: `denied_fix_comes_from_the_evidence_remedy`,
  `unavailable_fix_teaches_the_control_path`, `denied_summary_includes_the_fix`,
  `tool_result_summary_includes_the_tool_name`, `a_blind_retry_gets_a_fix_that_points_elsewhere`.
- **W9-2 · B-04 · Output ledger unificado.** ✔ feito. `feedback::Ledger` (head/tail + spill com
  ponteiro); `exec` spill para `.katu/spill/`; artefacto `bench/e18/ledger/` (26 008 → 8 284 B,
  31,8 %). Testes: `ledger_keeps_head_and_tail_with_a_marker`,
  `ledger_spill_adds_a_pointer_when_the_output_exceeds_the_threshold`,
  `large_output_is_spilled_with_a_pointer`, `small_output_is_not_spilled`.

**W9-3 · B-06 · Escalação de sandbox one-shot.** ✔ feito
- **Onde:** `kernel::session::approval` + `kernel::step` + `agent::turn::retry_with_approval`.
- **Proposta:** alargamento estrito com justificação e aprovação não herdada; teste de
  falso-negativo e de não-reuso.
- **Resultado ✔:** o par de eventos `ApprovalGranted`/`ApprovalRevoked` (B-06) torna a
  aprovação de escalação de sandbox **one-shot**: `session.approve()` concede a capacidade mínima
  (com justificação obrigatória), e `session.revoke_approval()` remove-a depois de usada. Ligado
  em `retry_with_approval` — depois do retry, a capacidade é revogada. A revogação é **persistente**
  (sobrevive ao replay). Artefacto `bench/e18/approval/raw.json`: 2 cenários, 2 critérios cumpridos.
  Testes: `one_shot_approval_is_revoked_after_use`, `one_shot_approval_does_not_survive_replay`,
  `a_second_escalation_requires_a_new_approval`, `approval_revoked_removes_the_capability`.

**W9-4 · B-07 · Contrato "só o delta" no prime.** ✔ feito (decisão escrita)
- **Onde:** `context::prime` (o `FOOTER` do prime compacto e `--long`).
- **Proposta:** uma linha no `render` — **model-visible**, logo exige bump de `PRIME_VERSION` e
  de `gate:prompt`; **só com A/B** do custo em tokens.
- **Resultado ✔:** a linha `"extrai so o necessario, nao copies o output inteiro"` foi adicionada ao
  `FOOTER` (compacto e `--long`); `PRIME_VERSION` 3→4; `gate:prompt` verde (1393 B ≤ 1400 B,
  384 tok). O artefacto `bench/e18/prompt/raw.json` foi regenerado. A **adoção** (manter a linha)
  exige **A/B** do custo em tokens com o modelo — se o A/B mostrar que o custo é alto demais, a
  linha é revertida (o `PRIME_VERSION` bump é o mecanismo de reversão: basta remover a linha e
  incrementar de novo). Teste: `prime_teaches_the_delta_only_contract`.

**W9-5 · B-08 · Modo `batch` declarativo (ADR).** ✔ feito (ADR 0026)
- **Onde:** ADR 0026 (`docs/adr/0026-modo-batch-declarativo-manter-loop-nativo.md`).
- **Proposta:** decidir por ADR se compensa capturar ~80 % do PTC sem motor JS.
- **Resultado ✔:** decisão de **manter o loop nativo** (B-01/B-02). O ganho de ~80 % do PTC é
  **alegado, não medido** (o Anexo B §B.3 regista que a própria nota do PTC admite "não há
  garantia incondicional de poupança"); o custo é um novo DSL + executor + proptest de
  determinismo; e o loop nativo já cobre o caso comum (lote B-01/B-02, **+76,3 %**). As
  condições para revisitar são explícitas: medição ≥ 20 %, proptest de determinismo, não
  Turing-completo, sem dep nova. O ADR 0026 regista a decisão e as alternativas consideradas.

**W10 · D2/D3 · Auditoria e aprovações.** ✔ feito
- **D2 (tamper detection):** cadeia de hash nos segmentos de auditoria — `SegmentInfo` ganhou
  `hash` (FNV-1a do conteúdo `.rec`) e `prev_hash` (hash do segmento anterior). `verify()`
  deteta adulteração de conteúdo e remoção de segmentos (`AuditError::Tamper`). Artefacto
  `bench/e18/audit/raw.json`: 2 cenários, 2 critérios cumpridos. Testes:
  `verify_detects_tampered_segment`, `verify_detects_removed_segment`.
- **D3 (MAC de aprovações):** `ApprovalGranted` ganhou `signature` (FNV-1a do conteúdo + chave
  `audit.mac_key`). Sem chave, a aprovação é recusada (fail-closed, `RefusalReason::MissingMacKey`).
  A chave vem da config `audit.mac_key` (via `defaults::from_root`). Testes:
  `approval_without_mac_key_is_refused`, `approval_with_mac_key_is_accepted`.
- **C7 (estatística robusta):** ✔ feito. `Summary` ganhou `mad` (Desvio Absoluto Mediano) e
  `robust_ci95_low`/`robust_ci95_high` (IC 95 % robusto: `mediana ± 1,96·MAD/√n`). O MAD é
  resistente a outliers; a média não. Artefacto `bench/e18/stats/raw.json`: 4 critérios, 4
  cumpridos. Testes: `mad_is_robust_to_outliers`, `the_robust_interval_contains_the_median`,
  `the_robust_interval_is_stable_under_outliers`, `empty_and_single_samples_have_zero_mad`.
- **W9-4 · B-07 · Contrato "só o delta" no prime.** ✔ feito (decisão escrita). A linha
  `"extrai so o necessario, nao copies o output inteiro"` foi adicionada ao `FOOTER` do prime
  (compacto e `--long`); `PRIME_VERSION` 3→4; `gate:prompt` verde (1393 B ≤ 1400 B). A **adoção**
  (manter a linha) exige **A/B** do custo em tokens com o modelo — se o A/B mostrar que o custo
  é alto demais, a linha é revertida. Teste: `prime_teaches_the_delta_only_contract`.
- **W9-5 · B-08 · Modo `batch` declarativo (ADR).** ✔ feito (ADR 0026). Decisão: **manter o
  loop nativo** (B-01/B-02). O ganho de ~80 % do PTC é alegado, não medido; o custo é um novo
  DSL + executor + proptest. Condições para revisitar: medição ≥ 20 %, proptest de
  determinismo, não Turing-completo, sem dep nova.
- **B-05** (par de eventos de sub-chamada) só faz sentido **dentro** de B-08; não antecipar.
- **B-09** (runtime real) e **B-10** (background jobs) mantêm-se rejeitados/adiados com o número —
  B-09 contradiz o binário único/G7.

### W10 · Anexo A — Tier 2/3 e resíduos do E18

- **A1/A2 (Tier 2):** ✔ feito (A1 adotado, **DPP rejeitado com o número**). O proxy de Q-02b/Q-03
  passa a ser um **contrato de distorção** e não um ganho: `R = tokens(S)/tokens(U)`,
  `D = 1 − I_ret(S)/I_ret(U)`, `D0 = D(sufixo)` na mesma taxa. Medido em `bench/e18/select/`
  (`raw.json` → `a1_rate_distortion`): a `R = 109‰` a distorção é **0 ‰** contra `D0 = 356‰` do
  histórico a `R = 804‰`. A2 mede a diversidade com o mesmo critério de aceitação do Anexo A
  (nenhum par acima de `sim_max`): utilidade **17 ‰** média / **500 ‰** máximo contra **624 ‰** /
  **1000 ‰** do histórico (que viola o teto). O **DPP é rejeitado**: há 81 trocas que aumentam a
  informação (ganho máximo 12 584 µ), mas são de **utilidade** e não de redundância — o MMR já
  resolve a diversidade, logo o determinante não tem o que maximizar.
- **D1 (Tier 2):** ✔ feito. *Taint*/*spotlighting* do output de tool antes do modelo
  ([`katu-core/src/taint.rs`](../../../crates/katu-core/src/taint.rs)): o delta viaja entre
  `<katu:untrusted …>`/`</katu:untrusted>`, com `escape` a neutralizar (`<` → `[`) qualquer `<` que
  forme `katu:`/`/katu:` **sem alterar o comprimento** (o teto `MAX_DELTA_BYTES` continua exato) e
  `inspect` como teste estrutural. O embrulho fica em `ToolReport::to_delta` — não no encoder de
  cada provider — para não partir `Model-visible ⟺ logged`; o *prime* (`PRIME_VERSION` 4 → 5)
  ensina o modelo a tratar as tags como dado. Artefacto `bench/e18/taint/`: **6/6 ataques da
  suíte red-team bloqueados**, custo **71 B** por resultado de tool (0,87 % de um delta de 8 KiB).
  Limite declarado: mede escape *estrutural*, não obediência do modelo.
- **C5 (Tier 2):** ✔ feito. Benjamini–Hochberg na família de regras `Enforced`
  ([`confidence.rs`](../../../crates/katu-policy/src/confidence.rs)): p-value **exacto** unilateral
  (`H0: p ≥ θ`, cauda binomial superior `P[X ≥ s | n, θ]`, em micro) por regra e BH a `q = 5 %`
  (`Threshold.q_milli`); `control_fdr` só pode **demover** uma promoção (fail-closed). Fecha a
  lacuna declarada em Q-11 ("LB unilateral sem controlo de múltiplas comparações"). Medido em
  `bench/e18/confidence/` (`raw.json` → `fdr`): **40 regras com 25 honras cada** — o limiar sozinho
  promove as 40, o BH **nenhuma** (`p = 0,0718 > 0,05`); nas mesma log com `n = 29` promove as 40.
  Preço escrito: **provar `Enforced` passa de `n = 25` a `n = 29`** (`0,9ⁿ ≤ 0,05` pede 28,43) —
  mais disciplina, nunca mais confiança.
- **C7 (Tier 2):** ✔ feito. Estatística robusta (mediana/MAD, IC robusto) em
  [`stats.rs`](../../../crates/katu-core/src/stats.rs); artefacto `bench/e18/stats/`.
- **C2 (Tier 3):** ✔ medido; **rejeitado para adoção** com o número — e **sem código em `src/`**.
  A fórmula do split conformal (`k = ⌈(n+1)·nível⌉`, quantil dos resíduos, *fail-closed* quando
  `k > n`) mora no bench que a mediu
  ([`stats/tests/conformal_bench.rs`](../../../crates/katu-core/src/stats/tests/conformal_bench.rs)), porque
  um item rejeitado deixa o **número**, não uma API pública sem consumidores (a `stats::conformal`
  que existia foi removida nesta revisão). Medido em `bench/e18/conformal/`, série sintética
  SplitMix64 com forecast rolante de 10: no regime **trocável** só **5 de 8** linhas publicadas
  atingem o nominal — a 95 % a cobertura fica em **913–947 ‰** contra as 950 ‰ prometidas (pior
  queda **37 ‰**). Achado que reformula o item: a diferença entre o regime trocável e o
  não-trocável é **≤ 12 ‰**, logo **a troca não é o que falha** — a independência dos resíduos é.
  Segunda condição, hoje decisiva: o `log` tem **0 ensaios** por regra (`n_cal ≥ 19` necessárias).
  Só entra em `src/` com base real não correlacionada e a varredura repetida.
- **E1 (Tier 3):** ✔ medido, **sem mudança** (o número é do ambiente, não do pool). Curva USL do
  mecanismo `spawn`/`join` do lote (mesmo de `in_parallel`), `N = 64` tarefas, mediana de 7, escada
  `T ∈ {1,2,3,4,6,8}`: `S(T) = 0,94 / 0,93 / 0,92 / 0,94 / 0,88 / 0,85`. A máquina **não escala**
  (2 threads não dão 1,5× apesar de `available_parallelism = 16`), pelo que o artefacto marca
  `scaling_suspect: true` e o número descreve o cgroup, não o katu. O teto de 8 é um limite de
  **custo** (PTC), não de paralelismo: fica como está, e o invariante que se trava é a cobertura da
  curva (`the_production_ceiling_is_inside_the_measured_ladder`). Condição para rever: repetir o A/B
  numa máquina com `S(2) ≥ 1,5×`; aí o teto passa a `min(8, available_parallelism())`.
  Artefacto `bench/e18/pool/`.
- **D2/D3:** ✔ feito. **D2:** cadeia de hash nos segmentos de auditoria (`hash` + `prev_hash`);
  `verify()` deteta adulteração e remoção. **D3:** MAC das aprovações (chave `audit.mac_key`,
  fail-closed sem ela). Artefactos `bench/e18/audit/`.
- **E18-T08 (PERT/CPM): ✂ rejeitado com o número.** O plano é uma **lista sequencial por
  invariante** ([`plan.rs`](../../../crates/katu-core/src/plan.rs): `feature_list` sem arestas de
  dependência, e `validate()` recusa mais de uma feature `in_progress` — `MultipleInProgress`, testado
  em `multiple_in_progress_is_rejected`). Com `0` arestas e largura 1, o caminho crítico **é** a
  lista: o CPM devolveria a ordem atual, e o `L(id)` memoizado não teria o que memorizar. Para
  reavaliar é preciso primeiro **criar** o DAG (dependências explícitas entre features), que é uma
  mudança de schema e de produto, não uma otimização.
- **E18-T09 (PPR semeado pelo working set): ✂ rejeitado com o número.** A parte de fusão **está
  feita** (RRF com `k = 60` em Q-02b, `SelectionParams::k_rrf`); a parte nova é o PPR, que exige um
  grafo de ligações entre memórias. Hoje [`Memory::search`](../../../crates/katu-core/src/memory.rs) devolve
  um **ranking plano** (`Vec<RecallHit>` com `score`), sem arestas: `0` arestas ⇒ Personalized
  PageRank não tem grafo para percorrer, e semeá-lo pelo working set seria reranking de uma lista
  (que o RRF já faz, com pesos versionados). Reavaliar quando existir uma frente que crie ligações
  explícitas entre notas.
- **E18-T05 (estado persistente e checkpoint): ✔ feito em Q-15; a parte restante rejeitada com o
  número.** `checkpoint = snapshot + Δ` com cauda limitada (128 KiB), hash canónico do estado e
  `state_of(replay) == state_at_end` já estão medidos em [`bench/e18/resume`](../../../bench/e18/resume/PROTOCOL.md):
  20 000 turnos retomam em **321 µs** contra **23 839 µs** do replay total (**−98,7 %**) e
  2 165 µs da política anterior por fase (**−85,2 %**). O que resta do entregável é a **partilha
  estrutural** do `State`, e o custo restante da retomada (321 µs) é a leitura da cauda (61 812 B)
  mais a leitura do snapshot — a cópia do estado é um `memcpy` de alguns KiB dentro desses 321 µs,
  ou seja, abaixo do ruído de medição. Sem número a retirar, fica **rejeitada**; se a retomada
  voltar a ser um gargalo (≥ 1 ms), mede-se de novo com o mesmo harness.

### Resumo W7–W10

| Onda | Itens | Fonte | Depende de |
|---|---|---|---|
| **W7** método | harness zero-dep (E18-T10) · regerar atribuição · S-04 | Q-09 residual | — |
| **W8** Anexo A T1 | B1 gramática · C3 calibração · C1 e-values · A3 VOI | Anexo A | W7 |
| **W9** Anexo B | B-03 · B-04 · B-06 · B-07 (gated) · ADR B-08 | Anexo B C1/C2 | W7 |
| **W10** Anexo A T2/T3 | A1/A2 · D1 · C2/C7/C5 · E1 · D2/D3 · E18-T05/T08/T09 | Anexo A + E18 | W8 |

**Regra de entrada em W8:** o harness de W7 está instalado (a lição de W1), logo as frentes novas
medem contra ele. Cada item fecha com artefacto cru, teste que o trava (`make check` + `msrv`) e a
decisão adotar-ou-reverter escrita — adotado com o número, ou rejeitado **com** o número.

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
| A4 | **Retrieval semântico de tools (embedding top-k)** | [`agent/catalog.rs`](../../../crates/katu/src/agent/catalog.rs) | tokens do catálogo, taxa de tool errada | top-k contém sempre a tool correta no conjunto de teste |
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
| D2 | **Proveniência Merkle + transparency log** (`audit.seal` já no catálogo) | [`audit/`](../../../crates/katu-core/src/audit/mod.rs) | deteção de adulteração | selo recomputável; tamper detetado |
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

**Estado (após W6):** **A4** rejeitado (§Q-18); **A1/A2** têm um proxy adotado em Q-02b/Q-03 (falta
a formalização com `D ≤ D0`, W10); os restantes entram em **W8** (Tier 1) e **W10** (Tier 2/3)
conforme o §10.

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
| B-01 ✔ | **Concorrência classificada por tool + pool limitado** | `agent/turn.rs` hoje corre `for ... in calls` **sequencial**; introduzir `is_concurrency_safe` fail-closed (default exclusivo) e pool | wall-clock de N leituras independentes; A/B |
| B-02 ✔ | **Vários tool calls por passo, com ordem de commit determinística** (já recebemos `step.calls`) | idem | turnos/tarefa |
| B-03 | **Erro tipado que ensina**: `ToolCallError` ↔ enriquecer `ToolOutcome` com `fix`/`toolName` (liga a Q-08) | [`error/mod.rs`](../../../crates/katu-core/src/error/mod.rs) | tentativas cegas ↓ |
| B-04 | **Output ledger unificado** (head/tail + spill) em vez de truncagens ad-hoc | `feedback.rs` + TOON | bytes de output, recuperação |
| B-05 | **Par de eventos de sub-chamada** no catálogo diag (`tool.dispatch.start`/`tool.dispatch.settle`) | `events.rs` | cobertura de instrumentação |
| B-06 | **Escalação de sandbox com justificação + aprovação one-shot** (alargamento estrito) | `approval`/E07 | aprovações forjáveis ↓; falso-negativo |
| B-07 | **Só o delta/devolução reentra no contexto** — tornar explícito no prime o contrato "extrai só o necessário" | `prime` (Q-04) | tokens de turno |

**Estado:** **B-01/B-02 ✔** (lote `Shared` com pool limitado; `bench/e18/batch/`, **+76,3 %**); os
restantes (B-03..B-07) entram em **W9** (§10).

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
já tem o pipeline (`kernel/pipeline`, `tool.rs`) e a política fail-closed; **já executou** as tool
calls de um passo com concorrência classificada (**B-01/B-02 ✔**, `bench/e18/batch/`) e falta-lhe
(a) os contratos tipados de **B-03..B-07** (W9) e (b) decidir, por ADR, se quer um **modo `batch`
declarativo** (**B-08**, W9) — sempre sem alterar G3.
