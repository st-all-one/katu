# STAGING — estado e pendências do katu

> Documento **operacional e volátil**: retrato do que está feito, do que falta e do que bloqueia,
> frente ao [`plan/`](plan/) e ao [`IMPLEMENTATION_PLAN.md`](IMPLEMENTATION_PLAN.md). Não substitui
> os épicos: cada item aponta para a tarefa. Atualizar quando um épico fecha (não é um contrato).

## 0. Snapshot

- **6 crates + `xtask`**: `katu-policy`, `katu-core`, `katu-tools`, `katu` (bin), `katu-providers`, `katu-tui`*.
- **685 testes** · catálogo de instrumentação **86 ids** · **11 tools** · **8 regras** (5 memória + 3 contenção) · **23 ADRs**.
- `cargo xtask check` (= `make check`) verde: fmt + clippy `-D warnings` + testes + `check-layers` + `check-diag` + `check-schemas` + `check-docs` + `check-surface` + `check-policy` + `check-unsafe` + `check-catalog` + `check-rule-coverage` + `check-slices` + `check-memory-swap` + `policy:audit` + `gate:bench` + `gate:provider` + `gate:render` + file-length ≤300 · `make instrument` verde.
- **O MVK passou** ([ADR 0001](docs/adr/0001-mvk-gate-aprovado.md)); o kernel (E04) e a política (E02) estão completos.
- **E20 — Superfície v2** (reforma do CLI/TUI inspirada no `kd`): **S0+S1+S2** — [ADR 0019](docs/adr/0019-superficie-cli-tui-v2.md)/[0020](docs/adr/0020-configuracao-global-local.md)/[0021](docs/adr/0021-layout-katu-e-versionamento.md), verbos exclusivos (`prime`/`upgrade`/`config`/`memo`/`run`/`tui`), `memo` só consulta, `prime` estático por grupo, body/stdin, `--json` por comando, `--log-level` global, config global/local fechada, `--params`/`--batch` (XOR) em `run`/`memo ask`, bootstrap `.katu/` com versionamento e snapshot da config, `memo doctor --fix`, e `run` com id da sessão + `round_exit`; **T04 ☑** (e2e live com `llama-server`: o id devolvido retoma e id desconhecido → exit 2), **T20 ☑** (`--digest [--force]` real pelo `knudge-core` + `--watch-service` com timer systemd `--user` e script embutido; fail-closed sem `systemctl`), **T10 ☑ / T15 ☑** (TUI v2: comandos `/` com mini-menus adaptados às capacidades + ajuda `?`; `Esc` como único cancelamento), **T14 ☑** (cópia por seleção de rato via OSC 52, sem dependência), **T16 ☑** (*steering* FIFO: input não bloqueante durante o turno aplicado no passo seguinte; diag `tui.steer`), **T11 ☑** (modo de planeamento `/plan`: regra `deny_write_outside` “escrita só sob `.katu/`” + `plan-no-shell`, artefacto `.katu/plan/<UTC>.md`; [ADR 0022](docs/adr/0022-modo-plano-e-deny-write-outside.md), vocabulário de política **v3**), **T12 ☑** (`@<path>` cita caminhos; `!<cmd>` corre `sh -c` pela política e é recusado no `/plan` com evidência), **T13 ☑** (contexto do projeto: `AGENTS.md` no topo do prompt + catálogo de skills `.agents/skill{,s}/*/SKILL.md` e `/skill:<nome>`; [ADR 0023](docs/adr/0023-contexto-e-duas-ias.md)), **T17 ☑** (padrões `provider`/`model`/`base`/`thinking` ligados a `run`/`tui`; **duas IAs**: embeddings externos por `embeddings.url`/`model`, `off` se ausente, projetados em runtime no knudge; `memo doctor` publica o estado), **T06 ☑** (paridade `memo`/`kd`: `memo ask` cobre as flags do `kd ask` — filtros, `--rank`/`--tags`/`--suggest`, `--id`/`--around` — e `memo knowledge` é o mapa estrutural real; porta `Memory::query`; `--semantic`/`--communities`/`--write` e a semântica temporal adiados), `--params`/`--batch` em `prime`/`tui`, e **modelo local recomendado** Qwen2.5-Coder-1.5B Q4_K_M (`scripts/llama.sh fetch|serve|chat`); épico em [`SURFACE_IMPLEMENTATION.md`](SURFACE_IMPLEMENTATION.md).

