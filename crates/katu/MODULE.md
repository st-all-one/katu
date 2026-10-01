# `katu` (binário)

**Épico:** E01/E04 · **Fase:** 0/2.

O **binário**: CLI, composição das portas e adaptador in-process do `knudge`. É aqui que vive
todo o código impuro confinado.

## Responsabilidade

- CLI (`clap`) e wiring das portas (`Clock`, `Rng`, `Fs`, `Env`, `Logger`). A superfície v2 (E20)
  vive em `src/cli.rs` + `src/cli/`: verbos exclusivos `prime`/`upgrade`/`config`/`memo`/`run`/`tui`,
  `prime` estático por grupo (`prime.rs`), `memo` só consulta (`memo.rs`), body/stdin (`input.rs`),
  `--json` por comando e `--log-level` global (default `quiet`); sem subcomando, `katu` abre a TUI.
  A **configuração** vive em `src/config.rs` (conjunto fechado de chaves, paths por SO, merge
  projeto > global) e `cli/config_cmd.rs` (`get/set/unset/list [--global]`, ADR 0020); a entrada
  estruturada `--params` (XOR com flags) e `--batch` JSONL vive em `cli/params.rs` +
  `cli/run_params.rs` (E20-T08). Cada grupo expõe `<grupo> prime` (`memo prime`, `config prime`),
  equivalente a `katu prime --group <g>`. Os **padrões** da config efetiva
  (`provider`/`model`/`base`/`thinking`/`behavior.auto_compact`/`recall.default_limit`) alimentam
  `run`/`tui`/`memo ask` via `src/defaults.rs` (E20-T17); o `thinking` da config entra como
  controlo **logado** no arranque da TUI. A **segunda IA** (embeddings) projeta-se em runtime na
  config do knudge (`src/memory/drain.rs`): `embeddings.url` ausente → `off` (nunca inventado).
  O **bootstrap** do `.katu/` vive em `src/bootstrap.rs` (layout
  idempotente, snapshot 1:1 da config, guardrails e blocos geridos em `.gitignore`/`.gitattributes`/
  `.git/info/exclude`, ADR 0021); `--init` e o arranque de sessão (`run`/`tui`/default TUI) correm-no
  antes do primeiro turno, e `memo doctor --fix` é o mesmo caminho.
- Adaptadores das portas em `src/ports/` (`mod.rs` = relógio/RNG/env; `process.rs` = `StdProcess`
  com timeout, `process_group` e leitura limitada — E07-T04; `fs/` = `StdFs` com escrita atómica
  **endurecida**: temporário exclusivo `O_EXCL`/`0600` e nome imprevisível, para não seguir um
  symlink plantado — E07-T04). O `process.rs` mata o **grupo** de processos no timeout
  (`kill(-pgid, SIGKILL)`, `kill_group`): é o **único ponto `unsafe`** do projeto (ADR 0016;
  exceção registada em `xtask check-unsafe`), isolado com a fronteira de segurança documentada.
- Adaptador in-process da porta `Memory` sobre o `knudge-core` (`src/memory/`, feature
  `memory-in-process` **default**) — o único sítio com dependência do knudge. A fachada `Knudge`
  (`!Sync`) é protegida por `Mutex` com cache de índice/grafo; `pre_edit` decide *supersede* em
  *dry-run* fiel ao `update`; `memo doctor` expõe `memory.status()` e a suíte de
  conformidade corre contra o adaptador (E03-T02/T05/T06/T07). A **paridade `memo`/`kd`** (E20-T06)
  vive em `src/memory/query.rs` (+ `query/{convert,map,suggest}.rs`): filtros, modos
  `--rank`/`--tags`/`--suggest`, `--id`/`--around` e o mapa estrutural (`memo knowledge`). O
  conhecimento vive em `.katu/knowledge` (E20-T19) e `memo drain --digest [--force]` drena a fila de
  embeddings pelo pipeline do `knudge-core` (`src/memory/drain.rs`, E20-T20), fail-closed sem
  provedor; o worker de auto-drain (`--watch-service`) vive em `src/watch_service.rs` (+ script
  `scripts/katu-idle.sh`). Sem o adaptador, `katu memory` **falha fechado** (`unavailable`, exit 10)
  — a memória é invariante de produção (G4).
