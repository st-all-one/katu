# 07 — Sessão e storage

## 1. SQLite como fonte da verdade

O goose persiste **toda** a conversa e a orquestração em **SQLite**, via `sqlx`. Não há journal/replay do agente: o estado é lido da conversa persistida a cada passo.

- Arquivo: `Paths::data_dir()` (diretório de dados do goose).
- Instância global: `static SESSION_STORAGE: LazyLock<Arc<SessionStorage>>`.
- Tipo central: `SessionManager` (`session/session_manager.rs`, **5.025 linhas**).

## 2. Schema

Três tabelas centrais + controle de versão. A inicialização abre uma transação com **`BEGIN IMMEDIATE`** para evitar a corrida em que dois processos criam a tabela ao mesmo tempo (comentário no código):

```sql
CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY, applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP);

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT '',
    user_set_name BOOLEAN DEFAULT FALSE,
    session_type TEXT NOT NULL DEFAULT 'user',
    working_dir TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    extension_data TEXT DEFAULT '{}',
    total_tokens INTEGER, input_tokens INTEGER, output_tokens INTEGER,
    cache_read_tokens INTEGER, cache_write_tokens INTEGER,
    accumulated_total_tokens INTEGER, accumulated_input_tokens INTEGER,
    accumulated_output_tokens INTEGER, accumulated_cache_read_tokens INTEGER,
    accumulated_cache_write_tokens INTEGER, accumulated_cost REAL,
    schedule_id TEXT, recipe_json TEXT, user_recipe_values_json TEXT,
    provider_name TEXT, model_config_json TEXT,
    goose_mode TEXT NOT NULL DEFAULT 'auto',
    archived_at TIMESTAMP, project_id TEXT, parent_session_id TEXT
);

CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id TEXT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    role TEXT NOT NULL,
    content_json TEXT NOT NULL,
    created_timestamp INTEGER NOT NULL,
    timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    tokens INTEGER,
    metadata_json TEXT
);

CREATE TABLE IF NOT EXISTS usage_ledger (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    created_timestamp INTEGER NOT NULL,
    model TEXT,
    input_tokens INTEGER, output_tokens INTEGER, total_tokens INTEGER,
    cache_read_tokens INTEGER, cache_write_tokens INTEGER,
    cost REAL, cost_source TEXT,
    is_compaction INTEGER DEFAULT 0
);
```

Índices: `idx_messages_session`, `idx_messages_timestamp`, `idx_messages_message_id`, `idx_messages_session_created (session_id, created_timestamp, id)`, `idx_sessions_updated (updated_at DESC)`, `idx_sessions_type`, `idx_sessions_parent`, `idx_usage_ledger_session`.

### Três stores (paralelo com o Pi)

| Store | Tabela | Natureza |
|---|---|---|
| Árvore/lista de conversa | `messages` | append-only; leitura ordenada |
| Estado da sessão | `sessions` | linha mutável (update) |
| Ledger de custo | `usage_ledger` | append-only por chamada |

## 3. Modelo de `Message`

`goose-provider-types/src/conversation/message.rs`:

```rust
pub struct Message {
    pub id: Option<String>,
    pub role: Role,
    pub created: i64,
    pub content: Vec<MessageContentBlock>,
    pub metadata: MessageMetadata,
}
```

`MessageContentBlock` é uma união rica (tag `type`, camelCase):

```rust
pub enum MessageContentBlock {
    Text(TextContent),
    Image(ImageContent),
    Document(DocumentContent),
    ToolRequest(ToolRequest),
    ToolResponse(ToolResponse),
    ToolConfirmationRequest(ToolConfirmationRequest),
    ActionRequired(ActionRequired),
    Thinking(ThinkingContentBlock),
    RedactedThinking(RedactedThinkingContentBlock),
    SystemNotification(SystemNotificationContent),
    Error(ErrorContent),
}
```

`MessageMetadata` carrega o que a UI e o agente precisam sem poluir o conteúdo:

```rust
pub struct MessageMetadata {
    pub user_visible: bool,       // aparece na UI
    pub agent_visible: bool,      // entra no contexto do modelo
    pub inference: Option<InferenceMetadata>,
    pub output_token_limit_reached: bool,
    pub steer: bool,              // UI-only; nunca vai ao provider
    pub turn_context: bool,       // evento de contexto por turno
    pub usage: Option<Box<MessageUsage>>,
    pub operations: Option<Box<OperationNotes>>,  // "o que a operação fez"
}
```

Dois pontos importantes:

- **Visibilidade dupla** (`user_visible`, `agent_visible`): uma mensagem pode existir para auditoria mas não entrar no contexto (ex.: kickoff após compaction).
- **`operations` (OperationNotes)**: é o mecanismo do `set_message_meta` da máquina de estados — registra o que já foi feito para que o pipeline reconstruído não repita.

