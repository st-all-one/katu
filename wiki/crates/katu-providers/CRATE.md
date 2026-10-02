# `katu-providers` — Camada de Providers

**Épico:** E12 · **Crate:** `crates/katu-providers` · **Único crate que fala com modelos** (DF8)

Adaptadores e transporte para os endpoints de modelo. O contrato vive no núcleo; aqui ficam
**adaptadores**, **dialetos de wire**, **transporte** e **catálogo**.

---

## 1. Visão Geral

O crate `katu-providers` é o único que fala com modelos. Implementa o caminho **built-in first-party**
(`opencode go/zen` + `llama.cpp`) e o caminho **declarativo** (JSON estilo `goose`) para os demais
providers. É **cliente** do plano de dados, nunca substrato do loop.

**Fronteira:**
- A **porta** (`Provider`, `ProviderEvent`, `ProviderOutcome`, `TokenUsage`, `ProviderError`,
  `ProviderSink`, `Flow`, `Thinking`, `Tier`, `ModelCapabilities`, `ToolDef`, `CollectSink`) vive em
  `katu_core::provider`. `katu-core`/`katu-policy`/`katu-tools` **nunca** dependem deste crate
  (firewall LLM-free).
- Depende de `katu-core`, `katu-policy`, `serde`, `serde_json`, `thiserror`, `ureq` 3 (rustls,
  sem compressão de transporte) e `flate2` (gzip opcional do pedido). Dev: `proptest`.
- `#![forbid(unsafe_code)]`; `#![allow(clippy::redundant_pub_crate)]`.

**Seam:** endpoint **stateless** de modelo. Nada de sessão, loop ou política delegados. O `Provider`
não toca o relógio — o TTFT real é medido pelo consumidor (DF9).

---

## 2. Arquitetura

### 2.1 Composição