- **Memória** (`src/memory/`, E03-T02, P-03): adaptador in-process da porta `Memory` sobre o
  `knudge-core` — o **único** sítio do binário com o vocabulário do knudge (firewall `check-layers`).
  O índice/grafo ficam em cache e são **invalidados** por cada escrita; o commit **reusa** o índice
  que o `pre_write` do mesmo gate construiu (P-03: gate com 1 000 notas −45,1 %, o custo era
  reconstruir `Index::from_store` duas vezes; [`bench/e18/memory`](../../bench/e18/memory/PROTOCOL.md)).
- **Runtime** (`src/runtime.rs`, feature `memory-in-process`): ponto de composição do loop
  (E03-T03/T07) — descobre a raiz, abre o adaptador, **recusa arrancar** sem memória saudável
  (fail-closed) e expõe `recall`/`remember` pelo caminho §42 (`src/runtime/memory.rs`). O comando
  `memo ask` exercita o recall; a escrita (`remember`) é do agente/`kd` (a superfície `memo` só
  consulta). `Runtime::open` cria sessão; `Runtime::resume`
  (por id ou a mais recente) retoma o log durável e **fecha** um turno aberto antes do seguinte.
  Os submodules `src/runtime/context.rs` (contexto efetivo + compactação, E09-T01/T07),
  `src/runtime/verify.rs` (gate de verificação sobre o log, E09-T03) e `src/runtime/memory.rs`
  estendem o runtime; o teto de contexto tem **um único dono** (`DEFAULT_CONTEXT_BUDGET`).
- **Contrato de escopo** (`src/scope.rs`, E09-T04): carrega `scope_contract.json` +
  `feature_list.json` da raiz no arranque e valida o `Plan` (schema + "≤ 1 `in_progress`"),
  **antes de qualquer turno** (fail-closed: artefacto pela metade ou inválido recusa o arranque).
  Com o artefacto, a tool `plan` deixa de ser `Unavailable` e o plano é registado
  (`PlanRecorded`) pela ordem §42 em `src/agent/plan.rs`, tornado a fase `Planned` alcançável;
  sem ele, `plan` mantém `Unavailable{scope-contract}`.
- **Loop de turnos** (`src/agent/`, feature `memory-in-process`, E12-T05/E10): liga o provider ao
  kernel — monta `ProviderRequest` via `Session::context` (`assemble`/`compact`: prime determinístico
  + digest, E09-T01/T07) e o catálogo de tools, e executa cada tool call pela ordem §42 (logar → política → efeito). O `router` mapeia os
  argumentos JSON do modelo num `ToolUse` resolvido (caminhos canonicalizados antes do veredicto,
  E07-T02) e no executor; a tool `memory` passa pelos caminhos de recall/escrita do gate de E05.
  O comando `katu run` exercita-o. Envelopes de `Dispatch`/memória vivem em `src/memory/commands.rs`.
  **Guard de loop** (Q-12/F7): cada passo é observado **antes** de executar
  (`kernel::guard`, CUSUM + **e-value** *anytime-valid* sobre a assinatura das chamadas); um ciclo de
  leitura sem progresso corta o turno no 5.º passo com `AgentError::LoopDetected` (categoria
  `conflict`, exit 5), emite `agent.loop` e **fecha** o turno. O turno fecha também nos outros
  erros (`TooManySteps`): um `TurnStart` sem `TurnEnd` deixaria a retomada inconsistente.
- **Gate de VOI** (A3/W8-4, `src/agent/turn/voi.rs`): não repete uma só-leitura já satisfeita no
  turno (`VOI = 0 < custo`) e **nunca** salta o irreconstruível; a mutação invalida a informação
  cacheada. **Opt-in** (`behavior.tool_voi`, default **off** até A/B com o modelo — precedente
  Q-02b/Q-03). Medido em [`bench/e18/voi`](../../bench/e18/voi/PROTOCOL.md): 2 de 11 chamadas
  evitadas em cenários canónicos, 0 irreconstruíveis saltados.