## 4. `Session` e `SessionType`

```rust
pub struct Session {
    pub id, working_dir, name: String,
    pub user_set_name: bool,
    pub session_type: SessionType,
    pub created_at, updated_at: DateTime<Utc>,
    pub extension_data: ExtensionData,
    pub usage, accumulated_usage: Usage,
    pub accumulated_cost: Option<f64>,
    pub schedule_id: Option<String>,
    pub recipe: Option<Recipe>,
    pub user_recipe_values: Option<HashMap<String, String>>,
    pub conversation: Option<Conversation>,
    pub message_count: usize,
    pub last_message_at: Option<DateTime<Utc>>,
    pub provider_name: Option<String>,
    pub model_config: Option<ModelConfig>,
    pub goose_mode: GooseMode,
    pub archived_at: Option<DateTime<Utc>>,
    pub project_id: Option<String>,
    pub parent_session_id: Option<String>,
    pub last_message_snippet: Option<String>,
}
```

`SessionType`: `User`, `Scheduled`, `SubAgent`, `Hidden`, `Terminal`, `Gateway`, `Acp`.

Isso revela que **a mesma abstração de sessão serve** para conversas humanas, jobs agendados, subagentes, sessões ocultas, integração de terminal, gateways e ACP.

## 5. Reconstrução da conversa e ordenação monotônica

A conversa é **reconstruída dos `messages`**:

```sql
SELECT role, content_json, created_timestamp, metadata_json, message_id
FROM messages WHERE session_id = ? ORDER BY created_timestamp, id
```

O ponto crítico é a **ordenação monotônica de timestamps** em `add_message`:

```rust
// Messages are read back ordered by (created_timestamp, id), so one built
// before the messages it is appended after would sort ahead of them —
// operations do that whenever they prepare a reply and fill it in while a
// tool runs. Never move a message ahead of what is already stored.
let latest: Option<i64> = SELECT MAX(created_timestamp) ... ;
let created = message.created.max(latest.unwrap_or(message.created));
```

Ou seja: o goose garante que uma mensagem **nunca seja inserida antes de algo já armazenado** — exatamente o problema que a invariante append-only do prefixo busca evitar (no Pi) e que aqui é resolvido na camada de persistência. Toda escrita de mensagem usa `BEGIN IMMEDIATE`.

## 6. API do `SessionManager`

Métodos principais (parcial):

- `create_session`, `get_session(id, include_messages)`, `list_sessions`, `list_sessions_with_limit`, `list_sessions_by_types`.
- `add_message`, `replace_conversation`, `save_compacted_conversation`.
- `update(id) -> SessionUpdateBuilder` (builder com `usage`, `recipe`, `extension_data`, `goose_mode`, `project_id`, `archived_at`, `provider_name`, `model_config`, `parent_session_id`…).
- `export_session`, `export_session_markdown`, `import_session`.
- `search_chat_history`, `get_session_usage_totals`, `record_usage_metrics`.
- `maybe_update_name` (geração assíncrona de nome via LLM).

O `SessionUpdateBuilder` evita uma explosão de parâmetros e mantém updates atômicos.

## 7. Subsistemas de sessão

| Arquivo | Papel |
|---|---|
| `session_manager.rs` | Núcleo (5.025 linhas) |
| `chat_history_search.rs` | Busca full-text no histórico |
| `export_markdown.rs` | Exportação (48 KB) |
| `import_formats/` | Importação de formatos externos |
| `legacy.rs` | Compatibilidade com formato antigo |
| `last_message_snippet.rs` | Snippet para listagem |
| `session_naming.rs` | Geração de nomes |
| `nostr_share.rs` | Compartilhamento via Nostr |
| `diagnostics.rs` | Diagnóstico da sessão |
| `extension_data.rs` | Estado das extensões por sessão |

## 8. Uso/ledger

- `ProviderUsage` + `Usage` (input/output/cache read/write/cost).
- `MessageUsage::from_provider_usage` converte para métricas por mensagem, com `elapsed_ms` e `time_to_first_token_ms`.
- `usage_ledger` registra cada chamada, marcando `is_compaction`.
- `SessionUsageTotals` agrega por sessão.

## 9. Lições

1. **SQLite + `sqlx`** é uma escolha pragmática para estado local transacional.
2. **`BEGIN IMMEDIATE`** para toda escrita que pode competir entre processos.
3. **`messages` reconstruível e ordenado**; `sessions` é a linha mutável.
4. **Ordenação monotônica de timestamps** evita reordenação sob escrita concorrente.
5. **Visibilidade dupla de mensagem** permite auditoria sem poluir o contexto.
6. **`OperationNotes` na metadata** fecha o loop de determinismo da máquina de estados.
7. **Uma abstração de sessão** cobre user/job/subagente/terminal/gateway/ACP.
