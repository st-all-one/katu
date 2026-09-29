# 00 — knudge: visão geral e mapa do guia

> Versão de referência: **v0.5.2** (binário `kd`, Rust 1.97+, edição 2024).
> Guia denso em pt-BR, otimizado para consumo por IA, baseado no repositório
> oficial `st-all-one/knudge` (fonte em `TMP/knudge/`). Desde a v0.5.2 o núcleo é
> publicável e embutível (fachada `Knudge`/`KnudgeBuilder`, `wiki/integration/`).
> Estado do contrato: `SCHEMA_VERSION = 2`, superfície v3 (**14 verbos** + `help`),
> `INDEX_FORMAT = retrieval-v4`.

## 1. O que é o knudge

O **knudge** é uma CLI Rust (binário **`kd`**) que dá **memória durável por
projeto a agentes de IA**. Notas em **Markdown são a fonte da verdade**; um
**índice derivado e reconstruível** (BM25 + âncoras + embeddings opcional,
fundidos por RRF) acelera a busca. Sem servidor, sem banco, sem daemon. O
binário **`knudge-mcp`** expõe gatilhos de memória por **MCP** (JSON-RPC 2.0
sobre stdio).

O ciclo central é:

```
kd ask → kd write → kd task → kd sync
(buscar)  (gravar)   (executar) (commit)
```

Três camadas conceituais:

| Camada | O que é | Você mexe? |
|---|---|---|
| **Notas** (`.knudge/notas/<tipo>/<id>.md`) | verdade canônica, frontmatter **TOON** + corpo Markdown | só por `kd write`/`kd task` |
| **Eventos** (`.knudge/eventos/events*.jsonl`) | auditoria append-only | nunca (gerado) |
| **Índice** (`.knudge/.idx/`) | derivado, descartável e reconstruível | nunca (`kd doctor --fix`) |

**Regra de ouro:** se o índice divergir, ele é reconstruído; as notas nunca
dependem dele.

## 2. Por que existe (problema → solução)

| Sintoma sem memória | Mecanismo do knudge |
|---|---|
| Redescobrir o mesmo a cada sessão | `kd ask` responde em uma linha |
| Conhecimento duplicado/contraditório | dedup na escrita (0,75/0,92) + detecção de contradição |
| Decisões perdidas em conversas | nota versionada com `--type decision` e o "porquê" |
| Tarefas esquecidas entre sessões | `kd task` + `kd rewind` |
| Dúvida sobre o que é atual | shelf-life, drift e `kd forget` aposentam o obsoleto |
| Contexto caro e ruidoso | ponteiros curtos por âncora, revelação progressiva |

O que ele **não é**: chatbot, banco de dados, serviço de nuvem, wiki com ACL,
indexador de PDFs/documentos grandes. É memória de **projeto**, em texto,
versionada no seu git.

## 3. Arquitetura em uma tela

```
knudge-mcp ──┐
             ├── knudge-core (núcleo puro + portas + adaptadores std)
knudge-cli ──┘
```

| Crate | Papel |
|---|---|
| **`knudge-core`** | modelo, schema, TOON, retrieval, grafo, tarefas, ciclo de vida, **portas** — lógica pura; `adapters` (std) isolado |
| **`knudge-cli`** | binário **`kd`**: `clap`, envelope `--json`, logging, montagem de adaptadores e saída |
| **`knudge-mcp`** | servidor **MCP**: motor de gatilhos + transporte JSON-RPC stdio |

O domínio depende de **portas** (`Clock`, `Rng`, `Env`, `Fs`, `Git`,
`HookRunner`, `Logger`, `Embedder`), nunca de SO. Invariantes de engenharia:
`#![forbid(unsafe_code)]`, sem `unwrap`/`expect`/`panic`, arquivos de produção
≤ 300 linhas, iteração determinística (`BTreeMap`/`IndexMap`).

## 4. As sete ideias centrais

1. **A nota é a verdade; o índice é um atalho.** Apagar `.idx/` nunca perde
   conhecimento.
2. **Uma afirmação por nota.** Curta, autocontida; o `id` deriva do conteúdo
   (`<tipo>_<base36(8)>`). Reclassificar o tipo **não** reescreve o `id`.
3. **Busque antes de gravar.** `kd ask "<rascunho>"` → dedup: `<0,75` cria,
   `0,75–0,92` funde, `≥0,92` rejeita.
4. **Âncoras ligam memória ao código.** `--anchor src/x.rs` (globs `src/**`);
   `kd ask --anchor PATH` busca sem query.
5. **O knudge propõe; você decide.** `doctor`/`learn`/`compact`/`prune` só
   propõem; aplica-se via `write`/`write --link`/`forget`.
6. **Contexto pequeno, resposta direta.** Saída padrão =
   `id|statement|score|why`; corpo só sob demanda.
7. **Determinismo e simplicidade.** Mesma entrada → mesma saída, byte a byte;
   sem nuvem, sem DB, sem daemon.

## 5. Superfície da CLI (14 verbos, D209)

Top-level: `init prime rewind ask write task map maintenance doctor drain config
forget sync self` (+ `help`; `kd` sozinho = `kd help`).

| Grupo | Verbos |
|---|---|
| **Domínio (8)** | `ask`, `write`, `task`, `rewind`, `map`, `doctor`, `drain`, `forget` |
| **Fundação/meta (6)** | `init`, `prime`, `sync`, `config`, `self`, `maintenance` |

## 6. Como este guia está organizado

| Bloco | Arquivos | Foco |
|---|---|---|
| Fundamentos | `01` | Filosofia, quando adotar, instalação, quickstart |
| Núcleo | `02`, `03` | Ciclo, convenções da CLI, modelo de dados e TOON |
| Escrita/leitura | `04`, `05` | `write`/dedup/update/link; `ask`/ranking/temporal |
| Estruturas | `06`, `07`, `08` | Grafo/ontologia; tarefas; handoff/rewind |
| Operação | `09`, `10`, `11` | Manutenção/saúde/ciclo de vida; config/git; embeddings/worker |
| Integração | `12`, `13` | MCP; arquitetura, determinismo e qualidade |
| Apoio | `14`, `15` | Troubleshooting; referência rápida |

## 7. Ordem de leitura (token-efficient)

Sempre `00` → `01` → `02`. Para **gravar**: `03`+`04`. Para **buscar**: `05`.
Para **planejar**: `07`+`08`. Para **operar**: `09`+`10`. Para **semântica**:
`11`+`12`. Para **contribuir no código**: `13`. Erros: `14`. Consulta: `15`.

## 8. Regras de ouro (resumo executável)

- `kd prime` = protocolo estático (uma vez por sessão); `kd prime --long`
  anexa a gramática TOON e o schema.
- **Nunca invente `id`** — copie do output; o id é derivado do conteúdo.
- `kd write` **rejeita** `--type task`/`epic` — use `kd task`.
- Arestas têm **via única**: `kd write --link "<FROM:ARESTA:TO>"`.
- `forgotten`/`superseded` ficam fora de `ask`/`rank`/`map`/`rewind`/
  `maintenance` por padrão (inclua com `--status`).
- **stdout = dados; stderr = logs.** Em `--json`, stdout é só o envelope.
- `--json` sem verbo → exit 2; busca vazia → `[no_results]`, exit 0;
  pipe fechado (EPIPE) → exit 0.
- Nada de `unwrap`/`expect`/`panic`/`unsafe` no código; arquivos ≤ 300 linhas.