\* `katu-tui` traz a base E10 (T01/T02/T03◐/T04/T05/T06/T07): esqueleto panic-safe, keymap puro,
render com throttle/teto de trabalho, painel de atividade efémero, recusas com regra + evidência,
seletor de modelo/pensamento (`m`/`t`), vista da lixeira (`l`), compactação efetiva (`c`) e gate de
verificação (`v`, com override por challenge);
`katu-providers` implementa a porta `Provider` e os built-in `opencode go/zen` + `llama.cpp` (ADR 0011).

## 1. O que já funciona

- Kernel event-sourced: `step` puro, log append-only, `derive_messages`/`state_of`/`snapshot`, `Session::verify()` (`Model-visible ⟺ logged`).
- Política pura (`evaluate`), vocabulário fechado v2, ledger de cobertura, auditoria.
- Gate de memória pelo caminho real (recall → write → close) e cost governor ligado ao `Session`.
- Tools: read (6 views), write, edit (CAS + dry-run + `Stale`), move, trash, bash (scrub + timeout), grep/find/ls, plan, memory (record + recall). **Linter de schema** (`katu-tools::schema`) imposto por `xtask check-schemas`.
- Contexto: `assemble` + prime (+`--long`), compactação determinística opt-in, gate de verificação, scope contracts.
- Contenção soft: sensíveis negados, fora do workspace → aprovação, busca como leitura, `Capability::Net`; **symlink resolvido via porta `Fs` antes do veredicto** (`katu-tools::resolve`); o **runtime do agente carrega memória + contenção** e define o workspace (`Runtime::open`), e as recusas chegam à UI com regra + evidência (E10-T04◐).
- Instrumentação transversal zero-custo (DF9): **redação por allowlist** no sink (E01-T07), **fingerprint determinístico** (`fingerprint!`, E19-T04) e **filtro por subsistema** (E19-T06).
- **Saída ao modelo colunar v3** (ADR 0006/0007): **sem headers** no *stream* (o prime v3 é o registo de esquema, re-emitido no início e após compactação), blocos literais (`\x1d`) para código, escalares explícitos em `k`, domínios no registo e **aliases de sessão** `#N`/`@N` (lazy, limiar 3). Projeções model-facing canónicas de `ToolOutcome`/`Error`/`VerificationReport`; digest de compactação em tabela `m` (sem `Debug`); catálogo de tools no prime (`prime_with_catalog`, anti-drift). JSON de máquina inalterado.
- **Sessões e auditoria** (ADR 0008/0009): sessões vinculadas ao projeto em `.katu/sessions/<id>` (id `s_<16hex>` determinístico, índice temporal `(created_ms, id)`, **snapshot de estado com offset** por fase — inclui o histórico temporal `rolling`/`velocity` que o log não reproduz —, retomada que lê só a cauda, `Session::create`/`resume`/`list`); auditoria permanente e pesquisável em `.katu/audit` (**segmentos colunares imutáveis** + **índice invertido binário `KAI1`** com posições e **Bloom** por segmento — termos, frases, filtros), local e em `.git/info/exclude`. A borda expõe `katu sessions` (lista id/instante/objetivo) e `--resume [last|id]` em `run`/`tui`; a retomada **fecha** um turno deixado aberto (processo morto a meio) de forma determinística antes de abrir o seguinte.
- **Memória de primeira classe** (ADR 0010): porta `Memory` com tipos do katu + adaptador in-process sobre `knudge-core` isolado no binário (`src/memory/`); `Runtime::open` (`src/runtime.rs`) monta a sessão com o adaptador e **recusa arrancar** sem memória saudável (fail-closed, E03-T03/T07); `katu remember`/`katu recall` exercitam o caminho §42; gate `check-memory-swap` (E03-T06).
- **Providers** (ADR 0011/0012): porta `Provider` em `katu_core::provider` (o núcleo **não** depende de provider) e adaptadores em `katu-providers` — built-in `opencode go/zen` e `llama-server`; **catálogo `model → dialeto`** e **providers declarativos** (`ProviderSpec` + JSON), com `chat/completions`, `responses` e `messages` (SSE incremental, tool calls completas, `x-opencode-session`, `TCP_NODELAY`, sem compressão); `FakeProvider`/`MockTransport` cobrem o loop sem rede; e2e com chave **auto-*skip***; `xtask provider-smoke` mede TTFT/usage; retry classificado (transitórios vs conta/quota, só antes do 1.º delta, `Retry-After` em segundos/ms/data) e `usage` robusto de cache; **cache de prefixo por modelo medido** (`deepseek-v4.1-flash`, 2.º turno `cached=896/1004`) com afinidade (`x-client-request-id`/`x-session-affinity`); compressão do pedido **rejeitada** pelos endpoints (opencode `401`/llama `415`) e desligada (ADR 0013). **Latência publicada** com artefacto cru (`bench/providers/latency.json`) e **gate de orçamento** (`xtask gate:provider`, ADR 0014).
- **Loop de turnos** (ADR 0015): `katu run` liga o provider ao kernel — pedido montado do log, tool calls logadas antes de executar (§42), resultado de volta ao modelo; roteador **fail-closed** dos argumentos JSON (caminhos canonicalizados antes do veredicto). Verificado e2e contra `llama-server` e `opencode-go` (um tool call real criou o ficheiro). **Contrato de escopo** (E09-T04): `scope_contract.json` + `feature_list.json` da raiz são carregados e validados no arranque (`katu/src/scope.rs`); com o artefacto, a tool `plan` executa e registra o plano (`PlanRecorded` §42), destrancando a fase `Planned`.
- **UI de terminal (E10, base)**: `katu tui` liga a crate `katu-tui` ao loop — estado central, keymap **puro** (`map_key`/`apply_action`) e render puro (E10-T01/T02/T06 ☑); a borda implementa o `Handler` que corre o turno e injeta `Update`s. `Runtime::begin_turn` suporta multi-turno. O **streaming é visível** num painel de atividade efémero (`run_turn_with` + `ActivitySink`, evento `tui.live`), fora do log e do transcript (E10-T05 ☑; a transcrição durável é `<root>/.katu/transcript.md`, vista read-only com `T`). Controlos: `m`/`t` (modelo/pensamento), `l`/`r`/`x` (lixeira: listar/restaurar/esvaziar com challenge), `c` (compactação), `v` (gate de verificação com override, E09-T01/T03/T07) e `T` (transcrição). O cabeçalho mostra a **próxima ação** do checkpoint (`Update::NextAction`, E10-T06) e o render por quadro tem gate próprio (`gate:render`, E15-T01). **Transparência**: o cabeçalho mostra o **uso/custo** do turno (`usage_line`: tokens in/out/cache/think + custo quando há preço; `policy/prices.toml` versionado, vazio = `unpriced`), os **argumentos crus** do modelo aparecem no painel ao lado da tool e **Esc/Ctrl-C cancelam** o turno de forma limpa (`ActivitySink::cancelled` → `Flow::Break`, diag `tui.cancel`).

