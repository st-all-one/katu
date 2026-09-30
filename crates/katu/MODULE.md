# `katu` (binário)

**Épico:** E01/E04 · **Fase:** 0/2.

O **binário**: CLI, composição das portas e adaptador in-process do `knudge`. É aqui que vive
todo o código impuro confinado.

## Responsabilidade

- CLI (`clap`) e wiring das portas (`Clock`, `Rng`, `Fs`, `Env`, `Logger`).
- Adaptadores das portas em `src/ports/` (`mod.rs` = relógio/RNG/env; `process.rs` = `StdProcess`
  com timeout, `process_group` e leitura limitada — E07-T04; `fs/` = `StdFs` com escrita atómica
  **endurecida**: temporário exclusivo `O_EXCL`/`0600` e nome imprevisível, para não seguir um
  symlink plantado — E07-T04). Sem FFI no MVP (ADR 0004).
- Adaptador in-process da porta `Memory` sobre o `knudge-core` (`src/memory/`, feature
  `memory-in-process` **default**) — o único sítio com dependência do knudge. A fachada `Knudge`
  (`!Sync`) é protegida por `Mutex` com cache de índice/grafo; `pre_edit` decide *supersede* em
  *dry-run* fiel ao `update`; `doctor --json` e `katu memory` expõem `memory.status()` e a suíte de
  conformidade corre contra o adaptador (E03-T02/T05/T06/T07). Sem o adaptador, `katu memory`
  **falha fechado** (`unavailable`, exit 10) — a memória é invariante de produção (G4).
- **Runtime** (`src/runtime.rs`, feature `memory-in-process`): ponto de composição do loop
  (E03-T03/T07) — descobre a raiz, abre o adaptador, **recusa arrancar** sem memória saudável
  (fail-closed) e expõe `recall`/`remember` pelo caminho §42. Os comandos `katu remember` e
  `katu recall` exercitam-no.
- **Contrato de escopo** (`src/scope.rs`, E09-T04): carrega `scope_contract.json` +
  `feature_list.json` da raiz no arranque e valida o `Plan` (schema + "≤ 1 `in_progress`"),
  **antes de qualquer turno** (fail-closed: artefacto pela metade ou inválido recusa o arranque).
  Com o artefacto, a tool `plan` deixa de ser `Unavailable` e o plano é registado
  (`PlanRecorded`) pela ordem §42 em `src/agent/plan.rs`, tornado a fase `Planned` alcançável;
  sem ele, `plan` mantém `Unavailable{scope-contract}`.
- **Loop de turnos** (`src/agent/`, feature `memory-in-process`, E12-T05/E10): liga o provider ao
  kernel — monta `ProviderRequest` a partir da projeção do log (`derive_messages`) + catálogo de
  tools, e executa cada tool call pela ordem §42 (logar → política → efeito). O `router` mapeia os
  argumentos JSON do modelo num `ToolUse` resolvido (caminhos canonicalizados antes do veredicto,
  E07-T02) e no executor; a tool `memory` passa pelos caminhos de recall/escrita do gate de E05.
  O comando `katu run` exercita-o. Envelopes de `Dispatch`/memória vivem em `src/memory/commands.rs`.
- **UI de terminal** (`src/tui.rs`, feature `memory-in-process`, E10-T01/T02/T05): comando
  `katu tui`. A UI (`katu-tui`) é pura (estado central + keymap + render) e a borda implementa o
  `Handler` que corre o turno e injeta `Update`s; `Runtime::begin_turn` abre o próximo turno
  (multi-turno) e `Runtime::phase` alimenta o indicador de fase (E10-T06). O streaming do modelo e
  as tools em curso vão **ao vivo** para o painel de atividade via `run_turn_with` + `ActivitySink`
  (`LivePainter`/`Painter`), sem entrarem no log (E10-T05). O turno é **síncrono** nesta fatia
  (executor em background é trabalho futuro).
- Exit codes na borda (a lógica propaga `Result`).
- Harness de medição do MVK (`examples/measure_mvk.rs`, feature `profile`, E05-T06): corre o
  caminho real e grava `bench/mvk/raw.json` (evidência tipada, DF5).

## Fronteira

- Pode depender de todos os crates, mas mantém o **firewall** a montante: os crates puros não
  dependem dele.
