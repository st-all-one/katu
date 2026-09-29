# 13 — Arquitetura, determinismo e qualidade

> Guia para quem trabalha **no código** do knudge (ou quer entendê-lo a fundo).
> Para uso, basta `02`–`12`.

## 1. Princípio: núcleo puro + portas + adaptadores (D65)

O domínio **não conhece** terminal, `argv`, relógio global, RNG global nem FS.
Todo acesso ao mundo externo atravessa uma **porta** (`knudge_core::ports`):

- testar o domínio só com **fakes** (`ports::fakes`), reprodutível byte a byte;
- trocar implementação (real ↔ fake ↔ remota) sem tocar no domínio;
- confinar `unsafe`/I/O impuro a poucos módulos.

```
knudge-mcp ──┐
             ├── knudge-core (núcleo puro + portas + adaptadores std)
knudge-cli ──┘
```

| Crate | Papel |
|---|---|
| `knudge-core` | modelo, schema, TOON, retrieval, grafo, tarefas, ciclo de vida + **portas**; `adapters` (std) isolado |
| `knudge-cli` | binário `kd`; `clap`, envelope `--json`, logging, saída |
| `knudge-mcp` | servidor MCP: motor de gatilhos + JSON-RPC stdio |

`knudge-core` é a única biblioteca; `cli`/`mcp` são pacotes de binário.

## 2. Portas e fakes (D65)

| Porta | Fornece | Fake |
|---|---|---|
| `Clock` | instante UTC (`Timestamp`) | `FixedClock` |
| `Rng` | aleatoriedade (só jitter) | `SeqRng` |
| `Env` | variáveis e argumentos | `FakeEnv` |
| `Fs` | I/O atômico, append, create-exclusive, rename, `fsync`, mtime | `MemFs`, `FaultyFs` |
| `Git` | worktree, status, `git` sem shell | `FakeGit` |
| `HookRunner` | hooks externos (sem shell, timeout + kill de grupo) | `NoopHookRunner` |
| `Logger` | log estruturado (stderr) | `RecordingLogger` |
| `Embedder` | provedor de embeddings | `FakeEmbedder` |

## 3. Mapa de módulos (`knudge-core`)

| Módulo | Responsabilidade |
|---|---|
| `error` | `Error`/`ErrorKind`, mapa código→exit, poison |
| `time` | `Timestamp` UTC em ms |
| `logging` | redação de segredos |
| `ports` | traits + fakes |
| `adapters` | implementações `std` |
| `config` | config em dois níveis, schema, codec TOML |
| `schema` | schema canônico, tipos, IDs, claims, proveniência, slots |
| `toon` | parser/emissor TOON |
| `jsonl` | leitura/escrita JSONL + JSON canônico |
| `store` | notas, eventos, lock, rebuild, purge, sweep |
| `git` | worktree, `info/exclude`, `AGENTS.md`, `sync` |
| `graph` | arestas, integridade, ciclos, rank (PageRank/PPR), comunidades, ontologia, TMS |
| `retrieval` | BM25, âncoras, filtros, views, RRF, fold, stemming, PPR |
| `write` | escrita, dedup (sieve + MinHash/LSH), update/supersede, lifecycle |
| `handoff` | rewind, manifest, orçamento, `context_id` |
| `maintenance` | diff, learn, compact |
| `task` | hierarquia, progresso/impacto/papel, flow |
| `health` | validators, evidência, audit, doctor, leitura tolerante, âncoras |
| `lifecycle` | shelf-life, retention, usage, confidence/beta, decay/drift, term-drift, clusters, demolição |
| `embeddings` | provedor HTTP, cache, fila, `rank_query`, `suggest` |

Cada crate tem um `MODULE.md` — mantenha-o em sincronia.

## 4. Persistência e concorrência (D20–D28)