```
┌─────────────────────────────────────────────────────────────────┐
│                    katu-providers                                │
├─────────────────────────────────────────────────────────────────┤
│  Providers (implementam Provider de katu-core)                  │
│  OpenCode │ Llama │ Declarative<T> │ FakeProvider               │
├─────────────────────────────────────────────────────────────────┤
│  Despacho por dialeto (engine.rs)                               │
│  chat/completions │ responses │ messages │ google               │
├─────────────────────────────────────────────────────────────────┤
│  Driver de streaming (wire.rs) + parser SSE (sse.rs)            │
├─────────────────────────────────────────────────────────────────┤
│  Transporte (transport.rs trait + http.rs UreqTransport/Mock)   │
├─────────────────────────────────────────────────────────────────┤
│  Catálogo (catalog.rs) + declarativo (declarative.rs) + usage   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  katu_core::provider (porta) — Provider, ProviderEvent, ...     │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2 Módulos

| Módulo | Responsabilidade |
|--------|------------------|
| `lib.rs` | Declaração de módulos e reexportações públicas |
| `catalog.rs` | `Dialect`, `MaxTokensField`, `ModelEntry`, `Catalog` |
| `declarative.rs` | `Engine`, `ProviderSpec`, `ModelEntrySpec`, JSON embutido |
| `declarative/provider.rs` | `Declarative<T>` (reusa adaptadores) |
| `engine.rs` | Despacho por dialeto, `WireConfig`, `Call`, `Dispatch` |
| `openai.rs` (+`chunk`/`decode`/`encode`) | Dialeto `chat/completions` |
| `responses.rs` (+...) | Dialeto `OpenAI` Responses API |
| `anthropic.rs` (+...) | Dialeto Anthropic Messages |
| `google.rs` (+...) | Dialeto Google Gemini |
| `opencode.rs` | Built-in gateway `zen`/`zen/go` |
| `llama.rs` | Provider local `llama-server` |
| `fake.rs` | `FakeProvider` determinístico |
| `wire.rs` | Driver SSE com retry + `Wiring` |
| `sse.rs` | Parser SSE incremental (zero-alloc por delta) |
| `transport.rs` | Trait `Transport` + `MockTransport` |
| `http.rs` | `UreqTransport` real (ureq) |
| `retry.rs` | `RetryPolicy`, `is_retryable`, `retry_after` |
| `error.rs` | Normalização/sanitização de erros |
| `models.rs` | Leitura defensiva de catálogos ao vivo |
| `usage.rs` | `Price`, `Cost`, `PriceTable`, métricas |
| `tests/` | Testes sem rede + `grammar`/`retry`/`structured` |

---

## 3. Módulos em Detalhe

### 3.1 Catálogo (`src/catalog.rs`)

Tabela `model → ModelEntry` que roteia o pedido e parametriza o wire. Todo em `BTreeMap` (ordem
determinística).

**`Dialect`** (4 dialetos):

```rust
#[non_exhaustive]
pub enum Dialect {
    ChatCompletions, // "chat/completions"
    Responses,       // "responses"
    Messages,        // "messages"
    Google,          // "google"
}
```

**`MaxTokensField`:**
- `MaxTokens` → `max_tokens` (default; dialetos antigos, `llama.cpp`, maioria dos gateways)
- `MaxCompletionTokens` → `max_completion_tokens` (modelos `OpenAI` recentes)

**`ModelEntry`** — metadados de roteamento e parametrização:

| Campo | Tipo | Uso |
|-------|------|-----|
| `id` | `String` | Id do modelo no endpoint |
| `dialect` | `Dialect` | Dialeto a usar |
| `context_limit` | `Option<u32>` | Janela de contexto |
| `max_tokens_field` | `MaxTokensField` | Campo do teto de tokens |
| `prompt_cache` | `bool` | Suporta cache de prefixo |
| `prompt_cache_retention` | `Option<String>` | Ex.: `"24h"` |
| `reasoning` | `bool` | Emite thinking |
| `reasoning_format` | `Option<String>` | Ex.: `"parsed"` |
| `tier` | `Tier` | Classe custo/capacidade (default `Balanced`) |
| `structured_output` | `bool` | Aceita `response_format`/`json_schema` |

Construtores fluentes: `new`, `with_context_limit`, `with_prompt_cache`,
`with_prompt_cache_retention`, `with_reasoning`, `with_max_tokens_field`, `with_reasoning_format`,
`with_tier`, `with_structured_output`.

**`Catalog`:** `insert`, `lookup`, `len`, `is_empty`, `models()` (ids ordenados), `select_tier(tier)`
(primeiro modelo daquele tier).

### 3.2 Declarativo (`src/declarative.rs`)

Um JSON por provider (estilo `goose`). Os built-in são **embutidos** via `include_str!`; a chave de
API é lida pela **borda** e injetada — nada aqui toca o ambiente.

**`Engine`** (motor de wire):

| Engine | Dialeto |
|--------|---------|
| `OpenAi` (default) | `ChatCompletions` |
| `OpenAiResponses` | `Responses` |
| `Anthropic` | `Messages` |
| `Google` | `Google` |

**`ProviderSpec`** (DTO declarativo): `name`, `engine`, `base_url`, `api_key_env`,
`session_id_header`, `affinity_headers`, `default_max_tokens_field`, `default_reasoning`,
`default_prompt_cache`, `default_prompt_cache_retention`, `structured_output` (**opt-in**),
`models: Vec<ModelEntrySpec>`.

**`ModelEntrySpec`**: `name`, `dialect`, `context_limit`, `prompt_cache`,
`prompt_cache_retention`, `reasoning`, `reasoning_format`, `structured_output`, `tier`. O
`ProviderSpec::catalog()` funde cada modelo com os defaults do provider.

**Definições embutidas:**
- `ProviderSpec::opencode_zen()` → `providers/opencode_zen.json`
- `ProviderSpec::opencode_go()` → `providers/opencode_go.json`
- `ProviderSpec::openai()` → `providers/openai.json`

**`SpecError`:** `Parse(String)`.

### 3.3 Provider declarativo (`src/declarative/provider.rs`)

```rust
pub struct Declarative<T: Transport> {
    transport: T,
    spec: ProviderSpec,
    catalog: Catalog,
    api_key: Option<String>,
    retry: RetryPolicy,
}
```

- `new`/`from_json`/`with_api_key`/`with_retry`/`spec`/`warm`.
- `stream`: procura o modelo no catálogo, escolhe o dialeto e **despacha** reusando os adaptadores
  do built-in.
- `dynamic_models`: `GET /models`, cai no catálogo estático se a resposta for vazia.

### 3.4 Despacho e wire (`src/engine.rs`)

Um só lugar decide, a partir do `Dialect`, qual o adaptador a usar e constrói o `Endpoint` com
autenticação/afinidade.

**Tipos internos (`pub(crate)`):**
- `WireConfig<'a>`: `base_url`, `api_key`, `session`, `session_header`, `affinity_headers`,
  `entry: Option<&ModelEntry>`, `reasoning_format`, `max_tokens`, `temperature`, `structured_output`.
