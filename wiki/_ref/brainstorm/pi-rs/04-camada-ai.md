# 04 — Camada de IA (`pi-ai`)

`packages/ai` (25,9k LOC) é a base de tudo. Define tipos, o contrato de streaming normalizado e implementa ~45 providers.

## 1. Superfície pública

Exportado no `index.ts` (core, sem efeitos colaterais): tipos, `models`, `auth`, `oauth`, `utils`, e o provider `faux` (testes). Factories de provider ficam em subpaths (`providers/*`), implementações em `api/*`, e a API global antiga em `compat`.

## 2. Tipos centrais

```rust
// Proposta de espelhamento
pub enum ApiId { OpenAiCompletions, OpenAiResponses, AnthropicMessages, GoogleGenerativeAi,
                 GoogleVertex, BedrockConverse, MistralConversations, AzureOpenAiResponses,
                 OpenAiCodexResponses, PiMessages, Custom(String) }

pub struct Model {
    pub id: String, pub name: String, pub api: ApiId, pub provider: String,
    pub base_url: String, pub reasoning: bool,
    pub input: Vec<Modality>,               // text | image | audio
    pub cost: ModelCost,                    // input/output/cacheRead/cacheWrite
    pub context_window: u64, pub max_tokens: u64,
    pub thinking_level_map: Option<ThinkingLevelMap>,
    pub prompt_cache: Option<ModelPromptCache>,
    pub sampling_params: Option<serde_json::Map<String, Value>>,
    pub compat: Compat,
}

pub struct Context { pub system_prompt: Option<String>, pub messages: Vec<Message>, pub tools: Vec<Tool> }
```

Notas:
- O original embute `systemPrompt`/`tools` **na mensagem system inicial** quando o transcript é normalizado (`TranscriptContext`). Manter essa convenção é importante para compatibilidade de wire.
- `Api` é `KnownApi | (string & {})` — em Rust, `ApiId` com variante `Custom(String)`.

## 3. Contrato de streaming (crítico)

Todo provider implementa:

```rust
#[async_trait]
pub trait ProviderStreams {
    fn stream(&self, model: &Model, ctx: &TranscriptContext, opts: StreamOptions)
        -> AssistantMessageEventStream;
    fn stream_simple(&self, model: &Model, ctx: &TranscriptContext, opts: SimpleStreamOptions)
        -> AssistantMessageEventStream;
    async fn fetch_deferred(...) -> AssistantMessageEventStream { unimplemented }
    async fn cancel_deferred(...) -> Result<()> { unimplemented }
}
```

Eventos normalizados (ordem `start → updates* → done|error`):

| Evento | Campos |
|---|---|
| `start` | `partial` (estrutura inicial da mensagem) |
| `text_start` / `text_delta` / `text_end` | `contentIndex`, `delta`/`content` |
| `thinking_start` / `thinking_delta` / `thinking_end` | idem (thinking redigido pode vir completo no start) |
| `toolcall_start` / `toolcall_delta` / `toolcall_end` | `contentIndex`, `delta`, `toolCall` |
| `done` | `reason`, `message` final |
| `error` | `reason` ("error"\|"aborted"), `error` |

Regras que o port precisa respeitar:
- `partial` **não é snapshot**: providers podem mutar a mesma mensagem/blocos enquanto eventos antigos ainda estão na fila. Consumidores inspecionam em tempo de handling.
- Deltas de blocos diferentes podem intercalar; sempre associar por `contentIndex`.
- Falha após `start` vira evento `error` (não erro do `Result`); falha de setup pode ser `error` sem `start`.
- `toolcall_end` entrega tool call **não validada** contra schema (validação é responsabilidade do agente).

Em Rust, `AssistantMessageEventStream` = `futures::stream::Stream<Item = AssistantMessageEvent> + Send`. O "partial mutável compartilhado" precisa de cuidado com aliasing: usar `Arc<Mutex<AssistantMessage>>` para os eventos ou emitir snapshots coerentes — a segunda opção é mais idiomática e segura, mas muda a semântica se o consumidor guardar `partial`. Decisão registrada em [15](15-riscos-e-decisoes.md) (o Pi explicitamente diz que não se deve reter `partial`).

## 4. Registry de modelos