---

## 2. Gaps por épico

| Tarefa | Estado | Pendência |
|---|---|---|
| **E01-T07** | ☑ | Redação por allowlist no sink (E01-T07); rotação de ficheiro fica deliberadamente gated (nunca apaga automaticamente). |
| **E01-T08** | ☑ | Política de memória e `unsafe` (ADR 0016): `#![forbid(unsafe_code)]` em todos os crates puros; `disallowed_types`/pânico/indexação/`as` negados; `reason` em todo `#[allow]`. Miri fica em E13. |
| **E01-T09** | ☑ | Política de recursos e runtime mínimo (ADR 0017): worker bloqueante por padrão, sem `tokio`; teto de corpo (`GET_BODY_CAP`), timeouts tipados, canais bounded. `spawn_blocking`/async só quando houver executor em background. |
| **E01-T10** | ☑ | `cargo xtask check` (`xtask/src/check.rs`) é o ponto de entrada único: fmt + clippy + testes + file-length + camadas + diag + schemas + docs + memória + política + `gate:bench`/`gate:provider`/`gate:render`; `make check` delega nele. Workflows `pr-fast`/`pr-msrv`/`ci` em camadas. |
| **E03-T02** | ☑ | **Adaptador in-process do `knudge-core`** (primário, no binário; feature default). |
| **E03-T03** | ☑ | `Runtime::open` monta sessão + adaptador + regras; `recall`/`remember` pelo §42 (E10/E12). |
| **E03-T04** | ☑ | `spawn_blocking` + timeout; **resolvido por ADR 0017** como não aplicável até existir caminho async (worker bloqueante por desenho). |
| **E03-T05** | ☑ | Suíte de conformidade corre contra o fake **e** o adaptador in-process. |
| **E03-T06** | ☑ | Gate de substituibilidade (`xtask check-memory-swap` + `make memory-swap`). |
| **E03-T07** | ☑ | `Runtime::open` recusa sem memória saudável (fail-closed); comandos falham com exit 10 sem adaptador. |
| **E06-T02** | ☑ | Linter de schema (`katu-tools::schema`) + `xtask check-schemas` em `make check`. |
| **E06-T03** | ☑ | Matriz multibyte (§45.22); `edit.hunks`/`added`/`removed` reais; chunk único truncado em limite UTF-8. `read.diff` sem `base` é integração CLI (§3.2). |
| **E06-T12** | ☑ | Formato ao modelo **colunar v3** (ADR 0006): sem headers (registo no prime), blocos literais, `k` explícito, aliases de sessão; qualidade (`rank`/`basis`/`ev`/`sym`). A/B: **-21%** vs JSON; aliases neutros no corpus sintético (§3.4). |
| **E06-T07** | ◐ | Rotação do registo de comando (→ E01-T07). |
| **E07-T02** | ☑ | Symlink resolvido via porta `Fs` (`Fs::canonicalize` + `katu-tools::resolve`) antes do veredicto; `StdFs`/`MemFs` com teste de escape. |
| **E07-T03** | ☑ | Gate do épico: autorização ausente fora do workspace recusada; symlink para fora negado na política. |
| **E07-T05** | ☑ | Contenção aplicada no loop (runtime carrega memória + contenção e define o workspace); **aprovação humana interativa** (challenge-and-response, §33): a UI pede, o humano assina (`override_reason`+`granted_by`), o kernel concede a **capacidade mínima** (`katu-policy::capability_for`) e a chamada é re-executada — evento `ApprovalGranted` no log, não herdado por outro alvo. |
| **E09-T03** | ☑ | Gate determinístico sobre os factos do log (`changed_files` relativos, só escritas com sucesso; `recorded_commands`); `Runtime::verify` grava `verification_report.json` e regista `VerificationRecorded`; a TUI (`v`) pede **override humano** por challenge se bloquear e regista-o em `overrides.jsonl` (evento `verify.override`). |
| **E09-T04** | ☑ | Carregar `scope_contract.json`/`feature_list.json` no arranque (`katu/src/scope.rs`): valida o `Plan` (schema + ≤ 1 `in_progress`) antes do turno e liga a tool `plan` ao kernel (`PlanRecorded` §42, `katu/src/agent/plan.rs`) — **destranca a fase `Planned`**; sem artefacto, `plan` mantém `Unavailable{scope-contract}`. |
| **E09-T05** | ◐ | `Metric`/portão de publicação — fecho. |
| **E09-T07** | ☑ | Compactação determinística e opt-in; o **contexto efetivo** do turno vem de `Session::context` (`assemble` puro ou com digest), com o resumo no `system` e o original recuperável no log; TUI `c` liga/desliga e CLI `--compact`. |
| **E10-T01…T07** | ◐ | **CLI/TUI**: T01 (esqueleto panic-safe + restauro), T02 (keymap puro testado por modo), T03◐ (render diferencial + throttle por `Clock` + teto de trabalho; benchmark por quadro feito — `gate:render`; falta zero alocações, E18-T10), T04 (recusas com regra + evidência **e** override por challenge-and-response — `katu-tui::Challenge`), T05☑ (painel de atividade efémero + streaming visível; transcrição durável `.katu/transcript.md` + viewer read-only `T`), T06☑ (fase/pendência + próxima ação do checkpoint no ecrã) e T07☑ (`m`/`t` modelo/pensamento, `l` lixeira com `trash::list`+`restore`, `x` esvaziar com challenge, `c` compactação efetiva, `v` gate de verificação) feitos em `katu-tui` + `katu tui`. O executor em background fica por fazer. |
| **E12-T01** | ☑ | Porta no núcleo + adaptadores `opencode`/`llama` com catálogo `model → dialeto`; o binário **liga o loop** (`katu run`, E12-T05). |
| **E12-T02** | ☑ | Formato declarativo próprio (`ProviderSpec`+JSON) e `Declarative<T>`; GDK `goose` rejeitado (ADR 0012); `Provider::models()` (catálogo) e `Provider::dynamic_models()` (descoberta **ao vivo** do endpoint, leitura defensiva em `models.rs`), com queda no catálogo; a lista da TUI vem daí. **E12-T03 ☑** tier pela política (`Tier`, `ModelEntry.tier`, `Catalog::select_tier`, `Provider::model_for_tier`, `policy/tiers.toml`, diag `provider.tier`). |
| **E12-T03** | ☑ | `TokenUsage`/`PriceTable` (base `provider_reported`, `unpriced`) e ligação ao **`Metric`** (`Cost::metric`/`usage_metrics`, com `Unit::Micros`/`Tokens`); **tier** pela política (`Tier`, `ModelEntry.tier`, `Catalog::select_tier`, `Provider::model_for_tier`, `policy/tiers.toml`); **uso/custo** visível no cabeçalho da TUI (`usage_line`, `policy/prices.toml`). |
| **E12-T04** | ☑ | Retry classificado (transitório vs conta/quota) só antes do 1.º delta, honrando `Retry-After`; timeout + cancelamento. |
| **E12-T05** | ☑ | **Loop de turnos** ligado ao kernel com tool execution §42 (`katu run`; roteador fail-closed em `src/agent/`; ADR 0015); testes de loop sem rede e e2e real verificado. |
| **E12-T06** | ◐ | `chat/completions`+`responses`+`messages`+`google` (`models/<id>:streamGenerateContent`) pelo mesmo `wire`; smoke **ao vivo** por dialeto e `dynamic_models` (auto-*skip*); faltam WebSocket/HTTP2. |
| **E12-T07** | ◐ | Artefacto cru (`bench/providers/latency.json`, offline+live) e **gate de orçamento** (`xtask gate:provider` em `make check`, ADR 0014); falta `criterion`/`dhat` e a matriz por dialeto/transporte (E18-T10). |
| **E12-T08** | ☑ | `llama-server` (L1) pelo mesmo trait; smoke real com Qwen2.5-Coder-1.5B Q4_K_M. |
| **E12-T10** | ☑ | `Control::{SetModel, SetThinking}` no kernel (`kernel::control`): `Event::Control` no log (audit `kind=control`), `ControlState` no `State` (sobrevive a *resume*) e validação pura com erro que **ensina** (`ControlError::ReasoningUnsupported`), usando `ModelCapabilities` do catálogo (`Provider::capabilities`); a borda (TUI `m`/`t`) valida e regista via `Runtime::set_control`; o agente nunca emite `Control`. |
| **E13-T01…T07** | ◐ | **E13-T01 ◐**: alvos `cargo xtask test:{unit,integration,e2e,all}`; property (`proptest` em TOON e `parse_models`); falta fuzz. **E13-T02 ☑**: `coverage.toml` + `check-rule-coverage` (matriz regra↔teste total). **E13-T04 ☑**: CI `miri` + `hygiene`. **E13-T05 ☑**: golden regenerável (`KATU_GEN_TEST_DATA=1`). **E13-T07 ☑**: invariantes em `session/tests/invariants.rs`. Falta E13-T03 (regressão invertida) e E13-T06 (matriz de aceitação). |
| **E14-T02…T07** | ◐ | **E14-T02 ☑** (postmortems + `check-docs` exige guardrail), **E14-T03 ☑** (router `AGENTS.md`), **E14-T04 ☑** (`check-policy`), **E14-T05 ☑** (`check-surface` + `surface.toml`), **E14-T06 ☑** (`check-catalog` gera `docs/catalog.md`), **E14-T07 ☑** (`check-slices`). |
| **E15-T01** | ◐ | `criterion` + gate de performance no CI. Feito: harnesses **zero-dep** + gates de **render** (`gate:render`, E10-T03) e **provider** (`gate:provider`, E12-T07), em `make check` e no CI; `criterion` preterido (consistente com E19-T02). Falta o micro-bench de seleção de regras e heap/escala (§44). |
| **E15-T03…T06** | ☐ | Tabela de recuo, determinismo de prefixo, instrumentação do prefixo (`✂`), negativos. |
| **E18-T01…T10** | ◐ | F1 (determinismo) é contrato transversal; **F4 parcial via E12-T07** (TTFT/cache/gate); F2/F3/F5–F10 por iniciar (fórmula + artefacto + teste, plan/19 §0.3). |
| **E19-T03** | ◐ | Instrumentar o caminho crítico por épico (cresce com o código). |
| **E19-T04** | ☑ | Fingerprint determinístico (`diag::fingerprint!`) com golden. |
| **E19-T06** | ☑ | Filtro de nível por subsistema (`KATU_INSTRUMENT_FILTER`/`set_filter`). |
| **E19-T05** | ◐ | Gate no CI (fecho). |
| **E08 · E11 · E17** | ⏸️ | Futuro (MCP, plugins WASM, jail de SO). |