| Caminho | Papel | Recuperável? |
|---|---|---|
| `notas/<tipo>/<id>.md` | **verdade** | versionado |
| `eventos/events*.jsonl` | auditoria append-only | `merge=union` |
| `.idx/` | **derivado** | descartável |
| `.knudge/emb_cache.jsonl` | cache vetorial (opt-in) | reconstruível |
| `.locks/` / `*.lock` | lock advisory | efêmero |

- **Escrita atômica** (D20): tmp + rename no mesmo dir; `fsync` em batch (D22).
- **Ordem de commit:** **nota antes do evento** (D21).
- **Lock advisory:** `O_CREAT|O_EXCL`, stale 30 s, reclaim por rename, ordem
  externo=container/interno=nota (evita deadlock ABBA), liberação RAII.
- **Rebuild double-buffer** (D27): `.idx.new/` + rename atômico.
- **Purga/detach/sweep** (D84/D160): remoção dispara `purge_derived` + detach de
  arestas; sweep remove só `*.tmp`/`*.stale` > 30 s, **nunca** `*.lock`.
- **Evento** `{id, op, note_id?, at, actor?, data?}`;
  `id = evt_<base36(8)>` (dedup on-read); rotação por tamanho; checkpoint
  `.idx/events.checkpoint`.
- **Leitura única:** `Corpus {notes, index, graph}` lê as notas uma vez e deriva
  índice+grafo do mesmo vetor.

## 5. Determinismo (requisito, não bônus)

- **Ordem canônica** em toda iteração: `HashMap`/`HashSet` **proibidos** (use
  `BTreeMap`/`IndexMap`); `LinkedList` também.
- Tempo/RNG/Env/FS só via portas (nunca `SystemTime::now`, `env::var`, etc.).
- **Tie-breaks explícitos:** RRF `(score desc, id asc)`, rank
  `(confidence desc, id asc)`, impacto `(impacto desc, created asc, id asc)`,
  sugestões `(score desc, from asc, to asc)`.
- Algoritmos determinísticos: PageRank/PPR, Louvain, MinHash/LSH.
- `Timestamp` UTC em ms; comparações com `total_cmp`; `NaN` tratado como `0`.

## 6. Erros (R30–R35)

- `enum Error` `#[non_exhaustive]`, `Send + Sync + 'static`; **nunca**
  `Box<dyn Error>` na API pública do core.
- Construtores nomeados: `Error::schema`, `invalid_input`, `conflict`,
  `not_found`, `timeout`, `config`, `internal`; I/O **sempre** com
  `Error::io(path, source)`.
- `ErrorKind::code()` é o contrato de máquina; `exit_code()` mapeia:
  `invalid_input=2, not_found=3, conflict=4, io=5, timeout=6, config=7,
  schema=8, unsafe_blocked=9, internal=70`; **101** reservado a panic.
- `retryable()` só para `Timeout`.
- Poison de mutex: `lock_or_recover` (nunca `.expect("poisoned")`).
- **Degradação graciosa** (R33): canal opcional que falha → resultado parcial +
  `warnings[]`; `strict` promove a erro.

## 7. Testes e verificação

| Camada | O que trava | Onde |
|---|---|---|
| Golden | bytes de `prime`/`--json`/erros/`init`/EPIPE | `knudge-cli/tests/golden*` |
| Property | TOON, RRF, decay/confiança, ids/hash | proptests |
| Stress | lock sem lost update, escritas concorrentes, leitor × rebuild | `knudge-core/tests/stress.rs` |
| Crash-injection | nota-antes-de-evento, tmp+rename, double-buffer | `FaultyFs` |
| Bordas | catálogo com o teste que trava cada uma | `DIVERGENCES.md` |
| Aceite | pipe/`--json`/erro/exit/estado por verbo | `17_matriz_aceitacao.md` |
| Dinâmica | miri no core puro; fuzz de TOON/JSONL; supply chain | `make ci` |

Regras de teste: **sem `unwrap`/`expect`/`panic`**; use `?`, `let Some(x) = …
else`, `assert!(matches!(...))`; `let _ = expr;` é proibido. Bug encontrado ⇒
adicione o **teste de regressão**.