- `Models` (antes `models.ts` + `models-store.ts`): provider id → `Provider`; `get_model(provider, id)`, `stream`, `stream_simple`.
- Catálogo estático gerado (`models.generated.ts`) com ~45 famílias de modelos por provider.
- **Geração**: `scripts/generate-models.ts` faz fetch de catálogos upstream e emite `providers/*.models.ts`. Em Rust: `xtask generate-models` produz `crates/pi-ai/src/catalog/generated.rs` (ou, melhor, um `.json`/`.rs` versionado). Ver [12](12-dependencias-rust.md) e [13](13-roadmap-migracao.md).
- Regra: **nunca editar o gerado à mão** — editar o gerador.

## 5. Providers implementados (famílias)

OpenAI (completions/responses/codex/azure), Anthropic messages, Google generative-ai + Vertex, Amazon Bedrock Converse, Mistral, xAI, Groq, Cerebras, OpenRouter, Vercel AI Gateway, DeepSeek, NVIDIA, Together, Baseten, HuggingFace, Cloudflare (Workers AI + AI Gateway), GitHub Copilot, OpenCode (Zen/Go), ZAI/ZAI coding-cn, MiniMax (cn), Moonshot (cn), Kimi coding, Meta, Qwen Token Plan (individual/cn), Xiaomi (cn/ams/sgp), Radius, TypeSafe, Ant Ling, Fireworks. Mais: qualquer API OpenAI-compatible (Ollama, vLLM, LM Studio, llama.cpp).

### Estratégia de port
1. Começar por 3 APIs de wire distintas: **OpenAI-compatible (chat completions)**, **Anthropic Messages**, **Google Generative AI**. Diferencie por `ApiId`.
2. OpenAI-compatible cobre a maioria dos providers (Groq, xAI, DeepSeek, OpenRouter, Together, Mistral-compat, Ollama, etc.) — um adapter parametrizado por `base_url` + `compat`/headers.
3. Responses API (OpenAI), Bedrock Converse e Vertex são adapters separados.
4. Fragmentação e parsing: `eventsource-stream` para SSE; WebSocket (Codex) com `tokio-tungstenite`.
5. JSON parcial de tool arguments: `partial-json` (TS) → crate `json-event-parser` ou implementação incremental sobre `serde_json::Deserializer`.

## 6. Auth

- Resolução por `env` (OPENAI_API_KEY, ANTHROPIC_API_KEY, ...), credential store em disco, e OAuth.
- OAuth: GitHub Copilot, OpenAI Codex, Vertex AI, Radius. Fluxos device-code e PKCE, com callbacks interativos (`OAuthLoginCallbacks`).
- Em Rust: `oauth2` crate + `reqwest`; credential store em `~/.pi/...` (JSON com permissões restritas) — mesmo caminho do original para interoperar.
- `getApiKey(provider)` injetável (usado pelo SDK).

## 7. Utils a portar

- `event-stream.ts` — `EventStream` com predicado de terminal (vira `Stream`).
- `retry.ts` / `provider-retry.ts` — backoff, `maxRetryDelayMs`, tratamento de `retry-after`.
- `overflow.ts` — detecção de context overflow para acionar compactação.
- `estimate.ts` — estimativa de tokens.
- `json-parse.ts`, `assistant-message-frame.ts` (frames compactos de mensagens), `uuid.ts` (UUIDv7), `hash.ts`, `sanitize-unicode.ts`.
- `headers.ts`, `pi-user-agent.ts`, `provider-env.ts`, `node-http-proxy.ts`.
- `typebox-helpers.ts` — em Rust, `schemars` + validação `jsonschema`.

## 8. Faux provider

`providers/faux.ts` permite testes sem rede/API. Deve ser portado cedo — é a base dos testes de integração do agente e do harness. Ver [14](14-estrategia-testes.md).

## 9. Custos e tokens

`Usage` inclui `input`, `output`, `cacheRead`, `cacheWrite`, `cacheWrite1h?`, `reasoning?`, `totalTokens`, e `cost` breakdown. Cálculo de custo por modelo (`Model.cost`) e agregação de sessão (usage entries). Manter precisão (f64) e arredondamento idênticos para não divergir relatórios.

## 10. Thinking/reasoning

Níveis unificados `off|minimal|low|medium|high|xhigh|max`, mapeados por `thinkingLevelMap` para valores provider-specific; `thinkingBudgets` para providers por token. `streamSimple` recebe `reasoning`; `stream` recebe opções específicas (`AnthropicOptions`, `GoogleOptions`, etc.).

> O detalhe do **wire** (agrupamento do passo, compat por provider, o caso `opencode-go`) está em [`PROVIDER_WIRE`](../PROVIDER_WIRE.md).
