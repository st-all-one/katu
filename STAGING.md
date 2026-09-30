# STAGING — estado e pendências do katu

> Documento **operacional e volátil**: retrato do que está feito, do que falta e do que bloqueia,
> frente ao [`plan/`](plan/) e ao [`IMPLEMENTATION_PLAN.md`](IMPLEMENTATION_PLAN.md). Não substitui
> os épicos: cada item aponta para a tarefa. Atualizar quando um épico fecha (não é um contrato).

## 0. Snapshot

- **6 crates + `xtask`**: `katu-policy`, `katu-core`, `katu-tools`, `katu` (bin), `katu-providers`*, `katu-tui`*.
- **411 testes** · catálogo de instrumentação **73 ids** · **11 tools** · **8 regras** (5 memória + 3 contenção) · **10 ADRs**.
- `make check` verde (fmt + clippy `-D warnings` + testes + `check-layers` + `check-diag` + `check-schemas` + `check-docs` + `check-memory-swap` + `policy:audit` + `gate:bench` + file-length ≤300) · `make instrument` verde.
- **O MVK passou** ([ADR 0001](docs/adr/0001-mvk-gate-aprovado.md)); o kernel (E04) e a política (E02) estão completos.

\* `katu-providers` e `katu-tui` são **stubs vazios**.

## 1. O que já funciona

- Kernel event-sourced: `step` puro, log append-only, `derive_messages`/`state_of`/`snapshot`, `Session::verify()` (`Model-visible ⟺ logged`).
- Política pura (`evaluate`), vocabulário fechado v2, ledger de cobertura, auditoria.
- Gate de memória pelo caminho real (recall → write → close) e cost governor ligado ao `Session`.
- Tools: read (6 views), write, edit (CAS + dry-run + `Stale`), move, trash, bash (scrub + timeout), grep/find/ls, plan, memory (record + recall). **Linter de schema** (`katu-tools::schema`) imposto por `xtask check-schemas`.
- Contexto: `assemble` + prime (+`--long`), compactação determinística opt-in, gate de verificação, scope contracts.
- Contenção soft: sensíveis negados, fora do workspace → aprovação, busca como leitura, `Capability::Net`; **symlink resolvido via porta `Fs` antes do veredicto** (`katu-tools::resolve`).
- Instrumentação transversal zero-custo (DF9): **redação por allowlist** no sink (E01-T07), **fingerprint determinístico** (`fingerprint!`, E19-T04) e **filtro por subsistema** (E19-T06).
- **Saída ao modelo colunar v3** (ADR 0006/0007): **sem headers** no *stream* (o prime v3 é o registo de esquema, re-emitido no início e após compactação), blocos literais (`\x1d`) para código, escalares explícitos em `k`, domínios no registo e **aliases de sessão** `#N`/`@N` (lazy, limiar 3). Projeções model-facing canónicas de `ToolOutcome`/`Error`/`VerificationReport`; digest de compactação em tabela `m` (sem `Debug`); catálogo de tools no prime (`prime_with_catalog`, anti-drift). JSON de máquina inalterado.
- **Sessões e auditoria** (ADR 0008/0009): sessões vinculadas ao projeto em `.katu/sessions/<id>` (id `s_<16hex>` determinístico, índice temporal `(created_ms, id)`, **snapshot de estado com offset** por fase — inclui o histórico temporal `rolling`/`velocity` que o log não reproduz —, retomada que lê só a cauda, `Session::create`/`resume`/`list`); auditoria permanente e pesquisável em `.katu/audit` (**segmentos colunares imutáveis** + **índice invertido binário `KAI1`** com posições e **Bloom** por segmento — termos, frases, filtros), local e em `.git/info/exclude`.
- **Memória de primeira classe** (ADR 0010): porta `Memory` com tipos do katu + adaptador in-process sobre `knudge-core` isolado no binário (`src/memory/`); `Runtime::open` (`src/runtime.rs`) monta a sessão com o adaptador e **recusa arrancar** sem memória saudável (fail-closed, E03-T03/T07); `katu remember`/`katu recall` exercitam o caminho §42; gate `check-memory-swap` (E03-T06).

---

## 2. Gaps por épico

