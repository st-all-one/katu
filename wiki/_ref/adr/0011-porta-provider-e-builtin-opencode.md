# ADR 0011 — Porta `Provider` e built-in `opencode go/zen` sobre transporte bloqueante

- **Estado:** aceite
- **Data:** 2026-09-30
- **Decisões fundacionais:** DF8 (provider commodity), DF1 (loop possuído), DF4 (fail-closed),
  DF5 (evidência tipada), DF9 (instrumentação)
- **Épicos:** E12 (camada de providers), E01-T09 (runtime mínimo), E18/F4 (latência)
- **Relaciona:** [plan/13](../plan/13-providers.md), [plan/02](../plan/02-fundacao.md),
  [plan/19](../plan/19-otimizacao-profunda.md)

## Contexto

O `katu-providers` era um stub e o firewall LLM-free (E01) proíbe que `katu-core`/`katu-policy`/
`katu-tools` dependam de crates de provider. Faltava decidir **onde vive o contrato** do modelo e
**como** se fala com o gateway built-in sem introduzir um runtime assíncrono (o plan/02 fixa
*"worker bloqueante por padrão; `tokio` mínimo só se necessário"*).

A observação real do gateway `zen/go/v1` mostrou: dialeto `chat/completions` com SSE, `usage` por
chunk, `reasoning_content` (thinking) e tool calls fragmentadas; o Go exige
`x-opencode-session`; o `llama-server` local fala o mesmo dialeto. O hot path pede **TTFT baixo**,
não compressão.

## Decisão

1. **O contrato é do katu e vive no núcleo.** `katu_core::provider` define `Provider` (streaming
   normalizado, tool calling, contagem de tokens/custo), `ProviderEvent`, `ProviderOutcome`,
   `TokenUsage` e `ProviderError`. Nenhum tipo externo na API; o núcleo compila com a camada
   desligada (E12-T01). Os adaptadores vivem em `katu-providers`.
2. **Transporte bloqueante afinado** (`ureq` + `rustls`, HTTP/1.1): `TCP_NODELAY`, *pooling* de
   ligações, `Accept-Encoding: identity` (sem compressão de transporte), timeouts de ligação e de
   resposta. Sem `tokio`: o custo dominante é a rede/modelo, não o cliente.
3. **Streaming incremental é o caminho único.** Um parser SSE próprio consome linha a linha (nunca
   bufferiza a resposta) e as tool calls só são emitidas **completas** (acumuladas por índice). O
   `usage` vem com base `provider_reported`; sem preço → `unpriced` (DF5).
4. **O nome ao modelo vem do registry.** `bash`/`grep`/`find`/`ls`/`memory` (de `ToolId`) e **não**
   `ToolName::as_str()` (`exec`/`search`/…); a tradução inversa é `ToolName::parse` + registry.
5. **O built-in e o local partilham o mesmo caminho.** `opencode` (zen/go) e `llama-server` (L1)
   usam o dialeto `chat/completions`; os dialetos `responses`/`messages`/`google` ficam
   explicitamente `Unsupported` até E12-T06.
6. **Instrumentação desde já** (DF9): span `provider.request`, eventos `provider.ttft`,
   `provider.chunk` e `provider.error`. O TTFT real é medido pelo consumidor (o provider é puro e
   não toca relógio).
7. **Retry seguro e limitado** (E12-T04): falhas transitórias (`408`/`409`/`429`/`5xx` ou
   `x-should-retry: true`) são repetidas com backoff exponencial, honrando `Retry-After`, **apenas
   antes do primeiro delta** (depois duplicaria texto); os limites de conta/quota do opencode
   (`GoUsageLimitError`/`FreeTierError`, `insufficient_quota`, …) são permanentes e não se repetem.
8. **Base de evidência robusta:** o `usage` lê `cached_tokens` em `prompt_tokens_details`, no topo
   do `usage` ou em `prompt_cache_hit_tokens` (DeepSeek/gateways); sem preço → `unpriced` (DF5).
9. **O provider é endpoint de modelo, não agente** (DF1/DF8): nada de sessão, loop ou política
   delegados a terceiros.

## Alternatives considered

1. **`reqwest`/`tokio` (async).** Rejeitada por agora: contraria a decisão de *worker bloqueante*
   do plan/02 e arrasta um runtime para um caminho onde a latência é da rede. O trait fica pronto
   para um transporte assíncrono quando E12/E01-T09 o exigir.
2. **Escrever HTTP/1.1 + TLS à mão sobre `std`.** Rejeitada: *chunked*, *keep-alive* e TLS
   corretos são superfície grande para reimplementar; o `ureq` dá controlo (`no_delay`, sem
   compressão) com menos risco.
3. **Definir o trait `Provider` em `katu-providers`.** Rejeitada: o kernel não a poderia usar sem
   violar o firewall; o contrato tem de viver no núcleo (como a porta `Memory`).
4. **Usar `ToolName::as_str()` como nome ao modelo.** Rejeitada: o registry expõe `bash`/`grep`/
   `find`/`ls`/`memory` e três nomes mapeiam para `ToolName::Search`; usar o nome de política
   quebraria o catálogo e a resolução.
5. **Bufferizar a resposta inteira e devolver o JSON final.** Rejeitada: viola "latência >
   compressão" — o consumo por delta é o que dá TTFT baixo e cancelamento imediato.
6. **Sem retry (só timeout).** Rejeitada: uma falha transitória (`429`/`5xx`/rede) abortaria o
   turno; o retry limitado, só antes do primeiro delta, melhora a resiliência sem duplicar texto.
7. **Catálogo modelo → dialeto (como o `pi`).** Adiada para E12-T06/T10: exige catálogo curado e
   mais três dialetos; o `chat/completions` cobre os modelos em uso e a via declarativa do
   `goose` (E12-T02) é o caminho para o resto.

## Consequências

- **Positivas:** o caminho built-in corre ponta-a-ponta (medido: `llama-server` Qwen2.5-Coder-1.5B
  Q4_K_M com TTFT ~55 ms; `opencode go/longcat-2.5-preview-free` com TTFT ~2,5 s); falhas
  transitórias são repetidas com backoff (sem duplicar texto) e os limites de conta do opencode
  são recusados de imediato; `MockTransport`+`FakeProvider` provam o loop sem rede; a
  instrumentação já expõe TTFT/chunks; o desenho foi cruzado com o `pi`/`goose` (mesmo seam
  `engine: openai` + `x-opencode-session`); a firewall permanece intacta.
- **Negativas / dívida:** só o dialeto `chat/completions` está implementado; o loop de turnos com
  tool execution (E12-T05/E10) e o WebSocket/HTTP2 (E12-T06) faltam; a reconstrução dos argumentos
  de tool a partir do log é **melhor esforço** (o log guarda o `ToolUse` resolvido, não os
  argumentos crus — dívida E04/E12); sem artefacto de latência publicado (E12-T07).
- **Travas:** `make check` (inclui `check-layers`, `check_file_length.sh`); clippy `-D warnings`;
  `xtask provider-smoke` como instrumento dev-only; `#![forbid(unsafe_code)]`.