- `Call<'a>`: `endpoint`, `request`, `options`, `retry`.
- `Dispatch<'a>`: `wire`, `dialect`, `request`, `retry`.

**`stream`** — despacha:
```rust
match dispatch.dialect {
    Dialect::ChatCompletions => openai::stream_chat(transport, &call, sink),
    Dialect::Responses       => responses::stream(transport, &call, sink),
    Dialect::Messages        => anthropic::stream(transport, &call, sink),
    Dialect::Google          => google::stream(transport, &call, sink),
}
```

**`endpoint_for`** — constrói `Endpoint { url, headers }`:
- Cabeçalhos base: `content-type`, `accept: text/event-stream`, `accept-encoding: identity`
  (latência primeiro), `user-agent: katu/<versão>`.
- Caminhos: `/chat/completions`, `/responses`, `/messages`,
  `/models/{model}:streamGenerateContent?alt=sse`.
- Autenticação por dialeto (`push_auth`): `Bearer` (ChatCompletions/Responses),
  `x-api-key` + `anthropic-version: 2023-06-01` (Messages), `x-goog-api-key` (Google).
- Afinidade: `session_header` + `affinity_headers` levam o id da sessão.

**`options`** — deriva `EncodeOptions` da entrada de catálogo (campo de tokens, cache de prefixo
com `prompt_cache_key = session`, `reasoning_format`, `structured_output`).

**`models_request`** — `GET <base>/models` (barato) para pré-aquecer TCP/TLS.

### 3.5 Dialeto `chat/completions` (`src/openai/`)

O caminho quente do built-in; cobre `zen`, `zen/go` e `llama-server`.

**`encode.rs`:** `EncodeOptions`, `encode_request` (serialização **direta**, sem árvore `Value`
intermédia), `encode_tool`, `tool_call_schema`, `model_tool_name`, `tool_arguments`,
`thinking_effort`.

- `ChatRequest` serializa `stream: true` + `stream_options.include_usage: true`.
- `max_tokens`/`max_completion_tokens` conforme `max_tokens_field`.
- `prompt_cache_key`/`prompt_cache_retention`, `reasoning_format`, `reasoning_effort`.
- `response_format` (quando `structured_output` **e** há tools): `json_schema` com `tool_call_schema`.
- `model_tool_name`: mapeia o registry (`exec`→`bash`, `search`→`grep|find|ls`, `memoryw*`→`memory`).
- `tool_arguments`: reconstrói argumentos do `ToolUse` tipado (melhor esforço; dívida registada
  E04/E12 — o log guarda o uso **resolvido**, não os argumentos crus).

**`tool_call_schema`:** deriva dos próprios `ToolDef` do pedido um `oneOf` com uma variante por tool,
ligando o nome (`const`) aos seus `parameters`; `strict: true`. Impede, por construção, argumentos
que não validem (a classe de falha de `wire::parse_arguments`). Determinístico (ordem do pedido).

**`chunk.rs`:** estruturas serde do `chat.completion.chunk` (`Chunk`, `Choice`, `Delta`,
`ToolCallDelta`, `UsageJson`, detalhes). `Delta::reasoning()` aceita três variantes de campo
(`reasoning_content`/`reasoning`/`reasoning_text`). `text_of` aceita `content` string ou lista de
partes.