---

## 3. Gaps por natureza

### 3.1 Bloqueadores (impedem a experiência completa)

1. **TUI (E10) — base feita; faltam fechos.** `katu tui` abre uma UI panic-safe com keymap puro,
   multi-turno síncrono, **streaming visível**, **recusas com regra + evidência**, **aprovação por
   challenge-and-response**, **orçamento de render** (throttle por `Clock` + teto de trabalho),
   **seletor de modelo/pensamento** (`m`/`t`), **vista da lixeira** (`l`, `trash::list`+`restore`),
   **compactação efetiva** (`c`) e **gate de verificação** (`v`; override por challenge)
   (`katu-tui`, T01/T02/T03◐/T04/T05/T06/T07). Faltam o executor em background e a verificação de
   zero alocações no render (E18-T10).
2. **`spawn_blocking`+timeout (E03-T04).** O adaptador in-process está ligado e o loop usa a tool
   `memory` (§42); o timeout do worker bloqueante fica para quando existir caminho async (ADR 0017;
   E03-T04 não aplicável).

### 3.2 Integração CLI/TUI (lógica já feita no core)

- `read view=diff` precisa que o chamador forneça o `base` (checkpoint/leitura anterior).

### 3.3 From-zero ainda não iniciado (E18)

Nenhuma fórmula implementada: contexto submodular+MMR (T02), compactação por entropia/JS (T03),
confiança Wilson (T06), anomalia CUSUM/SPRT `✂` (T07), PERT/CPM `✂` (T08), fusão RRF/PPR (T09) e
harness estatístico (T10); o estado com partilha estrutural+Δ (T05) existe como snapshot+tail, não
como estrutura persistente. **F4 (transporte por latência)** está **parcial** via E12-T07 —
TTFT/percentis, cache de prefixo e gate de orçamento feitos; faltam *hedging*, pool/backpressure e
buffer adaptativo (E18-T04).

