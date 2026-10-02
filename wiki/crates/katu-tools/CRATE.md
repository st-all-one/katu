# `katu-tools` — Ferramentas e Contenção Soft

**Épico:** E06/E07 · **Crate:** `crates/katu-tools` · **Tipo:** crate puro (sem providers)

As **ferramentas** do core e a **contenção determinística soft**. Nenhuma operação sensível passa
sem veredicto; controlo em falta = recusa.

---

## 1. Visão Geral

O crate `katu-tools` implementa a **superfície fechada de tools** e a **contenção soft**:

- **Registry fechado** de 11 tools (`registry.rs`)
- **Schema declarativo** com linter anti-*prompt-poisoning* (`schema/`)
- **Executores** das tools: `read`/`write`/`edit`/`move`/`trash`/`bash`/`grep`/`find`/`ls`
- **Grupo de controlo**: `plan` e `memory` (recall + record)
- **Helpers**: diff unificado, outline heurístico, resolução de caminhos, linguagem

**Fronteira:**
- Depende de `katu-policy`/`katu-core`; **não** depende de `katu-providers`/`katu-tui` nem de
  `knudge-core`.
- Honestidade: soft **não** é fronteira de segurança (a jail real é E17/futura).
- `#![forbid(unsafe_code)]`.

---

## 2. Arquitetura

### 2.1 Composição

```
┌─────────────────────────────────────────────────────────────────┐
│                     katu-tools (crate puro)                     │
├─────────────────────────────────────────────────────────────────┤
│  registry (superfície fechada)  │  schema (linter + JSON)       │
├─────────────────────────────────────────────────────────────────┤
│  Executores (implementam o trait Tool de katu-core)            │
│  read │ write_file │ edit │ move_file │ trash │ exec           │
│  search │ plan │ write(note) │ recall                          │
├─────────────────────────────────────────────────────────────────┤
│  Helpers                                                        │
│  diff │ outline │ lang │ resolve                                │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  katu-core::kernel::Tool (trait) + katu-policy::ToolUse         │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2 Módulos

| Módulo | Responsabilidade |
|--------|------------------|
| `lib.rs` | Declaração dos módulos públicos |
| `registry.rs` | Superfície fechada de 11 tools (`TOOLS`) |
| `schema/mod.rs` | Linter de schema, `Concurrency`, `catalog` |
| `schema/specs.rs` | As 11 especificações declarativas (`SCHEMAS`) |
| `schema/json.rs` | JSON Schema (`tool_defs`) + forma wire (`wire_json`) |
| `diff.rs` | Diff unificado determinístico (O(n)) |
| `edit.rs` | Tool `edit` otimista multi-bloco atómica |
| `read/mod.rs` | Tool `read` com views |
| `read/views/` | Construtores de relatório por view |
| `write_file.rs` | Tool `write` de ficheiro novo |
| `move_file.rs` | Tool `move` (rename atómico) |
| `exec.rs` | Tool `bash` (Process + spill) |
| `search/` | Tools `grep`/`find`/`ls` + walk |
| `trash/` | Tool `trash` + índice append-only |
| `plan.rs` | Tool `plan` (validação de plano tipado) |
| `write.rs` | Tool `memory` (record) |
| `recall.rs` | Tool `memory` (recall) |
| `lang.rs` | Linguagem por extensão + conversões saturantes |
| `outline.rs` | Scanner heurístico de símbolos (Rust-first) |
| `resolve.rs` | Resolução canónica de caminhos (symlinks) |

---

## 3. Módulos em Detalhe

### 3.1 Registry (`src/registry.rs`)

A superfície é **exatamente** as famílias de `00b` §1.1; qualquer adição exige uma decisão
registada (DF12). A ordem é canónica (estável entre execuções).

**Famílias:**

```rust
pub enum Family {
    Write,   // write, edit, move, trash
    Read,    // read
    Exec,    // bash
    Search,  // grep, find, ls
    Plan,    // plan
    Control, // memory
}
```

**As 11 tools** (`TOOLS`):

| ToolId | Nome ao modelo | Família | Comandos de política |
|--------|----------------|---------|----------------------|
| `Write` | `write` | Write | `ToolName::Write` |
| `Edit` | `edit` | Write | `ToolName::Edit` |
| `Move` | `move` | Write | `ToolName::Move` |
| `Trash` | `trash` | Write | `ToolName::Trash` |
| `Read` | `read` | Read | `ToolName::Read` |
| `Bash` | `bash` | Exec | `ToolName::Exec` |
| `Grep` | `grep` | Search | `ToolName::Search` |
| `Find` | `find` | Search | `ToolName::Search` |
| `Ls` | `ls` | Search | `ToolName::Search` |
| `Plan` | `plan` | Plan | `ToolName::Plan` |
| `Memory` | `memory` | Control | `MemoryWrite` + `MemoryRecall` |

**API:**
- `is_registered(name: ToolName) -> bool` — pertence à superfície fechada?

**Testes:** garante 11 tools, ids únicos, contagem por família (4/1/1/3/1/1), ids minúsculos.

### 3.2 Schema (`src/schema/`)

#### 3.2.1 Linter (`src/schema/mod.rs`)

Valida os esquemas e devolve problemas **agregados** (`katu_core::validate::Issues`), cada um
apontando o campo exato e ensinando a corrigir.

**Constantes:**

```rust
const VERBS: &[&str] = &["read", "write", "edit", "move", "trash", "find", "grep",
    "ls", "exec", "bash", "search", "plan", "memory", "record", "recall", "close",
    "outcome", "compact", "model", "thinking"];

