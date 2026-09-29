# 05 — Providers

## 1. O contrato: trait `Provider`

Vive em `goose-provider-types/src/base.rs` (crate **GDK**, sem dependências internas). É o ponto de extensão mais importante do sistema.

```rust
pub type MessageStream = Pin<
    Box<dyn Stream<Item = Result<(Option<Message>, Option<ProviderUsage>), ProviderError>> + Send>,
>;

#[async_trait]
pub trait Provider: Send + Sync {
    fn get_name(&self) -> &str;

    fn provider_session_id(&self) -> Option<String> { None }
    async fn resume(&self, _session_id: &str) -> Result<(), ProviderError> { Ok(()) }

    /// Método primário.
    async fn stream(
        &self,
        model_config: &ModelConfig,
        system: &str,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<MessageStream, ProviderError>;

    /// Default: coleta o stream.
    async fn complete(&self, ...) -> Result<(Message, ProviderUsage), ProviderError> { ... }

    async fn get_context_limit(&self, model: &str, override_limit: Option<usize>) -> usize { ... }
    fn retry_config(&self) -> RetryConfig { RetryConfig::default() }
    async fn fetch_supported_models(&self) -> Result<Vec<String>, ProviderError> { Ok(vec![]) }
    async fn fetch_recommended_models(&self, toolshim: bool) -> Result<Vec<String>, ProviderError> { ... }
    async fn map_to_canonical_model(&self, provider_model: &str) -> Result<Option<String>, ...> { ... }
    // ... permission_routing(), manages_own_context(), etc.
}
```

### A regra de ouro do streaming

```rust
/// A message stream yields partial text content but complete tool calls, all within the Message object
/// So a message with text will contain potentially just a word of a longer response, but tool calls
/// messages will only be yielded once concatenated.
pub type MessageStream = ...;
```

**Interpretação:** cada item é uma `Message` **parcial** com texto incremental, mas **tool calls só aparecem completas**. Isso simplifica radicalmente o consumo: não há stream parcial de JSON de tool call.

### `ProviderMetadata` e `ConfigKey`

Providers se autodescrevem para a UI/CLI:

```rust
pub struct ProviderMetadata {
    pub name, display_name, description, default_model: String,
    pub known_models: Vec<ModelInfo>,
    pub model_doc_link: String,
    pub config_keys: Vec<ConfigKey>,
    pub setup_steps: Vec<String>,
    pub setup: Option<ProviderSetupMetadata>,
    pub deprecated: Option<ProviderDeprecation>,
}

pub struct ConfigKey {
    pub name, pub required, pub secret, pub default,
    pub oauth_flow, pub device_code_flow, pub primary,
}
```

Isso permite que `goose configure` e o desktop gerem formulários **a partir do provider**, sem código de UI por provider.

## 2. Duas formas de implementar um provider

### 2.1 Providers nativos (Rust)

Crates `goose-providers` + `goose/src/providers`:

| Arquivo | LOC | Observação |
|---|---:|---|
| `openai.rs` | 1.966 | Base OpenAI Chat Completions |
| `azure_foundry.rs` | 1.419 | Azure AI Foundry |
| `openrouter.rs` | 1.387 | OpenRouter |
| `databricks_v2.rs` | 1.282 | Databricks AI Gateway |
| `openai_live.rs` | 1.265 | Realtime/voz |
| `anthropic.rs` | 882 | Anthropic Messages |
| `ollama.rs` | 982 | Ollama (local) |
| `google.rs` | 215 | Gemini |
| `openai_compatible.rs` | 455 | Fábrica genérica OpenAI-compatível |
| `http_status.rs` | 836 | Mapeamento de erros HTTP |

No crate `goose` há ainda implementações específicas e de **OAuth**: `bedrock.rs` (75 KB), `claude_code.rs` (71 KB), `codex.rs` (51 KB), `chatgpt_codex.rs` (49 KB), `gcpvertexai.rs`, `gemini_oauth.rs`, `githubcopilot.rs`, `xai_oauth.rs`, `azureauth.rs`, `gcpauth.rs`, `kimicode.rs`, `cursor_agent.rs`, etc.

Esses providers de **assinatura** usam OAuth/device-flow para reutilizar a conta do usuário (Claude, ChatGPT, Gemini, Copilot).

### 2.2 Providers declarativos (JSON embutido)

Esta é a inovação de maior alavancagem. Em `goose-providers/src/declarative/definitions/` há **48 arquivos JSON**, um por provider, embutidos via `include_str!`:

```rust
macro_rules! expose_declarative_provider {
    ($module:ident, $definition:expr) => {
        pub mod $module {
            pub const JSON: &str = include_str!(concat!(... "/definitions/", $definition, ".json"));
            pub fn create(tls_config, key_resolver) -> Result<Box<dyn Provider>> {
                from_json(JSON, tls_config, key_resolver)
            }
        }
    };
}
```

Exemplo (`groq.json`):

```json
{
  "name": "groq",
  "engine": "openai",
  "display_name": "Groq",
  "api_key_env": "GROQ_API_KEY",
  "base_url": "https://api.groq.com/openai/v1/chat/completions",
  "models": [
    { "name": "llama-3.3-70b-versatile", "context_limit": 131072, "max_tokens": 8192 }
  ],
  "supports_streaming": true,
  "preserves_thinking": false,
  "setup": { "category": "model", "setup_method": "single_api_key", "docs_url": "..." }
}
```

O enum de engine suporta apenas **três dialetos**:

```rust
pub enum ProviderEngine { OpenAI, Ollama, Anthropic }  // aliases *_compatible
```

Campos avançados de `DeclarativeProviderConfig`:

