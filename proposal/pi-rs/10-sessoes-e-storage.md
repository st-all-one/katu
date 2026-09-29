# 10 — Sessões e Storage

## 1. Formato de sessão (JSONL em árvore)

Fonte: `docs/session-format.md`. Arquivo:

```
~/.pi/agent/sessions/--<path>--/<timestamp>_<session-id>.jsonl
```

- `<path>`: separador inicial removido; `/`, `\`, `:` → `-`.
- Cada linha é um JSON com `type`.
- **Header** (1ª linha): `{type:"session", version, id, timestamp, cwd, parentSession?}`.
- Versões: v1 linear (legado, migrado), v2 árvore (`id`/`parentId`), v3 renomeia role `hookMessage` → `custom`. Migração automática ao carregar.

### `SessionEntryBase`
```rust
struct SessionEntryBase { id: String, parent_id: Option<String>, timestamp: String /*ISO8601*/ }
```
`id`: normalmente hex de 8 chars; pode cair para UUID. `timestamp` ISO 8601 (diferente do timestamp da mensagem, que é Unix ms).

### Tipos de entrada

| `type` | Papel | Contexto LLM? |
|---|---|---|
| `session` | header (metadados) | não |
| `message` | `AgentMessage` (user/assistant/toolResult/system) | sim |
| `model_change` | troca de modelo | não |
| `thinking_level_change` | troca de thinking | não |
| `usage` | usage atribuído (ex.: cache warm) | não |
| `compaction` | summary + checkpoint (`firstKeptEntryId` obrigatório) | sim (summary) |
| `context_edit` | edição append-only de entrada anterior | altera projeção |
| `branch_summary` | resumo do branch abandonado (`fromId`) | sim |
| `custom` | estado de extensão (`customType`, `data`) | não |
| `custom_message` | mensagem injetada por extensão (`content`, `display`) | sim |
| `label` | bookmark (`targetId`, `label`) | não |
| `session_info` | nome de sessão (`name`) | não |

### Regras de context building (`buildContextEntries` → `buildSessionProjection` → `buildSessionContext`)
1. Caminha do leaf até a raiz.
2. Se há `CompactionEntry` no caminho, usa a **mais recente**: inclui a compaction primeiro; inclui entradas não-system de `firstKeptEntryId` até (excluindo) a compaction; inclui entradas após a compaction.
3. Preserva entradas não-message para o TUI.
4. `context_edit`: aplica o edit mais recente por alvo no branch ativo; `replacement: null` omite; `replacement` troca só o conteúdo mantendo role/metadados. Edits são relativos ao branch.
5. `usage`/`custom` não geram mensagem; `compaction` gera checkpoint system + `compactionSummary`; `branch_summary` gera `branchSummary`; `custom_message` gera `CustomMessage`.

### System messages como deltas
O primeiro request persiste um `system` com todas as `sections` e declarações de tools. Mudanças posteriores persistem `system` patchando `sections` por nome (`null` remove) e listando `toolsAdded`/`toolsRemoved`. `replace: true` estabelece nova baseline. Reexecutar em ordem reconstrói prompt+tools.

## 2. `SessionManager`

Autoritativo para o contexto finalizado:
- `messages`, `model`, `thinking_level`, `system_prompt` (read-only).
- Navegação de árvore: leaf ativo, `resetLeaf`, branch.
- Operações: `newSession`, `switchSession`, `fork`, `clone`, `importFromJsonl`.
- `SessionManager::in_memory()` para hosts sem arquivo.

Em Rust:
```rust
pub struct SessionManager { entries: Vec<SessionEntry>, leaf: Option<String>, storage: Box<dyn SessionStorage> }
impl SessionManager {
    pub fn in_memory() -> Self;
    pub fn append(&mut self, entry: SessionEntry) -> Result<()>;
    pub fn build_context(&self) -> SessionContext;
    pub fn branch(&mut self, from: &str) -> Result<()>;
}
```

Preservar campos exatos (`firstKeptEntryId`, `fromId`, `customType`, `targetId`, `label`, `tokensBefore`) para que sessões do Pi original abram no `pi-rs` e vice-versa.

## 3. Backends de storage

O `pi-agent` tem storage de sessão em memória e JSONL; o `pi-durable` (Pico) tem memória/JSONL/SQLite; o `sqlite-node` é o backend Node.

| Backend | Uso |
|---|---|
| memória | testes/SDK efêmero |
| JSONL | padrão do CLI; 1 owner serializa writes; sem lock cross-process |
| SQLite | `pi-durable`/`sqlite-node`; WAL, `synchronous=NORMAL`; migrações ordenadas |

Em Rust:
- JSONL: `serde_json` linha a linha; `fsync` opcional; append com marker de commit.
- SQLite: `rusqlite` (bundled) com WAL; portar as migrações. `sqlite-node` some.
- Storage trait para conformance (portar os casos do `pi-durable/testing`).

## 4. `pi-durable` (runtime durável "Pico")

Contratos: conversa, tarefa, documento; IDs; entries; `env` (capabilities); storage memory/JSONL/SQLite; testing (conformance + benchmarks). Docs: `packages/durable/docs/pico-v5*.md`.

Necessário apenas se o modo durável multi-processo (session workers, coordenação, replicação) for mantido. Para o MVP, a sessão JSONL de `pi-agent` cobre o caso de uso do CLI.

## 5. Migrações

- Ao carregar, migrar v1→v2→v3.
- `coding-agent/src/migrations.ts` também migra configs/deprecações; portar avisos de deprecação.
- Em Rust, manter funções de migração puras + testes com fixtures de cada versão.

## 6. Export/import

- `/export` gera HTML autocontido (ver 08 §7).
- `/import` importa JSONL.
- `session-export.ts`, `session-share.ts` (share via pi-share-hf) — share pode ficar de fora do MVP.

## 7. Fila de escrita e locks

- `proper-lockfile` (TS) → `fs2`/`fd-lock` em Rust.
- Um owner serializa writes; sem alocação de ID cross-process (documentado).
- `file-mutation-queue` para edit/write (ver 07 §8).

## 8. Ordem de port

1. `SessionEntry` + parser/serializer JSONL (todas as variantes).
2. `SessionManager` em memória + `buildContext`.
3. Migrações v1→v3 com fixtures.
4. Storage JSONL em disco (append, load, fork).
5. `context_edit`, compactação, branch summary.
6. SQLite + conformance (fase posterior, se necessário).