const POISON: &[&str] = &["<system>", "<assistant>", "</", "<!--", "```",
    "ignore previous", "ignore all previous", "disregard"];

pub const MAX_DESCRIPTION_CHARS: usize = 1024;
```

**Tipos de parâmetro (`ParamKind`):**

| Tipo | JSON Schema |
|------|-------------|
| `Text` | `{"type":"string"}` |
| `Integer` | `{"type":"integer"}` |
| `Boolean` | `{"type":"boolean"}` |
| `ListText` | `{"type":"array","items":{"type":"string"}}` |
| `Path` | `{"type":"string"}` (resolvido antes da política) |
| `Enum(&[...])` | `{"type":"string","enum":[...]}` (conjunto fechado) |
| `Id(pattern)` | `{"type":"string","pattern":...}` (obrigatório, não vazio) |

**Validações:**
- **Nome**: obrigatório, `snake_case`, sem *poisoning*, primeira palavra num `VERB`.
- **Descrição**: obrigatória, contém `"Use when"` e `"Do not use for"`, dentro do teto, sem
  *poisoning*.
- **Parâmetros**: nomes únicos `snake_case`, descrição não vazia, sem *poisoning*, enums não
  vazios e sem duplicados, ids com `pattern` não vazio.

**Concorrência (`Concurrency`):**

```rust
pub enum Concurrency {
    #[default]
    Exclusive, // Corre sozinha (fail-closed)
    Shared,    // Só lê: pode correr em paralelo
}

const SHARED_TOOLS: &[&str] = &["read", "grep", "find", "ls"];

