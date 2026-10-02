# `katu-providers`

**Épico:** E12 · **Fase:** 8 · **Único crate que fala com modelos** (DF8).

A camada de **providers**: o caminho built-in first-party é nosso; o resto é commodity.

## Responsabilidade

- **Porta no núcleo.** O contrato (`Provider`, `ProviderEvent`, `ProviderOutcome`, `TokenUsage`,
  `ProviderError`) vive em `katu_core::provider`; aqui ficam só os **adaptadores**. O núcleo
  compila com a camada desligada (firewall LLM-free, E12-T01).
- **Built-in (hot path).** [`opencode`](src/opencode.rs) — gateway `zen`/`zen/go`, header
  `x-opencode-session` para afinidade (obrigatório no Go); o dialeto por modelo vem do
  [`catalog`](src/catalog.rs).
- **Declarativo (commodity).** [`declarative`](src/declarative.rs) + `providers/*.json` (estilo
  `goose`): `ProviderSpec` com `engine`/`base_url`/catálogo, exposto por `Declarative<T>`; a chave
  é injetada pela borda.
- **Local (L1).** [`llama`](src/llama.rs) — `llama-server` tratado como endpoint
  `OpenAI`-compatible (`/v1/chat/completions`, `GET /health`); mesmo trait, mesma política.
- **Fake.** [`fake`](src/fake.rs) — turnos guionados determinísticos para os testes de loop
  (E12-T05), sem rede.
- **Transporte.** [`transport`](src/transport.rs) define `Transport` com `MockTransport` (testes)
  e [`http`](src/http.rs) o `UreqTransport` real: bloqueante, `TCP_NODELAY`, *pooling*,
  `Accept-Encoding: identity` (sem compressão de resposta — latência primeiro, E12-T06). O gzip do
  **pedido** é opt-in (`with_request_compression`) e fica **desligado**: os endpoints built-in
  rejeitam-no (opencode `401`, llama `415`; ADR 0013). `warm()` pré-aquece a ligação.
- **Catálogo e despacho.** [`catalog`](src/catalog.rs) mapeia `model → {dialect, context_limit,
  max_tokens_field, prompt_cache, prompt_cache_retention, reasoning, structured_output, tier}`;
  `Catalog::models()` expõe
  os ids em ordem determinística, `Catalog::select_tier()` (E12-T03) o primeiro modelo de uma classe,
  e `Provider::{models, capabilities, model_for_tier}` (E12-T10/T03) alimentam a lista da TUI e a
  validação do controlo. `Provider::dynamic_models()` lê o catálogo **do endpoint**
  ([`models`](src/models.rs): leitura defensiva de `data[]`/`models[]`, determinística), com **queda
  no catálogo** estático; a borda regista a fonte. [`engine`](src/engine.rs)
  constrói o endpoint por dialeto (auth + afinidade: `x-opencode-session` e `affinity_headers`) e
  despacha. A **decodificação estruturada** (B1/W8-1, ADR 0025) é opt-in (`structured_output` no
  catálogo/`ProviderSpec`/`LlamaConfig`, ligada por `structured_output`, **default off**):
  o dialeto
  `chat/completions` acrescenta um `response_format` `json_schema` derivado dos `ToolDef` do pedido
  e **fail-open** (um `400` repete o pedido sem o campo). Artefacto em
  [`bench/e18/grammar`](../../bench/e18/grammar/PROTOCOL.md).
- **Streaming.** [`sse`](src/sse.rs) é um parser SSE incremental **sem alocação por delta** (P-04: a
  linha é uma fatia de `pending` e o payload é emprestado de `self.data`; **−28,5 %** no parser,
  [`bench/e18/transport`](../../bench/e18/transport/PROTOCOL.md)); [`wire`](src/wire.rs) é o driver
  comum (retry só antes do primeiro evento, captura de erro, contagem de chunks).
  [`openai`](src/openai.rs) (`chat/completions`), [`responses`](src/responses.rs),
  [`anthropic`](src/anthropic.rs) (`messages`) e [`google`](src/google.rs)
  (`models/<id>:streamGenerateContent`) normalizam texto, thinking (aceita `reasoning_content`
  /`reasoning`/`reasoning_text` e `thought: true`) e tool calls **completas**; o `chat/completions`
  serializa direto (sem árvore `Value`); o grau de pensamento é codificado por dialeto
  (`reasoning.effort`/`reasoning_effort`/`thinking.budget_tokens`/`thinkingConfig`); `usage` com base
  `provider_reported`; preços [`usage`](src/usage.rs) devolvem `unpriced` sem tabela (DF5) e ligam
  ao `Metric` (`Cost::metric`/`usage_metrics`, `Unit::Micros`/`Tokens`, base de evidência).
- **Cache de prefixo (por modelo).** `prompt_cache`/`prompt_cache_retention` vêm do catálogo; o
  `prompt_cache_key` deriva da sessão. Medido em `deepseek-v4.1-flash` (2.º turno `cached=896/1004`;
  ADR 0013).
- **Erros.** [`error`](src/error.rs) normaliza a mensagem (`error.message`/`message`/`detail`) e
  retira credenciais/*query* de `URL`s antes de logar; [`retry`](src/retry.rs) respeita
  `x-should-retry`, `Retry-After` (segundos/ms/data) e classifica limites de conta como
  **permanentes**.
- **Instrumentação** (DF9): span `provider.request`, eventos `provider.ttft`, `provider.chunk`,
  `provider.retry`, `provider.error`. O TTFT real é medido pelo consumidor (o provider não toca
  relógio).
- **Latência** (E12-T07): `xtask provider-smoke`/`bench-provider` medem TTFT/overhead; artefacto
  cru em `bench/providers/latency.json` e gate de orçamento `xtask gate:provider` (ADR 0014).

## Seam

Endpoint **stateless** de modelo, nunca o agente (DF1/DF8): nada de sessão, loop ou política
delegados. A HttpApi v2 do agente OpenCode **não** é o seam.

## Fronteira

- É **cliente** do plano de dados, não substrato do loop.
- `katu-core`/`katu-policy`/`katu-tools` nunca dependem deste crate.
- WebSocket/HTTP2 são explicitamente `Unsupported`: o `ureq` 3 é HTTP/1.1 e a troca de stack não
  se justifica para um só stream (`responses`/`messages`/`google` estão implementados mas sem
  validação ao vivo; ADR 0012/0013).