| Tarefa | Estado | Pendência |
|---|---|---|
| **E01-T07** | ☑ | Redação por allowlist no sink (E01-T07); rotação de ficheiro fica deliberadamente gated (nunca apaga automaticamente). |
| **E01-T08** | ☐ | Política de memória e `unsafe` (documento/decisão). |
| **E01-T09** | ☐ | Política de recursos e runtime mínimo. |
| **E01-T10** | ◐ | `xtask` e CI em camadas (fecho). |
| **E03-T02** | ☑ | **Adaptador in-process do `knudge-core`** (primário, no binário; feature default). |
| **E03-T03** | ☑ | `Runtime::open` monta sessão + adaptador + regras; `recall`/`remember` pelo §42 (E10/E12). |
| **E03-T04** | ☐ | `spawn_blocking` + timeout; **gated** em E12/E01-T09 (worker bloqueante por desenho). |
| **E03-T05** | ☑ | Suíte de conformidade corre contra o fake **e** o adaptador in-process. |
| **E03-T06** | ☑ | Gate de substituibilidade (`xtask check-memory-swap` + `make memory-swap`). |
| **E03-T07** | ☑ | `Runtime::open` recusa sem memória saudável (fail-closed); comandos falham com exit 10 sem adaptador. |
| **E06-T02** | ☑ | Linter de schema (`katu-tools::schema`) + `xtask check-schemas` em `make check`. |
| **E06-T03** | ☑ | Matriz multibyte (§45.22); `edit.hunks`/`added`/`removed` reais; chunk único truncado em limite UTF-8. `read.diff` sem `base` é integração CLI (§3.2). |
| **E06-T12** | ☑ | Formato ao modelo **colunar v3** (ADR 0006): sem headers (registo no prime), blocos literais, `k` explícito, aliases de sessão; qualidade (`rank`/`basis`/`ev`/`sym`). A/B: **-21%** vs JSON; aliases neutros no corpus sintético (§3.4). |
| **E06-T07** | ◐ | Rotação do registo de comando (→ E01-T07). |
| **E07-T02** | ☑ | Symlink resolvido via porta `Fs` (`Fs::canonicalize` + `katu-tools::resolve`) antes do veredicto; `StdFs`/`MemFs` com teste de escape. |
| **E07-T03** | ☑ | Gate do épico: autorização ausente fora do workspace recusada; symlink para fora negado na política. |
| **E07-T05** | ◐ | **Autorização interativa** (`override_reason`+`granted_by`) — CLI/TUI (E10). |
| **E09-T03** | ◐ | Override interativo (CLI/TUI); kernel `→ Verified` feito. |
| **E09-T04** | ◐ | Carregar `scope_contract.json`/`feature_list.json` no arranque (CLI). |
| **E09-T05** | ◐ | `Metric`/portão de publicação — fecho. |
| **E09-T07** | ◐ | Comando do utilizador para compactar (CLI/TUI); gatilho do kernel feito. |
| **E10-T01…T07** | ☐ | **CLI/TUI inteira** (ratatui/crossterm, keymap, render, superfície de política, checkpoint). |
| **E12-T01…T10** | ☐ | **Camada de providers inteira** (built-in `opencode go/zen`, GDK, custo, timeout, fake, llama.cpp). |
| **E13-T01…T07** | ☐ | Camadas de teste, teste real por regra, regressão invertida, Miri/geiger/machete, goldens, matriz de aceitação. |
| **E14-T02…T07** | ☐ | Postmortems, "um facto um lar", `policy/` versionado, teto de superfície, catálogos gerados, slices. |
| **E15-T01** | ◐ | `criterion` + gate de performance no CI. |
| **E15-T03…T06** | ☐ | Tabela de recuo, determinismo de prefixo, instrumentação do prefixo (`✂`), negativos. |
| **E18-T01…T10** | ☐ | **Otimização profunda (matemática/info/estatística): 0%.** |
| **E19-T03** | ◐ | Instrumentar o caminho crítico por épico (cresce com o código). |
| **E19-T04** | ☑ | Fingerprint determinístico (`diag::fingerprint!`) com golden. |
| **E19-T06** | ☑ | Filtro de nível por subsistema (`KATU_INSTRUMENT_FILTER`/`set_filter`). |
| **E19-T05** | ◐ | Gate no CI (fecho). |
| **E08 · E11 · E17** | ⏸️ | Futuro (MCP, plugins WASM, jail de SO). |