pub fn concurrency_of(name: &str) -> Concurrency { ... }
```

> **Fail-closed:** tool desconhecida ⇒ `Exclusive`.

**Catálogo (`catalog`):** gera uma `RowTable` TOON com nome + assinatura compacta (domínios fechados
inline, opcionais com `?`), para o prime (ADR 0006).

#### 3.2.2 Especificações (`src/schema/specs.rs`)

As 11 especificações declarativas, na mesma ordem canónica do registry:

```rust
pub const SCHEMAS: &[ToolSchema<'static>] =
    &[READ, WRITE, EDIT, MOVE, TRASH, BASH, GREP, FIND, LS, PLAN, MEMORY];
```

**Exemplos de descrições (que ensinam):**
- `read`: "Use when you need file contents or structure. Do not use for searching many files (use
  grep or find)."
- `write`: "Use when creating a brand-new file. Do not use for changing an existing file (use
  edit)."
- `edit`: "…One call may carry several replacements, applied in order and all-or-nothing."
- `bash`: "Use when running a program with known argv. Do not use for evaluating a shell string."

#### 3.2.3 JSON Schema (`src/schema/json.rs`)

- `tool_defs() -> Vec<ToolDef>`: deriva de `SCHEMAS` (fonte única já validada), pelo que **não há
  *drift*** entre o que o modelo vê e o que o roteador aceita.
- `property(param)`: inclui a **descrição** que ensina o modelo (Q-06).
- `wire_json(tool)`: forma wire OpenAI (`{"type":"function","function":{…}}`).

### 3.3 Diff (`src/diff.rs`)

Diff unificado determinístico, sem dependências e sem LLM. Compara duas versões por linhas com
**prefixo/sufixo comum** (O(n)); o miolo é removido/adicionado. Evita a matriz O(n·m) de um LCS
completo.

**Tipos:**

```rust
pub enum DiffLine { Context(String), Remove(String), Add(String) }

pub struct Hunk {
    pub old_start: u32, pub old_len: u32,
    pub new_start: u32, pub new_len: u32,
    pub lines: Vec<DiffLine>,
}

pub struct Diff {
    pub hunks: Vec<Hunk>,
    pub added: u32,
    pub removed: u32,
}

pub fn unified(old: &str, new: &str, context: usize) -> Diff { ... }
```

### 3.4 Tool `edit` (`src/edit.rs`)

Patch **otimista** com *compare-and-swap* (`Fs::write_atomic_if`). Se o ficheiro mudou entretanto,
devolve `Unavailable { control: "stale" }` — **recuperável** ("relê e reaplica"), nunca sobrescreve
edição concorrente.

**Multi-bloco atómico:** uma chamada aplica uma **lista** de substituições, por ordem, com semântica
*tudo ou nada*. Se uma não casar exatamente uma vez, **nada é gravado** e o relatório diz **qual**
falhou e **que âncoras únicas existem perto**.

**Tipos:**

```rust
pub struct Replacement { pub old: String, pub new: String }

pub enum FailureKind { NotFound, Ambiguous }

pub struct EditFailure {
    pub index: usize,          // Qual substituição falhou
    pub kind: FailureKind,     // Porquê
    pub anchors: Vec<String>,  // Âncoras únicas mais próximas
}

pub struct EditFileTool<'a> {
    pub fs: &'a dyn Fs,
    pub replacements: Vec<Replacement>,
    pub dry_run: bool,
}
```

**Constantes:** `MIN_ANCHOR_CHARS = 3`, `MAX_ANCHORS = 3`.

**Fluxo (`execute`):**
1. Lê o ficheiro atual.
2. `apply(&text, &replacements)` — aplica atomicamente numa cópia.
3. Se falhar → `rejected(...)` (relatório `edit.rejected` que **ensina**, com âncoras + remédio).
4. Se `dry_run` → `edit.dry-run`.
5. `write_atomic_if` → `edit.patch`.

**Relatórios:** `edit.patch`, `edit.dry-run`, `edit.rejected` (com `anchors` e `remedy`).

**Helpers de âncoras:** `nearest_anchors`, `match_lines`, `is_unique`, `common_prefix_chars`,
`describe` (formato `"linha: texto"`).

### 3.5 Tool `read` (`src/read/`)

Devolve metadados e ponteiros, não um despejo.

**Views (`View`):**

| View | Descrição |
|------|-----------|
| `Full` | Conteúdo completo (truncado) |
| `Range` | Range de linhas |
| `Outline` | Só símbolos |
| `Summary` | Outline + imports + flags (**default**) |
| `Symbol` | Corpo de um símbolo |
| `Diff` | Diff contra `base` |

**Orçamento (`ReadBudget`):** `max_lines: 400`, `max_bytes: 24_000` (default).

```rust
pub struct ReadTool<'a> {
    pub fs: &'a dyn Fs,
    pub view: View,
    pub range: Option<LineRange>,
    pub symbol: Option<String>,
    pub base: Option<String>,
    pub budget: ReadBudget,
}
```

**Relatórios (por view):** `read.full`, `read.range`, `read.outline`, `read.summary`,
`read.symbol`, `read.diff`.

**Truncagem determinística:** `full` devolve `Page { cursor, total, truncated }` e `next`
(`read <id>@<linha>`) quando truncado — mas nunca um cursor que não avança (`shown > 0`).

### 3.6 Tool `write` (`src/write_file.rs`)

Só ficheiros **novos**; existentes via `edit`.

```rust
pub struct WriteFileTool<'a> {
    pub fs: &'a dyn Fs,
    pub content: Vec<u8>,
}
```

**Fluxo:** recusa se `fs.exists(target)` (`Unavailable { control: "exists" }`) → `write_atomic` →
relatório `write.file` com `id`, `hash` e `next` (`read <id>`).

### 3.7 Tool `move` (`src/move_file.rs`)

Renomeação **atómica** (`Fs::rename`) sob escopo, sem sobrescrever o destino (fail-closed: nunca
há *clobber* silencioso).

**Fluxo:** lê a origem → recusa se o destino existe → `rename` → relatório `move.file` com
`from`/`to`/`from_id`/`to_id`/`bytes` e `next` (`read <to_id>`).

### 3.8 Tool `bash` (`src/exec.rs`)

Execução com `argv`/`cwd` resolvidos, ambiente filtrado de segredos e timeout. A política decide
**antes**; aqui só se executa o veredicto. Sem regex sobre a string.

```rust
pub struct ExecTool<'a> {
    pub process: &'a dyn Process,
    pub env: &'a dyn Env,
    pub fs: &'a dyn Fs,
    pub root: &'a Path,
    pub timeout_ms: u64,
    pub parent: Option<String>,
}
```

**Constantes:**
- `DEFAULT_TIMEOUT_MS = 30_000`
- `SPILL_DIR = ".katu/spill"`
- `SECRET_MARKERS = ["KEY", "SECRET", "TOKEN", "PASSWORD", "PASSWD", "CREDENTIAL"]`

**`scrub_env`:** remove variáveis sensíveis do ambiente herdado.

**`render_with_spill`:** se o output exceder o teto do `Ledger`, o texto (já redigido) é vertido
para `.katu/spill/<id>.<stream>`; best-effort (se falhar, mantém head/tail sem ponteiro).

**Relatório:** `exec.run` com `argv`/`cwd`/`exit`/`signal`/`timed_out`/`duration_ms`/`stdout`/
`stderr` (+ `stdout_spill`/`stderr_spill` quando há).

**Outcomes ortogonais:** `exit_code`, `signal`, `timed_out` (nunca `ProcessError::NotFound` →
`Unavailable { control: "not-found" }`).

### 3.9 Tool `search` (`src/search/`)

Varredura **determinística** (ordem canónica, sem `HashMap`), com limite de resultados e informação
negativa.

```rust
pub struct SearchTool<'a> {
    pub fs: &'a dyn Fs,
    pub limit: usize,   // DEFAULT_LIMIT = 200
}