- `headers`, `session_id_header_override` — propagação de session id;
- `auth: AuthConfig { command, args, refresh_interval, timeout_seconds, cwd }` — **credencial obtida por comando** (ex.: CLI de nuvem), com cache e refresh reativo;
- `dynamic_models` — usa `/v1/models` ou a lista estática;
- `skip_canonical_filtering`;
- `toolshim`, `preserves_thinking`, `emit_clear_thinking`;
- `models`, `env_vars`, `setup`.

**Resultado:** adicionar um provider OpenAI-compatível é, na maioria dos casos, **adicionar um JSON** — sem Rust.

## 3. Formats (conversão) e o registro canônico

### Formats

`goose-provider-types/src/formats/` converte o `Message`/`Conversation` do goose para o wire format de cada vendor:

| Arquivo | LOC |
|---|---:|
| `openai.rs` | 5.788 |
| `anthropic.rs` | 3.305 |
| `openai_responses.rs` | 3.123 |
| `databricks.rs` | 2.075 |
| `google.rs` | 1.888 |
| `snowflake.rs` | 732 |
| `ollama.rs` | 450 |

O `goose-providers/src/formats/` tem variantes específicas (`bedrock.rs` 63 KB, `gcpvertexai.rs`).

### Registro canônico de modelos

`goose-provider-types/src/canonical/`:

- **`canonical_models.json`** (4,6 MB) — catálogo embutido de modelos, pricing, limites, modalidades, `tool_call`, `release_date`.
- **`canonical_mapping_report.json`** (140 KB) — relatório de mapeamento provider-name → canonical-id.
- **`models_dev.rs`** — busca ao vivo do **models.dev** com fallback para o catálogo embutido (feature `online-model-meta`; commit `4dea9b4` "fetch model metadata from models.dev live with bundled fallback").
- **`name_builder.rs`** — normaliza `claude-sonnet-4-5-20250929` → `anthropic/claude-sonnet-4.5`.
- **`registry.rs`** — `CanonicalModelRegistry::bundled()`, `load_cached_catalog`, `refresh_remote_catalog`.

Uso concreto: `fetch_recommended_models` filtra os modelos do provider por **texto + tool calling** e ordena por `release_date`:

```rust
if !canonical_model.modalities.input.contains(&Modality::Text) { return None; }
if !canonical_model.tool_call && !toolshim { return None; }
```

Isso significa que a **lista de modelos que a UI mostra** é derivada de um registro central, não da lista crua do vendor.

## 4. Inventário de providers

`goose/src/providers/inventory/mod.rs` (49 KB) mantém um **serviço de inventário** com refresh assíncrono:

- `ProviderInventoryEntry`, `InventoryModel`, `InventoryIdentity`;
- `RefreshPlan`, `RefreshJobPlan`, `RefreshSkipReason`, `RefreshGuard` (com `Drop`);
- `ProviderInventoryService`.

O `ProviderEntry` (registro) carrega:

```rust
pub struct ProviderEntry {
    metadata: ProviderMetadata,
    constructor: ProviderConstructor,
    inventory_identity: InventoryIdentityResolver,
    inventory_configured: InventoryConfiguredResolver,
    cleanup: Option<ProviderCleanup>,
    provider_type: ProviderType,
    supports_inventory_refresh: bool,
    tls_config: Option<TlsConfig>,
    toolshim: bool,
}
```

`ProviderConstructor` é um `Arc<dyn Fn(extensions, working_dir, tls, default_model) -> BoxFuture<Result<Arc<dyn Provider>>>>` — providers são construídos por **fábricas**, permitindo injeção de extensões/TLS/config.

## 5. `toolshim`: tools em modelos que não suportam tools

`goose/src/providers/toolshim.rs` (81 KB) resolve um problema real: modelos locais/menos capazes não fazem *function calling* nativo. O shim:

1. Pega a saída de texto do modelo;
2. Envia a um **modelo interpretador** (mesmo ou outro);
3. Extrai as intenções de tool call;
4. Anexa as tool calls à `Message` original.

`ToolInterpreter` é um trait; há implementação para Ollama (structured output). É ativado por `toolshim: true` no provider/config.

## 6. Providers ACP (delegação a outros agentes)

O goose pode usar **outros agentes** como provider via ACP:

- `claude_acp.rs` (Claude Code)
- `codex_acp.rs` (Codex)
- `copilot_acp.rs`
- `amp_acp.rs`
- `pi_acp.rs` (**Pi** — o mesmo analisado no dossiê `pi-rs`)

Nesse modo, o agente externo executa tools internamente; o goose repassa as extensões configuradas como servidores MCP.

## 7. Autenticação e segredos

- `provider_secrets.rs` (20 KB) — armazenamento seguro (keyring/cripto).
- `oauth.rs` (30 KB), `oauth_device_flow.rs` (21 KB) — fluxos OAuth e device code (RFC 8628).
- `command_auth.rs` (17 KB) — credencial via comando externo.
- `private_file.rs` (16 KB), `azureauth.rs`, `huggingface_auth.rs`, `gcpauth.rs`, `gemini_oauth.rs`, `xai_oauth.rs`.
- `keyring` com feature `vendored`; no Windows, `winapi`/`wincred`.

## 8. Lições

1. **Um trait estreito (`stream`) + defaults ricos** é suficiente para ~28 implementações nativas.
2. **Streaming com "texto parcial, tool calls completas"** simplifica o consumidor.
3. **Providers declarativos em JSON** reduzem custo marginal a quase zero para dialetos OpenAI/Anthropic/Ollama.
4. **Registro canônico de modelos** centraliza pricing/capacidades/limites e alimenta recomendação e limites de contexto.
5. **Fábricas + inventário** desacoplam descoberta, configuração e instanciação.
6. **`toolshim`** estende o alcance a modelos sem function calling.
7. **Delegação via ACP** reaproveita agentes externos como "providers".