**`decode.rs`:** `ChatDecoder` (estado `Idle`/`Streaming`/`Done`/`Cancelled`).
- Acumula tool calls **por índice** (`BTreeMap<u32, ToolAccum>`) e só as emite **completas**
  (`flush_tools` no `[DONE]`/fecho). Id vazio → `call_<index>`.
- `absorb_usage`: base `provider_reported`; aceita `prompt_tokens_details.cached_tokens`,
  `prompt_cache_hit_tokens`, `cached_tokens` e `completion_tokens_details.reasoning_tokens`.
- `announce` marca o **TTFT** (`provider.ttft`) na primeira emissão.
- `map_stop`: `stop`→`EndTurn`, `tool_calls`/`function_call`→`ToolCalls`, `length`→`Length`,
  `content_filter`→`ContentFilter`, resto→`Other`.

**`stream_chat`:** codifica, envia via `wire::stream`; se um `400` ocorrer com `structured_output`
ligado, **repete sem `response_format`** (fail-open B1/W8-1), emitindo
`structured_output_fallback`.

### 3.6 Dialeto `responses` (`src/responses/`)

`POST /responses`; o stream identifica cada evento pelo campo `type`
(`response.output_text.delta`, `response.output_item.added`,
`response.function_call_arguments.delta`, `response.completed`). Tool calls só **completas**
(`response.output_item.done` ou fecho).

### 3.7 Dialeto `messages` (`src/anthropic/`)

`POST /messages`; SSE com `event:`/`data:`. Blocos por índice
(`content_block_start`/`delta`/`stop`); tool calls completas no `content_block_stop` ou fecho.
Contabilização repartida (`message_start` para entrada, `message_delta` para saída).

**`encode.rs`:** `DEFAULT_MAX_TOKENS = 4096` (`max_tokens` obrigatório). `thinking_budget`:
Low=1024, Medium=8192, High=24 576 (`None` omite). Com raciocínio ligado, a temperatura é **omitida**
(a Anthropic exige =1). Tools com `input_schema`.

### 3.8 Dialeto `google` (`src/google/`)

`models/<id>:streamGenerateContent?alt=sse`. Consome `parts` de texto/raciocínio (`thought: true`) e
`functionCall` (completo, sem acumulação). Contabilização em `usageMetadata`.

### 3.9 Built-in `opencode` (`src/opencode.rs`)

Gateway stateless de modelo, dois produtos com o mesmo seam:
- **Zen** (pay-as-you-go): `https://opencode.ai/zen/v1/*`
- **Go** (subscrição): `https://opencode.ai/zen/go/v1/*` — exige `x-opencode-session`

**`OpenCodeConfig`:** `base_url`, `api_key`, `session`, `dialect`, `max_tokens`, `temperature`,
`session_header`, `affinity_headers`, `catalog`. Construtores: `zen`, `go`, `at`, `with_session`,
`with_base_url`, `with_dialect`, `with_catalog`, `endpoint`.

**`OpenCode<T>`:** `new`, `with_retry`, `with_session`, `config`, `warm`. Implementa `Provider`
(`id()` = `"opencode"`, `models`, `dynamic_models`, `capabilities`, `model_for_tier`, `stream`).
O dialeto por omissão é `chat/completions`; o catálogo pode encaminhar um modelo para
`responses`/`messages`; `google` fica `Unsupported`.

### 3.10 Local `llama` (`src/llama.rs`)

`llama-server` tratado como endpoint `OpenAI`-compatible (`/v1/chat/completions`) + `GET /health`.

**`LlamaConfig`:** `base_url`, `health_url`, `max_tokens`, `temperature`, `reasoning_format`,
`structured_output` (opt-in). `local(port)` → `http://127.0.0.1:<port>/v1` + `/health`.

**`Llama<T>`:** `new`, `with_retry`, `health()`, `warm`. `id()` = `"llama"`. `capabilities` marca
`reasoning: true` (o modelo local decide se emite). `stream` usa sempre `ChatCompletions`.

### 3.11 Fake (`src/fake.rs`)