### 3.4 Dívida técnica concreta

- Emissor TOON concatena `String` (sem `fmt::Write`) — irrelevante sem perfil.
- **Densidade colunar v3 (ADR 0006/0007):** o micro-bench dev-only (`xtask --features tokenizer -- bench-toon`, `cl100k_base`) mede **-18%** vs JSON no corpus alinhado ao registo. A/B das costuras: **digest -36%** (tabela `m` sem `Debug`/id), **catálogo de tools +91 tokens** (clareza/anti-drift — o dono preferiu um prime claro) e **emissor ~1,16–1,21x** (escrita direta, perfil dev). Aliases neutros no corpus sintético (dependem da reutilização por sessão). Detalhe e método em [`docs/toon-melhorias.md`](docs/toon-melhorias.md).
- **Índice de auditoria e retomada (ADR 0008/0009):** A/B dev-only (`xtask bench-audit` / `xtask bench-resume`) — índice binário `KAI1` **-76–79%** vs tabela `t` (100k eventos: 4,85 MB vs 23,9 MB; `decode` ~8× mais rápido que `build`); retomada por snapshot+`offset` **~8–9×** mais rápida que o replay total (20k turnos: 26 ms vs 237 ms).
- Process-group kill / cgroup **deferido** para E17 ([ADR 0004](docs/adr/0004-sem-ffi-kill-grupo-e17.md)).
- **Números publicados:** MVK (`bench/mvk/`) e **latência do provider** (`bench/providers/latency.json`, offline+live) com base/artefacto em `bench/published.toml` (DF5); falta o harness estatístico (`criterion`/`dhat`) de E18-T10.
- **Latência do provider (dev-only):** `xtask provider-smoke`/`bench-provider` medem TTFT/total/usage contra o `llama-server` local (Qwen2.5-Coder-1.5B Q4_K_M, CPU: ~48-55 ms TTFT p50) e o built-in `opencode go` (`longcat-2.5-preview-free`: ~1,5-2,5 s TTFT, `cached=0`; `deepseek-v4.1-flash` com cache de prefixo medido). O gate é **offline e determinístico** (`xtask gate:provider`, ADR 0014); o número live é artefacto, não limite.