- **UI de terminal** (`src/tui.rs`, feature `memory-in-process`, E10-T01/T02/T05): comando
  `katu tui`. A UI (`katu-tui`) é pura (estado central + keymap + render) e a borda implementa o
  `Handler` que corre o turno e injeta `Update`s; `Runtime::begin_turn` abre o próximo turno
  (multi-turno) e `Runtime::phase` alimenta o indicador de fase (E10-T06). O streaming do modelo e
  as tools em curso vão **ao vivo** para o painel de atividade via `run_turn_with` + `ActivitySink`
  (`LivePainter`/`Painter`), sem entrarem no log. O **steering** (E20-T16) é consultado **entre
  passos** (`ActivitySink::steer`) e injetado como mensagem de utilizador no passo seguinte. O
  **modo de planeamento** (`/plan`, E20-T11) vive em `src/runtime/plan_mode.rs` (regras
  `plan-write-only-katu`/`plan-no-shell` + artefacto `.katu/plan/<UTC>.md`), e `!<cmd>` (E20-T12)
  corre `sh -c` pelo pipeline em `src/agent/shell.rs`. O **contexto do projeto** (E20-T13) é lido
  no arranque (`src/runtime/skills.rs`): o `AGENTS.md` da raiz vai para o **topo** do prompt de
  sistema (fonte de verdade máxima) e as skills `.agents/skill{,s}/*/SKILL.md` entram como
  catálogo (nome/descrição/caminho); `/skill:<nome>` força o carregamento. A **transcrição durável** é projetada do log
  (`Runtime::transcript`, `src/runtime/transcript.rs`) e escrita atomicamente em
  `<root>/.katu/transcript.md` após cada turno (`src/tui/transcript.rs`); a TUI serve-a numa vista
  read-only (`T`, E10-T05). As **recusas de política**
  (`Denied`/`Unavailable`) chegam ao painel/transcript com regra + evidência, e uma
  `RequireApproval` abre um **challenge-and-response** na TUI: o humano assina
  (`reason`+`granted_by`), o kernel regista `ApprovalGranted` e concede a capacidade mínima
  (`katu-policy::capability_for`), re-executando a chamada (E10-T04/E07-T05, §33). O runtime
  carrega as regras de **memória + contenção** e define o **workspace** no arranque (`Runtime::open`),
  pelo que a contenção é aplicada no loop. A TUI liga/desliga a compactação (`c`), lista/restaura a
  lixeira (`l`/`r`) e **esvazia-a** (`x`, com challenge; `trash::empty` + `Fs::remove`, E10-T07), e
  corre o **gate de verificação** (`v`; se bloquear, pede override humano por challenge, E09-T03).
  O **modelo/pensamento** (`m`/`t`) passa por `Runtime::set_control` (validado contra o catálogo com
  erro que ensina, E12-T10) e a lista de modelos vem de `Provider::dynamic_models()` com queda em
  `Provider::models()` (E12-T02); sem controlo explícito, o turno usa o **tier** da fase
  (`src/tier.rs`: `policy/tiers.toml` → `TierPolicy`, resolvido por `Provider::model_for_tier`,
  E12-T03). O turno
  seguinte usa o estado de controlo (o agente nunca se auto-escala). O **checkpoint** de fase é
  escrito no fim de cada turno (`Runtime::write_checkpoint`, próxima ação de `next_phase`) e lido no
  arranque (`Runtime::checkpoint`); o cabeçalho mostra a próxima ação (E10-T06). O handler vive em
  `src/tui/handler.rs` (a borda ficou sob o teto de linhas). O utilizador pode **cancelar** o turno
  (Esc/Ctrl-C durante o stream → `ActivitySink::cancelled` → `Flow::Break`, diag `tui.cancel`); o
  **uso/custo** do turno aparece no cabeçalho (`usage_line`, `src/pricing.rs` + `policy/prices.toml`)
  e os **argumentos crus** do modelo no painel (`Activity::Tool { args }`). `--resume [last|id]`
  retoma uma sessão e `memo sessions` lista-as. O turno é **síncrono** nesta fatia (executor em
  background é trabalho futuro).
- Exit codes na borda (a lógica propaga `Result`).
- Harness de medição do MVK (`examples/measure_mvk.rs`, feature `profile`, E05-T06): corre o
  caminho real e grava `bench/mvk/raw.json` (evidência tipada, DF5).

## Fronteira

- Pode depender de todos os crates, mas mantém o **firewall** a montante: os crates puros não
  dependem dele.