## 8. Ferramentas de desenvolvimento

```bash
make check     # fmt --check + clippy -D warnings + test + gate de 300 linhas (portão único)
make fmt       # cargo fmt --all
make clippy    # clippy --workspace --all-targets -- -D warnings
make test      # cargo test --workspace
make build     # cargo build --workspace
make install   # release + binários + config global + completions + PATH
make dist      # release otimizado + pacote da plataforma em dist/
make ci        # check + nextest + deny + audit + machete + typos
make miri      # verificação dinâmica de UB no core puro
make fuzz      # build dos alvos de fuzz (TOON, JSONL)
make bench     # bancada (micro + e2e) → bench/ULTIMO.md
make update-version VERSION=v0.2.1
```

Toolchain: Rust **1.97+** (edição 2024), MSRV obrigatória. `rustfmt`: largura 100,
indent 4, newline Unix.

## 9. Invariantes de engenharia

- `#![forbid(unsafe_code)]` em `core`/`cli`/`mcp`.
- Sem `Rc`/`RefCell` no core; estado compartilhado via `Arc<Mutex<_>>`.
- Sem `unwrap`/`expect`/`panic` em `src/`; poison via `into_inner`.
- **Arquivos de produção ≤ 300 linhas** (gate mede `*/src/*`).
- Tipos proibidos (`clippy.toml`): `Rc`/`Weak`/`RefCell`/`Cell`, `LinkedList`,
  `HashMap`/`HashSet`.
- Macros proibidas: `dbg!`/`todo!`/`unimplemented!`/`unreachable!`/`panic!`.
- Aritmética com `overflow-checks`; prefira `checked_*`/`saturating_*`.
- Indexação `[]` negada; use `.get()`/`.first()`/iteradores.
- Todo `#[allow]` exige `reason`.
- Dependências em `[workspace.dependencies]`; justifique (orçamento R43).

## 11. Núcleo embutível e fachada (D214 — v0.5.2)

Desde a **v0.5.2** o `knudge-core` é **publicável** (`publish = true` em crates.io) e expõe uma
**fachada de incorporação** para uso como biblioteca em outro projeto Rust:

- `Knudge` / `KnudgeBuilder` — monta o núcleo reproduzindo a composição da CLI/MCP, com
  adaptadores `std` + `Project` + config.
- `knowledge_dir(".a/b")` — o diretório de conhecimento é **configurável** (default `.knudge`).
- `Project` ganha `layout` / `resolve_with` / `at` / `with_layout` / `ignored_dirs`; os padrões de
  Git (`info/exclude`, `.gitattributes`, `sync`, `AGENTS.md`) e a varredura de âncoras **derivam do
  layout**. Sem mudança de comportamento no default.

**Guia de integração:** `wiki/integration/` (indexado em `wiki/README.md` e `llms.txt`) documenta,
por subsistema, como embutir o núcleo — fachada, modelo, store, busca, escrita, tarefas/handoff,
grafo, saúde/manutenção, ciclo de vida, embeddings, git/config, erros/determinismo — com
recomendações de otimização e checklist de produção.

**Para um consumidor como o katu:** a porta `Memory` liga-se a esta fachada **in-process**, não à
CLI `kd`.

- `wiki/specs/TOON.md` — contrato de bytes.
- `wiki/specs/ARCHITECTURE.md` — camadas.
- `wiki/specs/matematica.md` — fórmulas e constantes.
- `wiki/specs/DIVERGENCES.md` — bordas + teste que trava cada uma.
- `plan/03_decisoes-fechadas.md` — decisões `D01–D214`.
- `wiki/integration/` — guia de incorporação do núcleo (fachada `Knudge`/`KnudgeBuilder`, D214).
- `plan/implementation/16_cli_surface.md` — superfície do `kd`.
- `plan/implementation/17_matriz_aceitacao.md` — matriz de aceite.