`Turn { events, stop }`, `Turn::text`, `FakeProvider::new(id, turns)`/`text`. Cada `stream` consome o
turno seguinte (cursor em `Mutex`); guião esgotado → `ProviderError::Unsupported`. Sem I/O.

### 3.12 Driver SSE (`src/wire.rs`)

Uma só implementação da política de retry (só antes do primeiro evento), captura do corpo de erro e
contagem de chunks. Cada dialeto fornece um `Wiring`.

**`Wiring` (trait):** `feed_payload`, `finish_stream`, `has_emitted`, `has_data`, `is_cancelled`,
`is_done`, `final_outcome`.

**`stream`:** loop de tentativas; sucesso → evento de diagnóstico com tokens de entrada **reais**
(Q-01); falha retriable e `attempt < max_retries` → `provider.retry` + `sleep(delay)` e repete; senão
→ `provider.error` e devolve. **Cada tentativa cria um decodificador novo** (`make: FnMut() -> W`).

**`run_attempt`:** envia + alimenta; classifica:
- Falha de transporte → `retriable = !decoder.has_emitted()`.
- `status >= 400` → `http_failure` (corpo normalizado, `is_retryable`, `retry_after`).
- Falha do decodificador → não-retriable.
- Cancelamento do sink (`is_cancelled && !is_done`) → `ProviderError::Cancelled`.

**`parse_arguments`:** string JSON vazia → objeto vazio; inválida → `ProviderError::Decode`.

**`send_and_feed`:** cria `SseParser`, acumula até 2048 bytes de corpo de erro enquanto não há dados,
e emite `provider.chunk` por fragmento.

### 3.13 Parser SSE (`src/sse.rs`)

Parser incremental **sem alocação por delta** (P-04): a linha é uma **fatia** de `pending` e o
payload é **emprestado** de `self.data` (sem `String` intermédia). Suporta `\r\n`, comentários (`:`)
e múltiplas linhas `data:` por evento. Estado incompleto fica em `pending`. `push` devolve
`Flow::Break` se o callback o devolver (cancelamento propagado).

### 3.14 Transporte (`src/transport.rs`)

**Trait `Transport`:** `send(request, sink) -> Result<HttpMeta, TransportError>`; métodos por omissão
`get` (textual, limitado a `GET_BODY_CAP`), `get_text`, `warm` (drena até `WARM_DRAIN_LIMIT` e
devolve a ligação ao pool).

**Tipos:** `Method` (`Get`/`Post`), `Headers`, `HttpRequest` (`post`), `HttpMeta`
(`status`/`headers`/`bytes`), `TransportError` (`Io`/`Protocol`/`Timeout`), `ChunkSink`.

**`MockTransport`:** serve bytes canónicos em memória, em fragmentos de `chunk` bytes (prova o
consumo incremental). `ok(body, chunk)` e `status(status, body)`.

**Constantes:** `WARM_DRAIN_LIMIT = 16 * 1024`, `GET_BODY_CAP = 1 << 20` (1 MiB).

### 3.15 Transporte real (`src/http.rs`)

`UreqTransport` sobre `ureq` 3 (bloqueante), afinado para **latência**:
- `config`: `no_delay(true)` (`TCP_NODELAY`), `http_status_as_error(false)`,
  `max_idle_connections_per_host(4)`, timeouts de `connect` e `recv_response`,
  `timeout_recv_body(None)` (o consumidor cancela).
- `send`: lê o corpo em blocos de `READ_BUFFER = 4096`, reutilizando o buffer (sem alocação por
  chunk), e alimenta o `sink`.
- `with_request_compression(threshold)`: gzip nível rápido **opt-in** (só acima do limiar e se
  encolher); fica **desligado** no built-in (os endpoints rejeitam-no).
- `map_error`: `Timeout`/`Io`/`Protocol`.

### 3.16 Retry (`src/retry.rs`)

**`RetryPolicy`:** `max_retries: u32` (default 2), `base_delay` (400 ms), `max_delay` (30 s).
`disabled()` = 0 tentativas extra. Sem jitter — um cliente, não uma manada.

