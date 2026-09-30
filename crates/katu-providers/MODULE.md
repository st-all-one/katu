# `katu-providers`

**Épico:** E12 · **Fase:** 8 · **Único crate que fala com modelos** (DF8).

A camada de **providers**: o caminho built-in first-party é nosso; o resto é commodity.

## Responsabilidade

- **Porta no núcleo.** O contrato (`Provider`, `ProviderEvent`, `ProviderOutcome`, `TokenUsage`,
  `ProviderError`) vive em `katu_core::provider`; aqui ficam só os **adaptadores**. O núcleo
  compila com a camada desligada (firewall LLM-free, E12-T01).
- **Built-in (hot path).** [`opencode`](src/opencode.rs) — gateway `zen`/`zen/go`, dialeto
  `chat/completions`, header `x-opencode-session` para afinidade (obrigatório no Go).
- **Local (L1).** [`llama`](src/llama.rs) — `llama-server` tratado como endpoint
  `OpenAI`-compatible (`/v1/chat/completions`, `GET /health`); mesmo trait, mesma política.
- **Fake.** [`fake`](src/fake.rs) — turnos guionados determinísticos para os testes de loop
  (E12-T05), sem rede.
- **Transporte.** [`transport`](src/transport.rs) define `Transport` com `MockTransport` (testes)
  e [`http`](src/http.rs) o `UreqTransport` real: bloqueante, `TCP_NODELAY`, *pooling*,
  `Accept-Encoding: identity` (sem compressão de transporte — latência primeiro, E12-T06).
- **Streaming.** [`sse`](src/sse.rs) é um parser SSE incremental; [`openai`](src/openai.rs)
  normaliza texto, `reasoning_content` (thinking) e tool calls **completas**; `usage` com base
  `provider_reported`; preços [`usage`](src/usage.rs) devolvem `unpriced` sem tabela (DF5).
- **Instrumentação** (DF9): span `provider.request`, eventos `provider.ttft`, `provider.chunk`,
  `provider.error`. O TTFT real é medido pelo consumidor (o provider não toca relógio).

## Seam

Endpoint **stateless** de modelo, nunca o agente (DF1/DF8): nada de sessão, loop ou política
delegados. A HttpApi v2 do agente OpenCode **não** é o seam.

## Fronteira

- É **cliente** do plano de dados, não substrato do loop.
- `katu-core`/`katu-policy`/`katu-tools` nunca dependem deste crate.
- Dialetos `responses`/`messages`/`google` são explicitamente `Unsupported` até E12-T06.
