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
  `Accept-Encoding: identity` (sem compressão de transporte — latência primeiro, E12-T06).
- **Catálogo e despacho.** [`catalog`](src/catalog.rs) mapeia `model → {dialect, context_limit,
  max_tokens_field, prompt_cache, reasoning}`; [`engine`](src/engine.rs) constrói o endpoint por
  dialeto (auth/afinidade) e despacha.
- **Streaming.** [`sse`](src/sse.rs) é um parser SSE incremental; [`wire`](src/wire.rs) é o driver
  comum (retry só antes do primeiro evento, captura de erro, contagem de chunks).
  [`openai`](src/openai.rs) (`chat/completions`), [`responses`](src/responses.rs) e
  [`anthropic`](src/anthropic.rs) (`messages`) normalizam texto, thinking e tool calls
  **completas**; `usage` com base `provider_reported`; preços [`usage`](src/usage.rs) devolvem
  `unpriced` sem tabela (DF5).
- **Erros.** [`error`](src/error.rs) normaliza a mensagem (`error.message`/`message`/`detail`) e
  retira credenciais/*query* de `URL`s antes de logar; [`retry`](src/retry.rs) respeita
  `x-should-retry`, `Retry-After` (segundos/ms/data) e classifica limites de conta como
  **permanentes**.
- **Instrumentação** (DF9): span `provider.request`, eventos `provider.ttft`, `provider.chunk`,
  `provider.retry`, `provider.error`. O TTFT real é medido pelo consumidor (o provider não toca
  relógio).

## Seam

Endpoint **stateless** de modelo, nunca o agente (DF1/DF8): nada de sessão, loop ou política
delegados. A HttpApi v2 do agente OpenCode **não** é o seam.

## Fronteira

- É **cliente** do plano de dados, não substrato do loop.
- `katu-core`/`katu-policy`/`katu-tools` nunca dependem deste crate.
- Dialetos `google` e WebSocket/HTTP2 são explicitamente `Unsupported` até E12-T06; `responses` e
  `messages` estão implementados mas ainda sem validação ao vivo (ADR 0012).