**`MAX_DELAY = 3600 s`** — teto absoluto de qualquer atraso pedido pelo servidor.

**`PERMANENT`** (marcadores de erro permanente — repetir só gasta tempo): `GoUsageLimitError`,
`FreeUsageLimitError`, `FreeTierError`, `Monthly usage limit reached`, `available balance`,
`insufficient_quota`, `quota exceeded`, `out of budget`, `billing`.

**`is_retryable`:** `x-should-retry` (`true`/`false`) vence; `PERMANENT` → `false`; senão
`408|409|425|429` ou `>= 500`.

**`retry_after`:** `Retry-After-Ms` → `Retry-After` (segundos) → `Retry-After` (data HTTP, descontando
o `Date` da resposta; sem `Date` é ignorada — o provider é puro).

**`body_retry_after`:** `error.metadata.retry_after_seconds` (forma `OpenRouter`).

**`delay`:** o pedido do servidor vence a exponencial; senão `base * 2^attempt`, limitado a
`max_delay`.

**`parse_http_date`** + `days_from_civil` (algoritmo de Hinnant) — sem dependência de datas.

### 3.17 Erros (`src/error.rs`)

**`normalize`:** extrai `error.message`, `error` (string), `message` ou `detail`; senão trunca o
texto; sanitiza. `MAX_CHARS = 512` (trunca por **caracteres**, com elipse).

**`sanitize`:** percorre qualquer `URL` `http(s)` no texto e chama `scrub`.

**`scrub`:** remove `userinfo` (`user:pass@`) e `query`/`fragment`, preservando
`esquema://host/caminho`.

### 3.18 Catálogo ao vivo (`src/models.rs`)

`parse_models(body)` — leitura **defensiva** (nunca falha por forma inesperada) e **determinística**
(ordena e deduplica). Reconhece `data[].id`, `data[].name`, `models[].name` (remove prefixo
`models/` do Google), array de topo e o próprio texto das entradas. Corpo não-JSON → vazio.
Proptest garante totalidade e determinismo.

### 3.19 Uso e custo (`src/usage.rs`)

Base de evidência (DF5): um modelo sem entrada na `PriceTable` devolve
`EvidenceBasis::Unpriced` com `micros = None`.

**`Price`:** `input`/`output`/`cached_input` em **micro-USD por 1M tokens**.
**`Cost`:** `micros: Option<u64>`, `basis`. `metric(model, artifact)` → `Metric` com `Unit::Micros`.
**`PriceTable`:** `set(model, price)`, `cost(model, usage)`.
**`compute`:** aritmética **inteira** (saturante) — `fresh*input + cached*cached_input + output*output`
dividido por 1e6.

**`usage_metrics`:** só emite campos reportados (`input`/`output`/`cached_input`/`reasoning`), cada
um com a base do `usage`, `Unit::Tokens`.

---

## 4. Abordagens de Engenharia

### 4.1 Porta no Núcleo, Adaptadores Aqui

**Princípio:** O contrato vive em `katu_core::provider`; este crate fornece só adaptadores.

**Benefício:** O núcleo compila com a camada desligada (firewall LLM-free, E12-T01); `katu-core`/
`katu-policy`/`katu-tools` nunca dependem deste crate.

### 4.2 Catálogo Declarativo como Dado

**Princípio:** A parametrização do wire é **dados** (`ProviderSpec`/JSON), não código. Os built-in
são JSON embutidos com `include_str!`.

**Benefício:** Adicionar um provider `OpenAI`/Anthropic/Google custa um JSON (~zero), reusando os
adaptadores de wire. Fail-closed nos defaults (`Engine::OpenAi`).

### 4.3 Despacho por Dialeto num Só Lugar

**Princípio:** `engine::stream` é o único sítio que mapeia `Dialect` → adaptador; `endpoint_for`
constrói URL + autenticação + afinidade.

**Benefício:** Built-in e declarativo partilham exatamente o mesmo caminho; sem duplicação de auth.

### 4.4 Retry Só Antes do Primeiro Evento

**Princípio:** `retriable = !decoder.has_emitted()`. Um retry depois de emitir duplicaria texto no
modelo.