pub fn search_use(root: &ResolvedPath, query: impl Into<String>, mode: SearchMode) -> ToolUse
```

**Modos:**

| Modo | Módulo | Relatório | Descrição |
|------|--------|-----------|-----------|
| `Grep` | `grep.rs` | `search.grep` | Conteúdo, classificado e clusterizado por símbolo |
| `Find` | `map.rs` | `search.find` | Nomes por relevância |
| `Ls` | `map.rs` | `search.ls` | Mapa semântico do diretório |

**`search_use`** traz a **raiz resolvida** em `resolved_paths`, para a política avaliar
`DenyRead`/`DenySensitiveRead` sobre o que a busca vai varrer.

**Walk (`walk.rs`):**
- `MAX_DEPTH = 16`
- `IGNORED = ["target", "node_modules", ".katu", ".venv", "dist", "build"]` (além de dotfiles)
- `MAX_FILES = 4096`
- Lê `.gitignore` da raiz (padrões simples; `!`/comentários ignorados).

**`grep`:** classifica cada hit (`Test`/`Source` por `#[test]`/`#[cfg(test)]`), clusteriza por
símbolo (`BTreeMap`), e reporta `scanned`/`searched`/`hits`/`clusters`/`negative`.

**`find`:** ranking determinístico — `name == needle` → 3, `name.contains(needle)` → 2, `path`
contém → 1, senão 0.

**`ls`:** entradas com tipo (`dir`) ou linguagem + símbolos (mapa semântico).

### 3.10 Tool `trash` (`src/trash/`)

A lixeira é **por projeto** (`<root>/.katu/trash`), fora do escopo de escrita normal e nunca
servida ao modelo por omissão. `trash` **move** (não copia+apaga).

```rust
pub struct TrashTool<'a> {
    pub fs: &'a dyn Fs,
    pub clock: &'a dyn Clock,
    pub root: PathBuf,
}
```

