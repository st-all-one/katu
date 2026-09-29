# STAGING — estado e pendências do katu

> Documento **operacional e volátil**: retrato do que está feito, do que falta e do que bloqueia,
> frente ao [`plan/`](plan/) e ao [`IMPLEMENTATION_PLAN.md`](IMPLEMENTATION_PLAN.md). Não substitui
> os épicos: cada item aponta para a tarefa. Atualizar quando um épico fecha (não é um contrato).

## 0. Snapshot

- **6 crates + `xtask`**: `katu-policy`, `katu-core`, `katu-tools`, `katu` (bin), `katu-providers`*, `katu-tui`*.
- **339 testes** · catálogo de instrumentação **62 ids** · **11 tools** · **8 regras** (5 memória + 3 contenção) · **4 ADRs**.
- `make check` verde (fmt + clippy `-D warnings` + testes + `check-layers` + `check-diag` + `check-docs` + `policy:audit` + `gate:bench` + file-length ≤300) · `make instrument` verde.
- **O MVK passou** ([ADR 0001](docs/adr/0001-mvk-gate-aprovado.md)); o kernel (E04) e a política (E02) estão completos.

\* `katu-providers` e `katu-tui` são **stubs vazios**.

## 1. O que já funciona

- Kernel event-sourced: `step` puro, log append-only, `derive_messages`/`state_of`/`snapshot`, `Session::verify()` (`Model-visible ⟺ logged`).
- Política pura (`evaluate`), vocabulário fechado v2, ledger de cobertura, auditoria.
- Gate de memória pelo caminho real (recall → write → close) e cost governor ligado ao `Session`.
- Tools: read (6 views), write, edit (CAS + dry-run + `Stale`), move, trash, bash (scrub + timeout), grep/find/ls, plan, memory (record + recall).
- Contexto: `assemble` + prime (+`--long`), compactação determinística opt-in, gate de verificação, scope contracts.
- Contenção soft: sensíveis negados, fora do workspace → aprovação, busca como leitura, `Capability::Net`.
- Instrumentação transversal zero-custo (DF9).

---

## 2. Gaps por épico

| Tarefa | Estado | Pendência |
|---|---|---|
| **E01-T07** | ◐ | Rotação/observabilidade de logs. |
| **E01-T08** | ☐ | Política de memória e `unsafe` (documento/decisão). |
| **E01-T09** | ☐ | Política de recursos e runtime mínimo. |
| **E01-T10** | ◐ | `xtask` e CI em camadas (fecho). |
| **E03-T02** | ☐ | **Adaptador in-process do `knudge-core`** (primário, no binário). |
| **E03-T03** | ☐ | Construtor à moda `build_with_transport()`. |
| **E03-T04** | ☐ | `spawn_blocking` + timeout no caminho async. |
| **E03-T05** | ◐ | Suíte de conformidade existe; falta correr contra o adaptador in-process. |
| **E03-T06** | ☐ | Gate de substituibilidade (`check-memory-swap`). |
| **E03-T07** | ☐ | Memória como invariante (produção sempre com memória; fail-closed no arranque). |
| **E06-T02** | ☐ | Tool-schema linter (reusa `validate::Issue`). |
| **E06-T03** | ◐ | Matriz multibyte (§45.22); `read.diff` sem `base` ligado; `edit.hunks` fixo. |
| **E06-T07** | ◐ | Rotação do registo de comando (→ E01-T07). |
| **E07-T02** | ◐ | Resolução de symlinks. |
| **E07-T03** | ◐ | Gate do épico: autorização de workspace. |
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
| **E19-T04** | ☐ | Consistência (fingerprint determinístico). |
| **E19-T05** | ◐ | Gate no CI (fecho). |
| **E19-T06** | ☐ | Filtro de nível por subsistema (OA18). |
| **E08 · E11 · E17** | ⏸️ | Futuro (MCP, plugins WASM, jail de SO). |

---

## 3. Gaps por natureza

### 3.1 Bloqueadores (impedem o agente de correr)

1. **Sem loop acionável (E10).** O binário só faz `version`/`doctor`. Não há driver do kernel nem TUI.
2. **Sem provider (E12).** `katu-providers` é stub; não há LLM.
3. **Sem memória real (E03-T02).** Só `FakeMemory`; o adaptador in-process do `knudge-core` não está ligado — **bloqueado** por dep (path `knudge/crates/knudge-core`, fora do registry).

### 3.2 Integração CLI/TUI (lógica já feita no core)

- Autorização interativa de E07-T05 / override de E09-T03.
- Comando de compactação de E09-T07.
- Carregamento de `scope_contract`/`feature_list` de E09-T04.
- `read view=diff` precisa que o chamador forneça o `base` (checkpoint/leitura anterior).

### 3.3 From-zero ainda não iniciado (E18)

Nenhuma fórmula implementada: contexto submodular+MMR (T02), compactação por entropia/JS (T03), transporte por latência (T04), estado com partilha estrutural+Δ (T05), confiança Wilson (T06), anomalia CUSUM/SPRT `✂` (T07), PERT/CPM `✂` (T08), fusão RRF/PPR (T09), harness estatístico (T10).

### 3.4 Dívida técnica concreta

- `edit.patch` devolve `hunks: 1` **fixo**, sem o delta real.
- Campo `cost` do `ToolReport` **nunca é preenchido**.
- `content_id` = 8 hex (32 bits) — colisões a escala.
- Emissor TOON concatena `String` (sem `fmt::Write`) — irrelevante sem perfil.
- Process-group kill / cgroup **deferido** para E17 ([ADR 0004](docs/adr/0004-sem-ffi-kill-grupo-e17.md)).
- **Nenhum número de tokens/latência publicado** (regra: nada sem artefacto — E15/E18).

---

## 4. Ordem recomendada (próximos passos)

1. **E10 (CLI/TUI)** — transforma o kernel+toolset num agente executável e destranca as autorizações de E07/E09.
2. **E12-T01/T05** — port `Provider` + provider fake (desbloqueia testes de loop reais).
3. **E03-T02** — adaptador in-process do knudge (memória real; decisão de dep pendente).
4. **E15-T01 + E18-T10** — harness de medição antes de qualquer otimização.
5. Fechos: **E06-T02/T03**, **E07-T02/T03**, **E01-T07**.

## 5. Regras que não se quebram

- Um épico só fecha com `make check` **e** o job `msrv` (Rust 1.97.0) verdes.
- Nenhuma otimização sem artefacto (E18-T10 + `xtask gate:bench`).
- `#![forbid(unsafe_code)]` preservado; FFI só com decisão registada (ADR).
- Firewall LLM-free intacta (`xtask check-layers`).
- Só o dono faz commit.