### 3.5 Dívida do loop de turnos (ADR 0015)

- **11 de 11 tools executáveis** pelo modelo: a tool `plan` exige `scope_contract.json`/
  `feature_list.json` na raiz (E09-T04); sem artefacto recusa com `Unavailable{scope-contract}`.
- **Argumentos crus do modelo não são logados**: o log guarda o `ToolUse` **resolvido**, não o JSON
  original — replay fiel e auditoria dos argumentos exatos ficam por resolver (dívida E04/§42).
- **`katu run` é um turno único** (sem REPL/TUI multi-turno): o turno abre e fecha na mesma
  invocação.
- **Streaming visível na TUI, não na CLI one-shot**: a TUI (`katu tui`) mostra os deltas do provider
  e as tools em curso ao vivo no painel de atividade (E10-T05 ☑); o `katu run` continua a acumular
  (turno único sem UI).

### 3.6 Baseline E18 (medido)

- **Servidores duradouros** (systemd `--user`): geral `katu-llama.service` (`qwen2.5-coder-1.5b`,
  `127.0.0.1:8080/v1`) e embeddings `knudge-embed.service` (`granite-embedding-97m-r2`, 384d,
  `127.0.0.1:8889/v1`); remoto `opencode-go`/`longcat-2.5-preview-free` por `KATU_OPENCODE_KEY`
  (efémera). Config global do katu ligada a ambos; o *drift* do endpoint do knudge (`:8080` vs
  `:8889`) foi corrigido.