**Fluxo:**
1. Lê o original (guarda os bytes para o hash).
2. `unique(original, now)` — preserva o relativo; desambigua em colisão (`MAX_COLLISIONS = 1000`).
3. `create_dir_all(parent)` → `rename` → `index::append`.
4. Relatório `trash.move` com `original`/`stored`/`undo_token`/`refs`/`bytes`/`at` e `next`
   (`restore <token>`).

**Funções públicas:**
- `list(fs, root) -> Vec<TrashItem>` — do mais recente para o mais antigo; filtra pela
  **existência** do ficheiro guardado (índice append-only preservado como rasto de auditoria).
- `restore(fs, root, token)` — **sempre permitido** (não passa pela política); recusa se o original
  está ocupado (`TrashError::Occupied`) ou o token é desconhecido (`TrashError::Unknown`).
- `empty(fs, root)` — **destrutivo**, remove permanentemente os ficheiros guardados.

**Índice (`trash/index.rs`):** append-only em `<root>/.katu/trash/index.tsv`. Formato por linha:
`at_millis \t stored \t original`, com `\`, `\t` e `\n` escapados. Apenas se **anexa** (nunca se
reescreve).

### 3.11 Tool `plan` (`src/plan.rs`)

Valida e reporta um **plano tipado** (`katu_core::plan::Plan`). A validação é de schema
(`Plan::validate`): sem `forbidden_files`, sem `rollback_plan` ou com mais de uma feature
`in_progress`, o plano **não** é aceite (fail-closed).

```rust
pub struct PlanTool { pub plan: Plan }
```

**`control_of`** mapeia o erro num `ControlId` acionável:
- `EmptyFeatureList` → `plan-features`
- `MissingForbiddenFiles` → `plan-forbidden`
- `MissingRollbackPlan` → `plan-rollback`
- `MultipleInProgress` → `plan-in-progress`

**Relatório:** `plan.validate` com features/in_progress/forbidden_files/allowed_files/rollback.

### 3.12 Tool `memory` (`src/write.rs` + `src/recall.rs`)

A tool `memory` só **pede**; o gate (`pre_write`/dedup/âncora) vive no kernel e não é contornável.

**`RecallTool`** (`recall.rs`):
```rust
pub struct RecallTool<'a> {
    pub memory: &'a dyn Memory,
    pub req: RecallReq,
}
```
- `memory.search(&req)` → relatório `memory.recall` com `query` + `hits` (cada hit com `rank`,
  `note`, `statement`, `score` em pontos base/10, `basis`, e `ev` = âncora se houver).
- Erro retryable → `Timeout`; senão → `Unavailable { control: "memory" }`.

**`WriteNoteTool`** (`write.rs`):
```rust
pub struct WriteNoteTool<'a> {
    pub memory: &'a dyn Memory,
    pub req: PreWriteReq,
}
```
- `memory.record(&req)` → relatório `memory.record` com `note`.
- Erro retryable → `Timeout`; senão → `Unavailable { control: "memory" }`.

### 3.13 Helpers

**`lang.rs`:**
- `language(path) -> &'static str` — por extensão (`rs`→`rust`, `py`→`python`, `js`→`javascript`,
  `ts`→`typescript`, `go`→`go`, `toml`→`toml`, `json`→`json`, `md`→`markdown`, resto `text`).
- `len_u64(usize) -> u64` e `to_i64(u64) -> i64` — conversões **saturantes**.

**`outline.rs`** — scanner heurístico de símbolos (Rust-first), **sem tree-sitter**:

```rust
pub enum SymbolKind { Fn, Struct, Enum, Trait, Impl, Mod, Const, Static, Type, Macro, Use }

pub struct Symbol {
    pub kind: SymbolKind,
    pub name: String,
    pub start: u32,  // 1-based
    pub end: u32,    // 1-based, inclusiva
}
```

- `outline(text) -> Vec<Symbol>` — ordem do ficheiro; símbolos **aninhados** contam.
- `normalize` remove `pub`/`pub(...)`/`async`/`unsafe`/`default`; `const fn` vira `fn`.
- `keyword` reconhece prefixos; `block_end` conta chaves (sem `{` na 1.ª linha → própria linha).
- **Não** é um parser: não ignora `{}` dentro de strings ou comentários.

**`resolve.rs`** — resolução canónica **antes** do veredicto (E07-T02):