**Benefício:** Segurança do stream e do consumidor; cada tentativa cria um decodificador novo.

### 4.5 Streaming Incremental

**Princípio:** O transporte consome **por delta** (`ChunkSink`), o parser SSE é incremental e as
tool calls só são emitidas completas.

**Benefício:** Latência primeiro; nunca bufferiza a resposta inteira no hot path.

### 4.6 Parser SSE Sem Alocação por Delta

**Princípio:** A linha é uma fatia de `pending`; o payload é emprestado de `self.data` (P-04).

**Benefício:** Remove duas alocações `String` por delta (**−28,5 %** no parser,
`bench/e18/transport`).

### 4.7 Latência Primeiro

**Princípio:** `accept-encoding: identity`, `TCP_NODELAY`, pooling (4 ligações idle/host),
`warm()` pré-aquece; gzip do pedido é opt-in e fica desligado.

**Benefício:** O hot path não paga compressão de transporte nem re-encode.

### 4.8 Saída Estruturada Opt-in e Fail-Open

**Princípio:** `response_format`/`json_schema` só é pedido quando o catálogo/`ProviderSpec`/
`LlamaConfig` o ligam; um `400` **repete sem o campo**.

**Benefício:** O `tool_call_schema` derivado dos `ToolDef` impede argumentos inválidos por
construção; mas o *fallback* nunca parte a chamada (fail-open B1/W8-1).

### 4.9 Sanitização de Erros

**Princípio:** `normalize` extrai a mensagem útil e `sanitize`/`scrub` removem credenciais e
*query* de qualquer `URL` antes de logar ou enviar ao modelo.

**Benefício:** Um erro de gateway nunca vaza a chave de API no log nem no contexto.

### 4.10 Erros Permanentes vs. Transitórios

**Princípio:** `PERMANENT` classifica limites de conta/quota como permanentes; `x-should-retry` vence.

**Benefício:** Não gasta tempo a repetir o que nunca vai passar.

### 4.11 Atraso do Servidor Limitado

**Princípio:** `MAX_DELAY = 1 h`; um `1e30` malformado degrada para "sem dica", nunca congela o
agente. O provider **não** toca o relógio.

**Benefício:** Um gateway hostil ou avariado não bloqueia indefinidamente.

### 4.12 Determinismo

**Princípio:** `BTreeMap` no catálogo, ordenação/dedup em `parse_models`, proptest de totalidade,
ordem de tools no `tool_call_schema`.

**Benefício:** Mesma entrada → mesma saída; testável sem rede.

### 4.13 Base de Evidência para Custo

**Princípio:** `PriceTable` vazia → `unpriced` (nunca inventa preço); aritmética inteira em micro-USD;
cada `Metric` carrega a `EvidenceBasis`.

**Benefício:** Números publicáveis só com base declarada (DF5).

### 4.14 Transporte Substituível

**Princípio:** O trait `Transport` permite injetar `MockTransport` (bytes canónicos) sem rede.

**Benefício:** Adaptadores e dialetos testáveis deterministicamente.

---

## 5. Gaps, Limitações e Pendências

### 5.1 Limitações Declaradas

| Limitação | Descrição |
|-----------|-----------|
| WebSocket / HTTP2 | Explicitamente `Unsupported`; `ureq` 3 é HTTP/1.1 (ADR 0012/0013) |
| `responses`/`messages`/`google` | Implementados mas **sem validação ao vivo** (e2e) |
| `google` no built-in opencode | Explicitamente `Unsupported` (só via declarativo) |
| gzip do pedido | Opt-in e **desligado**: os endpoints built-in rejeitam-no (opencode `401`, llama `415`) |
| Sem jitter no retry | Um cliente, não uma manada |

### 5.2 Dívida Registada (no código)

| Item | Descrição |
|------|-----------|
| `tool_arguments` | Reconstrói argumentos a partir do `ToolUse` **resolvido** (melhor esforço); o log não guarda os argumentos crus do modelo (E04/E12) |
| Inferência in-process (L2) | Fora de escopo; fica atrás de feature (comentário em `llama.rs`) |
| Preços | `PriceTable` vazia por omissão → tudo `unpriced` até ser preenchida pela borda |