---

## 3. Gaps por natureza

### 3.1 Bloqueadores (impedem o agente de correr)

1. **Sem loop acionável (E10).** O binário só faz `version`/`doctor`. Não há driver do kernel nem TUI.
2. **Sem provider (E12).** `katu-providers` é stub; não há LLM.
3. **Memória real ligada, kernel ainda não.** O adaptador in-process está no binário
   (`katu/src/memory/`, feature `memory-in-process` **default**) e passa a conformidade; falta o
   kernel consumir a porta no loop real (E10) — hoje só o `doctor` a expõe.

### 3.2 Integração CLI/TUI (lógica já feita no core)

- Autorização interativa de E07-T05 / override de E09-T03.
- Comando de compactação de E09-T07.
- Carregamento de `scope_contract`/`feature_list` de E09-T04.
- `read view=diff` precisa que o chamador forneça o `base` (checkpoint/leitura anterior).

### 3.3 From-zero ainda não iniciado (E18)

Nenhuma fórmula implementada: contexto submodular+MMR (T02), compactação por entropia/JS (T03), transporte por latência (T04), estado com partilha estrutural+Δ (T05), confiança Wilson (T06), anomalia CUSUM/SPRT `✂` (T07), PERT/CPM `✂` (T08), fusão RRF/PPR (T09), harness estatístico (T10).

### 3.4 Dívida técnica concreta

- Emissor TOON concatena `String` (sem `fmt::Write`) — irrelevante sem perfil.
- **Densidade colunar v3 (ADR 0006/0007):** o micro-bench dev-only (`xtask --features tokenizer -- bench-toon`, `cl100k_base`) mede **-18%** vs JSON no corpus alinhado ao registo. A/B das costuras: **digest -36%** (tabela `m` sem `Debug`/id), **catálogo de tools +91 tokens** (clareza/anti-drift — o dono preferiu um prime claro) e **emissor ~1,16–1,21x** (escrita direta, perfil dev). Aliases neutros no corpus sintético (dependem da reutilização por sessão). Detalhe e método em [`docs/toon-melhorias.md`](docs/toon-melhorias.md).
- **Índice de auditoria e retomada (ADR 0008/0009):** A/B dev-only (`xtask bench-audit` / `xtask bench-resume`) — índice binário `KAI1` **-76–79%** vs tabela `t` (100k eventos: 4,85 MB vs 23,9 MB; `decode` ~8× mais rápido que `build`); retomada por snapshot+`offset` **~8–9×** mais rápida que o replay total (20k turnos: 26 ms vs 237 ms).
- Process-group kill / cgroup **deferido** para E17 ([ADR 0004](docs/adr/0004-sem-ffi-kill-grupo-e17.md)).
- **Nenhum número de tokens/latência publicado** (regra: nada sem artefacto — E15/E18).

---

## 4. Ordem recomendada (próximos passos)

1. **E10 (CLI/TUI)** — transforma o kernel+toolset num agente executável e destranca as autorizações de E07/E09.
2. **E12-T01/T05** — port `Provider` + provider fake (desbloqueia testes de loop reais).
3. **E03-T02/T03/T06/T07 feitos** — adaptador in-process, runtime que monta a sessão com o adaptador (fail-closed), gate de substituibilidade e comandos `remember`/`recall`; falta o loop de turnos (provider, E12) e o `spawn_blocking`+timeout (E03-T04).
4. **E15-T01 + E18-T10** — harness de medição antes de qualquer otimização.
5. Fechos core/policy/tools: **E06-T02/T03/T12**, **E07-T02/T03**, **E01-T07**, **E19-T04/T06** — ✅; faltam **E06-T07** (rotação gated), **E03-T04** (timeout async), a iteração de densidade colunar (§3.4) e a integração de `read.diff`/CLI.

## 5. Regras que não se quebram

- Um épico só fecha com `make check` **e** o job `msrv` (Rust 1.97.0) verdes.
- Nenhuma otimização sem artefacto (E18-T10 + `xtask gate:bench`).
- `#![forbid(unsafe_code)]` preservado; FFI só com decisão registada (ADR).
- Firewall LLM-free intacta (`xtask check-layers`).
- Só o dono faz commit.