```rust
pub fn resolve(fs: &dyn Fs, path: &Path) -> Result<ResolvedPath, ResolveError>
```

- Canonicaliza via porta `Fs` (segue symlinks).
- A política nunca vê um caminho relativo nem um symlink por resolver.
- `ResolveError` (não-exaustivo): `Fs(FsError)` / `Policy(PolicyError)`.

---

## 4. Abordagens de Engenharia

### 4.1 Superfície Fechada

**Princípio:** As tools são exatamente 11, registadas em `TOOLS` com ordem canónica. Adições exigem
decisão registada (DF12).

**Benefício:** Sem *drift* entre registry, schema e executores; o catálogo do modelo é derivado de
uma fonte única.

### 4.2 Linter de Schema anti-*Poisoning*

**Princípio:** Cada nome/descrição/parâmetro é validado por `lint`/`lint_all`, que rejeita
marcadores de *prompt poisoning* (`<system>`, `ignore previous`, `<!--`, ` ``` `, …).

**Benefício:** O `xtask check-schemas` corre no CI; a descrição é obrigada a ensinar
(`Use when … Do not use for …`).

### 4.3 Descrições que Ensinam

**Princípio:** A descrição de cada parâmetro viaja no JSON Schema (`property`), e a descrição da
tool segue o padrão `Use when X. Do not use for Y.`.

**Benefício:** O modelo decide pelo uso e não adivinha o sentido dos campos (Q-06).

### 4.4 Fail-Closed

**Princípio:** Na dúvida, recusa.

**Exemplos:**
- Tool desconhecida → `Concurrency::Exclusive` (não paraleliza).
- `edit` que não casa exatamente uma vez → nada é gravado.
- `write`/`move` para destino existente → `Unavailable { control: "exists" }`.
- `argv` em falta em `bash` → `Unavailable { control: "argv" }`.
- `stale` no `edit` → `Unavailable { control: "stale" }` (recuperável).

### 4.5 Contenção Soft (Honesta)

**Princípio:** A contenção é **soft**: um comando lançado fora das tools do katu continua a correr.

**Benefício:** A limitação é **declarada** (`tests/containment_gate.rs`), nunca escondida. O
utilizador nunca acha que há uma jail que não existe.

### 4.6 Atomicidade e *Compare-and-Swap*

**Princípio:** `edit` usa `write_atomic_if` (CAS): só grava se o conteúdo atual casar com o lido.
`write`/`move` usam `write_atomic`/`rename` atómicos.

**Benefício:** Nunca sobrescreve edição concorrente; um `edit` multi-bloco é *tudo ou nada*.

### 4.7 Orçamento Determinístico

**Princípio:** `read` trunca de forma determinística (`ReadBudget`), `search` limita resultados e
usa ordem canónica (sem `HashMap`), `grep` clusteriza em `BTreeMap`.

**Benefício:** Mesma entrada → mesma saída; testável e previsível.

### 4.8 Resolução de Caminhos Antes do Veredicto

**Princípio:** `resolve` canonicaliza (segue symlinks) **antes** de construir o `ToolUse`. Um link
dentro do workspace que aponta para fora resolve-se para fora.

**Benefício:** A política vê o caminho real, nunca um relativo ou um symlink por resolver (E07-T02).

### 4.9 Truncagem com *Spill* (*Ledger*)

**Princípio:** O output de `bash` é redigido (segredos removidos) e truncado com head/tail; se
exceder o teto, o texto completo é vertido para `.katu/spill/` e devolve-se um ponteiro.

**Benefício:** O modelo vê o essencial; o resto fica acessível sem poluir o contexto.

### 4.10 Centroides de Concorrência como Dado

**Princípio:** `SHARED_TOOLS` (read/grep/find/ls) é uma constante; `concurrency_of` é fail-closed.

**Benefício:** A classificação é **dado**, não uma convenção enterrada no loop.

### 4.11 Índice Append-Only da Lixeira

**Princípio:** O índice da lixeira só se **anexa** (nunca se reescreve); `list` filtra pela
existência do ficheiro.

**Benefício:** Rasto de auditoria preservado; `empty` remove os ficheiros mas o índice permanece.

### 4.12 Helpers Partilhados

**Princípio:** `diff`, `outline`, `lang`, `resolve` são partilhados pelas tools.

**Benefício:** Sem duplicação; `outline` é usado por `read`, `grep` e `ls`.

---

## 5. Gaps, Flags e Pendências

### 5.1 Limitações Conhecidas (Declaradas)

| Limitação | Descrição |
|-----------|-----------|
| Contenção **soft** | Não é fronteira de segurança (a jail real é E17/futura) |
| `outline` heurístico | Não é um parser: não ignora `{}` em strings/comentários; tree-sitter fica gated por medição (DF12) |
| `walk` de `search` | `MAX_FILES = 4096`, `MAX_DEPTH = 16`; padrões de `.gitignore` simples |
| `search` determinístico | Sem `HashMap` (custo de performance em troca de determinismo) |

### 5.2 Pendências / Trabalho Futuro (no código)

| Item | Descrição |
|------|-----------|
| Tree-sitter | Substituir `outline` heurístico, gated por medição (DF12) |
| Jail real | E17/futura: contenção soft → fronteira de segurança |

### 5.3 Gaps de Implementação

| Item | Descrição |
|------|-----------|
| `PlanTool` | O plano vem do artefacto carregado no arranque, **não** dos argumentos do modelo |
| `memory` | Só **pede**; o gate vive no kernel (`pre_write`/dedup/âncora) |

### 5.4 Flags / Constantes de Operação

| Constante | Valor | Uso |
|-----------|-------|-----|
| `DEFAULT_TIMEOUT_MS` | 30_000 | Timeout de `bash` |
| `DEFAULT_LIMIT` | 200 | Limite de resultados de `search` |
| `MAX_DESCRIPTION_CHARS` | 1024 | Teto da descrição de uma tool |
| `MIN_ANCHOR_CHARS` | 3 | Mínimo para uma âncora candidata |
| `MAX_ANCHORS` | 3 | Teto de âncoras devolvidas |
| `MAX_COLLISIONS` | 1000 | Colisões na lixeira antes de desistir |
| `ReadBudget::default` | 400 linhas / 24_000 bytes | Orçamento de `read` |
| `SPILL_DIR` | `.katu/spill` | Diretório de spill do `bash` |
| `MAX_FILES` (search) | 4096 | Limite de ficheiros varridos |
| `MAX_DEPTH` (walk) | 16 | Profundidade máxima da recursão |

---

## 6. Testes

### 6.1 Testes Unitários (por módulo)

| Módulo | Testes |
|--------|--------|
| `registry` | Superfície de 11 tools, famílias, ids estáveis |
| `schema` | Linter (nome/descrição/params), `concurrency_of`, catálogo |
| `edit` | Aplicação atómica, âncoras, `dry_run`, `Stale` |
| `read` | Views, truncagem determinística |
| `write_file` | Escrita de ficheiro novo, recusa de existente |
| `move_file` | Rename atómico, recusa de destino existente |
| `exec` | Exit/signal/timeout, redação de segredos |
| `search` | grep/find/ls, walk com ignore |
| `trash` | Move/list/restore/empty, índice |
| `plan` | Validação de plano |
| `write`/`recall` | Record/recall, `Unavailable` em falha |

### 6.2 Testes de Integração

| Ficheiro | Gate |
|----------|------|
| `tests/enforcement.rs` | E06-T08: cada tool negada por `dispatch` **não** produz efeito |
| `tests/containment_gate.rs` | E07-T03: controlo em falta = recusa; soft **não** confina (declarado) |
| `tests/resolve_gate.rs` | E07-T02: symlink resolvido antes do veredicto |
| `tests/memory_tool.rs` | E06-T10: a tool só pede; o gate vive no kernel |

### 6.3 Bench (dev-only)

`src/edit/tests/bench.rs` — A/B do `edit` multi-bloco (Q-07): chamadas, bytes do payload e
**atomicidade**. Corre com `--ignored`, escreve o artefacto em `KATU_EDIT_OUT`. `check-diag` proíbe
`println!` em `crates/`: o número sai para **ficheiro**.

---

## 7. Referências

- **MODULE.md:** [`crates/katu-tools/MODULE.md`](../../crates/katu-tools/MODULE.md)
- **Políticas:** [`policy/`](../../policy/)
- **Bench:** [`bench/e18/edit`](../../bench/e18/edit/PROTOCOL.md)