### 5.3 Flags / Constantes de Operação

| Constante | Valor | Uso |
|-----------|-------|-----|
| `RetryPolicy::default` | 2 retries / 400 ms / 30 s | Política por omissão |
| `MAX_DELAY` | 3600 s | Teto de qualquer atraso |
| `MAX_CHARS` (error) | 512 | Truncagem do resumo de erro |
| `READ_BUFFER` (http) | 4096 | Buffer de leitura do corpo |
| `WARM_DRAIN_LIMIT` | 16 KiB | Drenagem do `warm()` |
| `GET_BODY_CAP` | 1 MiB | Teto de `Transport::get` |
| `DEFAULT_MAX_TOKENS` (Anthropic) | 4096 | `max_tokens` obrigatório |
| `thinking_budget` (Anthropic) | 1024 / 8192 / 24 576 | Low / Medium / High |
| Corpo de erro acumulado | 2048 bytes | Amostra para classificar retry |

### 5.4 Pendências / Trabalho Futuro

| Item | Descrição |
|------|-----------|
| Validação ao vivo | `responses`/`messages`/`google` precisam de e2e (ADR 0012/0013) |
| Preços reais | Preencher `PriceTable` para publicar custos |
| HTTP/2 | Trocar de stack se um dia se justificar |

---

## 6. Testes

### 6.1 Testes Unitários (por módulo)

| Módulo | Testes |
|--------|--------|
| `catalog` | `lookup`/substituição por id, builders, ordem determinística, `select_tier` |
| `declarative` | JSON, defaults, catálogo derivado, embutidos |
| `engine` | `options` do catálogo, `models_request`, afinidade |
| `retry` | `is_retryable`, `retry_after`, `delay`, datas |
| `error` | Extração de mensagem, remoção de credenciais/query |
| `models` | `data`/`models`/array de topo, dedup, proptest total |
| `usage` | `unpriced`, micro-USD inteiro, métricas com base |
| `sse` | Eventos multi-linha, `\r\n`, comentários, cancelamento |
| `http` | gzip roundtrip, compressão acima do limiar |
| `openai` | Encode do pedido, tool schema |
| `responses`/`anthropic`/`google` | Encode + decode de stream canónico |
| `fake` | Turnos guionados |

### 6.2 Testes de Adaptadores (sem rede)

`src/tests.rs` serve SSE canónico via `MockTransport` para opencode (texto, tool call fragmentada,
thinking), llama (`health`, `dynamic_models`, redação de erros) e o dialeto Google.

### 6.3 Testes Especializados

| Ficheiro | Cobertura |
|----------|-----------|
| `src/tests/retry.rs` | Retry de transporte/HTTP, `x-should-retry`, permanente |
| `src/tests/grammar.rs` | Saída estruturada (B1/W8-1) |
| `src/tests/structured.rs` | Derivação e fail-open do `response_format` |

### 6.4 e2e Opcional (`tests/live.rs`)

Corre só com credenciais/URL no ambiente; auto-*skip* sem elas:
- Built-in opencode: `KATU_OPENCODE_KEY` (+ `KATU_OPENCODE_BASE`, `KATU_OPENCODE_MODEL`)
- Local llama: `KATU_LLAMA_URL`

Sem rede, ambos devolvem `Ok(())` — o loop determinístico é coberto pelos testes com
`MockTransport`.

---

## 7. Referências

- **MODULE.md:** [`crates/katu-providers/MODULE.md`](../../../crates/katu-providers/MODULE.md)
- **Definições declarativas:** [`providers/`](../../../crates/katu-providers/providers/)
- **Bench de transporte:** [`bench/e18/transport`](../../../bench/e18/transport/PROTOCOL.md)
- **Bench de gramática:** [`bench/e18/grammar`](../../../bench/e18/grammar/PROTOCOL.md)
- **ADRs:** 0012/0013 (stack HTTP), 0014 (orçamento de latência), 0025 (saída estruturada)