- **Baseline** (`bench/e18/`): turno e2e quente p50 **360 ms** (llama) / **343 ms** (opencode-go);
  **overhead fora do provider 72 ms/turno** — `log.append` 18 ms (5 fsync), `fs.write` 26 ms
  (7 escritas atómicas), `session.open` 8,5 ms; arranque 6,5 ms; embeddings 16 ms; prompt de
  3265 tokens (`cached=3254` a quente; cold start local ~40 s). Números em `bench/published.toml`
  (base `measured`, artefacto `bench/e18/raw.json`). Protocolo/relatório: `bench/e18/`.
- **Pontos cegos de instrumentação** (a fechar antes de otimizar): `KnudgeMemory::open`,
  `skills::discover`/`read_instructions`, `load_rules`, `HttpEmbedder` (2.ª IA), `context.build`
  sem `tokens`, `provider.request` sem `prompt_tokens`; 11 ids do catálogo nunca emitidos
  (`audit.index`, `contain.check/deny`, `context.trim`, `katu.shutdown`, `kernel.stop`,
  `memory.read/compact`, `policy.waiver`, `store.load/save`); `cli.prime`/`memo.drain`/`tui.slash`/
  `mouse.copy` prometidos no `SURFACE_IMPLEMENTATION` §7 e ausentes do catálogo.

---

## 4. Ordem recomendada (próximos passos)

1. **E10 (CLI/TUI)** — a superfície sobre o loop já acionável. **Feito:** loop de eventos,
   keymap puro, render com throttle/teto de trabalho, multi-turno síncrono, streaming visível,
   recusas com regra + evidência e **aprovação por challenge-and-response**
   (T01/T02/T03◐/T04/T05/T06/T07; E07-T05 ☑). **Falta:** zero alocações no render (E18-T10).
2. **E03-T04** — `spawn_blocking` + timeout do adaptador: **resolvido por ADR 0017** (não aplicável
   até existir caminho async; worker bloqueante por desenho). E01-T08/T09 fechados (ADRs 0016/0017).
3. **E15-T01 + E18-T10** — `hyperfine`/`dhat` + gate de regressão no CI (o render e o provider já
   têm gates zero-dep); falta o micro-bench de seleção de regras e os orçamentos de heap/escala (§44).
   **Baseline E18 medido** (§3.6, `bench/e18/`) — fechar primeiro os pontos cegos (§3.6), senão a
   otimização fica às cegas.
4. **E12-T06/T10** — validação ao vivo de `responses`/`messages`/`google` e `dynamic_models`
   (auto-*skip*; E12-T02 ☑); faltam WebSocket/HTTP2; `Control::{SetModel, SetThinking}` no kernel.
5. Fechos core/policy/tools: **E06-T02/T03/T12**, **E07-T02/T03**, **E01-T07**, **E19-T04/T06** — ✅;
   faltam **E06-T07** (rotação gated), a iteração de densidade colunar (§3.4) e a integração de
   `read.diff`/CLI.
6. **E18 (T02/T03/T05/T06/T09)** — frentes from-zero, cada uma com **fórmula + artefacto + teste**
   (plan/19 §0.3); corta-primeiro **T07/T08** `✂`.

## 5. Regras que não se quebram

- Um épico só fecha com `make check` **e** o job `msrv` (Rust 1.97.0) verdes.
- Nenhuma otimização sem artefacto (E18-T10 + `xtask gate:bench`).
- `#![forbid(unsafe_code)]` preservado; FFI só com decisão registada (ADR).
- Firewall LLM-free intacta (`xtask check-layers`).
- Só o dono faz commit.
