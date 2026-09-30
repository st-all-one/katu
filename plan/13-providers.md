# E12 — Camada de providers (commodity)

> **Fase 8.** Dois níveis deliberados: **integração built-in própria com o `opencode` (go/zen)** —
> o caminho quente, otimizado ao limite da velocidade de comunicação — e os **demais providers via
> GDK/declarativo** (commodity: reaproveitar, não possuir, §2).
>
> **Decisões:** DF8, DF6, DF4, DF5. **Depende de:** E05.
> **Gate do épico:** firewall LLM-free intacta; **caminho built-in medido** (latência/throughput
> antes de compressão); demais providers via GDK; nenhum provider no grafo do núcleo.
>
> **Matemática.** Filas/hedging/prefix-cache do hot path em
> [`19-otimizacao-profunda.md`](19-otimizacao-profunda.md) (E18, F4).

---

## Política de providers

| Nível | Providers | Como | Porquê |
|---|---|---|---|
| **Built-in (hot path)** | `opencode` (`go`, `zen`) — **gateway** | adaptador próprio, transporte afinado | é o provider do produto; a latência é UX |
| **Local (opcional)** | `llama.cpp` (`llama-server` ou in-process) | mesmo trait `Provider`; FFI confinada atrás de feature | offline, custo zero, latência mínima sem rede |
| **GDK/declarativo** | todos os demais | `goose-provider-types` + `goose-providers` (JSON) | commodity: dezenas de dialetos OpenAI/Anthropic/Ollama com custo marginal ~zero |

### Seam do built-in: endpoint de modelo, **nunca** o agente

O built-in liga-se ao **gateway** (`zen/v1/*`, `zen/go/v1/*`), que é um **endpoint stateless de
modelo**. A **HttpApi v2** do agente OpenCode (`/api/*`: sessões, prompts, permissions, PTY, shell)
é **outro alvo** — útil só como *referência* e, no máximo, como **delegação opcional
out-of-process** (mesma regra do E08/E11), jamais como substrato do loop. Ver as notas de
pesquisa do gateway OpenCode (fora do projeto).

### Gateway do built-in (Go/Zen)

Ambos são gateways **compatíveis com SDKs conhecidos**, autenticados por API key (notas de pesquisa
do gateway OpenCode, fora do projeto):

| Produto | Base | Modelos |
|---|---|---|
| **Zen** (pay-as-you-go) | `https://opencode.ai/zen/v1/*` | curados/validados |
| **Go** ($10/mês; Go Plus $40) | `https://opencode.ai/zen/go/v1/*` | open-source |

Quatro dialetos a normalizar no adaptador (mapeados pelo `@ai-sdk/*` de referência):

| dialeto | path | SDK |
|---|---|---|
| OpenAI **Responses API** | `v1/responses` | `@ai-sdk/openai` |
| Anthropic **Messages** | `v1/messages` | `@ai-sdk/anthropic` |
| OpenAI **chat/completions** | `v1/chat/completions` | `@ai-sdk/openai-compatible` |
| Google Gemini | `v1/models/<id>` | `@ai-sdk/google` |

**Session affinity:** header `x-opencode-session`. **Transporte:** HTTP/SSE **ou WebSocket**
(o servidor OpenCode modela `Provider.Transport = http|websocket`,
`Provider.Settings { timeout, chunkTimeout, compaction, transport }`). Não confundir com a
**HttpApi v2** do agente OpenCode (`/api/*`), que é outro alvo (notas de pesquisa, fora do projeto).

### Prior art (`pi` / `goose`) — o que fazem com o opencode

Ambos tratam o opencode como **gateway OpenAI-compatible** com o mesmo truque de afinidade:

- **`pi`** (`_REF/pi/packages/ai`): cada modelo do catálogo declara o seu `api`
  (`openai-completions`, `openai-responses`, `anthropic-messages`, `google-generative-ai`) e todos
  são embrulhados por `withOpenCodeSessionHeader` (põe `x-opencode-session` a partir de
  `sessionId`). O pedido `chat/completions` leva `stream_options.include_usage`, `prompt_cache_key`
  (cache de prefixo) e cabeçalhos de afinidade (`session_id`, `x-client-request-id`,
  `x-session-affinity`). O retry (`utils/provider-retry.ts`) espelha os SDKs: `408/409/429/5xx`,
  respeita `x-should-retry` e `retry-after-ms`/`retry-after`, exponencial com jitter e teto (60 s);
  erros de conta/quota do opencode Go/free-tier (`GoUsageLimitError`, `FreeUsageLimitError`,
  "Monthly usage limit reached", "available balance", `insufficient_quota`) são **permanentes**.
- **`goose`** (`_REF/goose/crates/goose-providers`): opencode entra por **definição declarativa**
  (`definitions/opencode_go.json`/`opencode_zen.json`): `engine: openai`, `base_url` zen/go,
  `api_key_env: OPENCODE_API_KEY`, `session_id_header_override: x-opencode-session` e catálogo de
  modelos com `context_limit`/`preserves_thinking`. O `ProviderRetry`
  (`goose-provider-types/src/retry.rs`) faz 3 tentativas, exponencial com jitter, honrando
  `Retry-After`; o `http_status.rs` normaliza erros e extrai `retry_after_seconds` do corpo.

**Adotado no katu:** o mesmo seam (`engine: openai` + `x-opencode-session`), o retry classificado
com `x-should-retry`/`Retry-After` (segundos, ms ou data `HTTP`) e os limites do opencode como
permanentes, o `usage` robusto de cache, o **catálogo `model → dialeto`** e a via **declarativa**
(`ProviderSpec` + JSON; ADR 0012). O **cache de prefixo é por modelo** (`prompt_cache` +
`prompt_cache_retention`) e foi **medido** em `deepseek-v4.1-flash` (2.º turno: `cached=896/1004`;
ADR 0013); os cabeçalhos de afinidade extra (`x-client-request-id`/`x-session-affinity`) seguem o
`pi`; o `chat/completions` serializa direto, sem árvore `Value`. A **compressão do pedido** foi
**rejeitada** pelos endpoints (opencode `401`, llama `415`) e fica opt-in desligada; HTTP/2 está
bloqueado pelo `ureq` (HTTP/1.1). Os dialetos `responses`/`messages`/`google` estão implementados;
**por adotar:** WebSocket, `dynamic_models` e `Control::SetModel` (E12-T10).

---

## Princípios

1. **Firewall LLM-free** (§21): `katu-core`, `katu-policy` e `katu-tools` **não podem** depender de
   crates de provider. O modelo é cliente do plano de dados.
2. **Tiers com latência/custo conhecidos:** o determinístico resolve primeiro; o LLM é opt-in, e a
   **política** escolhe o nível (§22).
3. **Built-in é tese; o resto é commodity.** O adaptador de `opencode go/zen` é **possuído e
   afinado** (coluna tese de [`00`](00-tese-e-escopo.md) §2); os demais providers entram pelo
   GDK/declarativo, isolados atrás do mesmo trait `Provider` do katu.
4. **Latência > compressão no caminho built-in.** O transporte otimiza **TTFT** e throughput:
   keep-alive/pooling de conexão, `TCP_NODELAY`/HTTP2 quando disponível, streaming incremental
   emitido por delta (sem buffer integral), parse SSE incremental, zero re-encode, cancelamento
   imediato, **sem compressão de transporte** (`Accept-Encoding: identity`) para não pagar
   CPU/latência. Compactação/sumarização de contexto **não** entra no hot path — é off-path/opt-in
   (`goose-context-management` só nos caminhos GDK e fora do turno). A economia de tokens (G6)
   continua a valer para **o que o modelo precisa** (só o delta), mas **não** se paga latência para
   comprimir.
5. **Uma capacidade, um provedor:** exatamente um caminho por provider; sem legado em paralelo.
6. **GDK pinado e atrás de trait.** O GDK está em `0.1.0-alpha.11` (risco R1): contratos do katu
   próprios, versão pinada, `vendor` de peças pequenas se a API oscilar. O caminho built-in **não**
   depende do GDK.
7. **O provider é um endpoint de modelo, não um agente.** O katu **não** delega o loop, a sessão ou
   a política a um servidor de terceiros (DF1/DF2). Um agente externo só entra como *provider* se
   falar o protocolo de modelo; delegar a orquestração a outro agente é o erro do arags/maxima
   (§20, §48–§49) — e a HttpApi v2, ao possuir sessão/loop/permissions, **não** é o seam do
   built-in.

---

## Tarefas

### E12-T01 ☑ Port `Provider` e adaptador built-in
- **Entregáveis:** trait `Provider` (streaming normalizado, tool calling, contagem de custo/tokens);
  adaptador **built-in `opencode go/zen`** como provider por omissão do MVP — o hot path de E12-T06;
  o `llama.cpp` local é o segundo built-in (E12-T08/T09); os demais providers ficam para E12-T02
  (GDK).
- **Estado:** porta em `katu_core::provider` (núcleo sem dependência de provider) e adaptadores em
  `katu-providers`; built-in `opencode` e `llama` pelo dialeto `chat/completions`, com catálogo
  `model → dialeto` (ADR 0012). O fake cobre o loop sem rede.
- **Aceite:** o núcleo compila com a feature do provider desligada; `xtask check-layers` falha se
  um crate de provider entrar em `core`/`policy`/`tools`.

### E12-T02 ◐ Providers declarativos (commodity)
- **Entregáveis:** formato declarativo (`engine`/`base_url`/catálogo) para os **demais** providers,
  isolado atrás do trait próprio; `goose-context-management` só como fonte de compaction **off-path**.
- **Estado:** `ProviderSpec` + `providers/*.json` (opencode zen/go, openai) e `Declarative<T>`,
  reusando os adaptadores de dialeto (ADR 0012). A integração direta com o GDK `goose` foi
  **rejeitada** (risco R1, `tokio`/`reqwest`/tipos externos). O catálogo é exposto por
  `Provider::models()`/`Catalog::models()` (ordem determinística) e `Provider::capabilities()`
  (E12-T10), pelo que a lista de modelos da TUI vem do catálogo. Falta ler o catálogo **do endpoint**
  (`dynamic_models` ao vivo) e cobrir mais dialetos.
- **Aceite:** trocar a fonte de commodity muda só o adaptador; nenhum tipo externo na API do katu;
  o caminho built-in (`opencode go/zen`) **não** passa pelo GDK.

### E12-T03 ◐ Custo/tokens e tiers
- **Entregáveis:** contabilização por chamada; seleção de tier pela política; `Metric` com base de
  evidência (`provider_reported` quando vier do provider, `inferred` quando estimado).
- **Estado:** `TokenUsage` (input/output/cached/reasoning) com base `provider_reported`; `PriceTable`
  em micro-USD com `unpriced` quando não há preço (DF5). Falta ligar ao `Metric`/seleção de tier.
- **Aceite:** custo reportado usa a base correta; `unpriced` para modelo sem preço público; nunca
  inventar preço (DF5).

### E12-T04 ☑ Timeout, retry e cancelamento
- **Entregáveis:** timeout tipado; retry/backoff só em operação idempotente; cancelamento que
  atinge quiescência (§43).
- **Nota (OA17):** se surgir uma cadeia de fallback entre providers, o "primeiro que responde
  vence" usa um despacho `bail` no event bus — **só** com consumidor real (não antecipar).
- **Estado:** timeout de ligação/resposta no transporte e cancelamento imediato
  (`ProviderSink -> Flow::Break` fecha a ligação). Retry classificado (`408`/`409`/`429`/`5xx` ou
  `x-should-retry`), honrando `Retry-After`, **só antes do primeiro delta** (depois duplicaria
  texto); limites de conta/quota do opencode são permanentes.
- **Aceite:** provider que trava é cancelado sem vazar tarefa; retry não duplica efeito.

### E12-T05 ☑ Testes com provider fake e snapshot
- **Entregáveis:** provider fake determinístico para o loop (E04/E05); replay de sessão gravada
  sem chave; política explícita "inference is cheap here — não racionar" nos e2e com chave.
- **Estado:** `FakeProvider` (turnos guionados) e `MockTransport` (SSE canónico) cobrem o caminho
  sem rede; e2e com chave **auto-*skip*** (`KATU_OPENCODE_KEY`/`KATU_LLAMA_URL`). O **loop de
  turnos** liga o provider ao kernel com tool execution pela ordem §42 (`katu run`; roteador
  fail-closed em `crates/katu/src/agent/`; ADR 0015): o histórico é a projeção do log, as tool
  calls são logadas antes de executar e o resultado volta ao modelo. Testes de loop correm sem
  rede (fake + `MemFs`); e2e real verificado contra o `llama-server` e o built-in `opencode-go`.
- **Aceite:** todo teste de loop corre sem rede; o e2e com chave auto-*skip* sem credencial.

### E12-T06 ◐ Adaptador built-in `opencode go/zen` (hot path)
- **Entregáveis:** cliente dos quatro dialetos do gateway (`zen/v1/{responses,messages,
  chat/completions,models/<id>}` e `zen/go/v1/…`) normalizados no trait `Provider`; transporte
  HTTP/SSE **e** WebSocket; keep-alive/pooling, `TCP_NODELAY`/HTTP2, sem `Accept-Encoding`,
  `chunkTimeout` próprio; header `x-opencode-session` para afinidade; streaming incremental por
  delta (texto parcial) e tool calls **completas** (paridade com `MessageStream` do GDK);
  cancelamento imediato; zero re-encode; reconexão explícita.
- **Estado:** `chat/completions`, `responses`, `messages` e `google`
  (`models/<id>:streamGenerateContent?alt=sse`; `thought: true` vira thinking, `functionCall`
  completo) normalizados pelo mesmo `wire`, com retry/erro partilhados; `x-opencode-session`,
  keep-alive/`TCP_NODELAY`, sem compressão; catálogo `model → dialeto` (ADR 0012). WebSocket/HTTP2
  são explícitos `Unsupported`; `responses`/`messages`/`google` ainda sem validação ao vivo.
- **Aceite:** TTFT dentro do orçamento (E12-T07); nenhum buffer integral da resposta; cancelar
  interrompe o stream e não vaza conexão/tarefa; a sessão mantém afinidade via
  `x-opencode-session`.

### E12-T07 ◐ Benchmark de comunicação e orçamento de latência
- **Entregáveis:** `criterion`/`hyperfine` **por dialeto** (Responses/Anthropic/chat/Google) e
  **por transporte** (HTTP/SSE vs WebSocket); métricas de **TTFT**, tokens/s, overhead por turno
  e bytes de rede; `Metric` com base tipada (DF5) — tokens `provider_reported` quando vierem do
  provider, `inferred` quando estimados.
- **Estado:** instrumento dev-only `xtask provider-smoke` mede TTFT/total/usage contra o built-in
  e o `llama-server`; **artefacto commitado** (`bench/providers/latency.json`, offline +
  live) e **gate de orçamento** (`xtask gate:provider` no `make check`; dado em
  `bench/providers/budget.toml`). Protocolo/relatório em `bench/providers/` (ADR 0014).
- **Aceite:** artefacto commitado por número; regressão > X% falha o gate (E15); a linha em que a
  compressão teria poupado bytes mas foi preterida pela latência fica **visível** (negativo/
  `unpriced`), conforme DF5.

### E12-T08 ☑ Provider local `llama.cpp` via `llama-server` (L1)
- **Entregáveis:** `llama-server` tratado como provider OpenAI-compatible (mesmo trait, sem
  exceções); configuração de endpoint/modelo; health/readiness; mesmo caminho de política,
  budget e benchmark. Sem `unsafe`.
- **Estado:** `Llama` sobre o mesmo `chat/completions`, `GET /health` e smoke real com
  Qwen2.5-Coder-1.5B Q4_K_M (TTFT ~55 ms em CPU, 12 threads).
- **Aceite:** funciona offline; o mesmo teste de provider fake corre contra ele; `xtask check-layers`
  mantém o núcleo sem dependência de inferência local.

### E12-T09 ☐ In-process `llama.cpp` (L2, opcional, feature-gated)
- **Entregáveis:** binding in-process (`llama-cpp-rs`/C API) atrás de `provider-llama-inproc`,
  executado no **worker bloqueante** (E01-T09); `unsafe` confinado num módulo com `// SAFETY:`;
  streaming por callback do `llama.cpp` normalizado no mesmo `Provider`; sem rede no hot path.
- **Aceite:** TTFT in-process ≤ ao caminho HTTP/WebSocket (E12-T07) na mesma máquina; desligar a
  feature remove o crate do grafo; Miri/geiger verdes (E13-T04).

### E12-T10 ☑ Controlo de modelo e grau de pensamento
- **Objetivos:** cumprir o core §1.1 #11 — alterar **modelo** e **grau de pensamento**
  (reasoning/thinking) em runtime, sem reiniciar a sessão.
- **Entregáveis:** `Control::{SetModel, SetThinking}` no kernel; mapeamento por dialeto
  (`reasoning.effort` no OpenAI Responses, `thinking.budget_tokens` no Anthropic, `thinking_config`
  no Google, equivalente no `llama.cpp`); só os built-in (`opencode go/zen`, `llama.cpp`) expõem a
  capacidade — os demais via GDK quando suportarem; **o utilizador** aciona; o agente **não** se
  auto-escala (custo/qualidade) e, se pedir, vira `NeedsHuman`.
- **Estado:** o `Catalog` expõe `model → {dialect, context_limit, reasoning}` e `Catalog::models()`
  (ordem determinística); o wire mapeia o grau de pensamento por dialeto — `reasoning.effort`
  (Responses), `reasoning_effort` (`chat/completions`), `thinking.budget_tokens` (Anthropic) e
  `thinkingConfig.thinkingBudget` (Google, E12-T06). O kernel tem `Control::{SetModel, SetThinking}`
  (`kernel::control`), `ControlState` no `State` (sobrevive a *resume*) e `Event::Control` no log
  (audit `kind=control`); a validação é pura e o erro **ensina** (`ControlError::ReasoningUnsupported`
  nomeia o modelo e diz para baixar para `off`), usando `ModelCapabilities` derivadas do catálogo
  pela borda (`Provider::capabilities`). A borda (TUI `m`/`t`) valida e regista via
  `Runtime::set_control`; a lista de modelos vem de `Provider::models()` (T02). O agente **não** tem
  caminho para emitir `Control` — só o utilizador; se o pedisse, seria `NeedsHuman` (E07).
- **Aceite:** trocar de modelo a meio da sessão preserva log/estado (só muda o provider do próximo
  turno); grau de pensamento inválido para o modelo é recusado com erro que ensina; nenhum caminho
  deixa o agente escolher um modelo mais caro sem aprovação.

## Definition of Done

- [ ] E12-T01…T10 concluídas.
- [ ] Firewall LLM-free verificada pelo `xtask check-layers`.
- [ ] **Caminho built-in `opencode go/zen` medido** (TTFT/throughput) e a funcionar ponta-a-ponta;
      demais providers via GDK; local `llama.cpp` opcional (L1 sempre; L2 só se a medição justificar).
- [ ] `cargo xtask check` e job `msrv` verdes.

## Não-objetivos

- 45 providers, OAuth complexo, gateways de plataforma (§0).
- Reimplementar os demais providers à mão: eles vêm do **GDK/declarativo**.
- Compressão/compaction no caminho built-in: fica off-path (latência primeiro).
- **Usar a HttpApi v2 do OpenCode como substrato do loop/sessão/política** (ver princípio 7 e seam acima).
- Retomar o `pi-rs` (portar o commodity inteiro): **não** — é o oposto da regra commodity §2.
- **Auto-escala de modelo pelo agente** (escolher um modelo/grau de pensamento mais caro sozinho):
  viola o core §1.1 #11 — a troca é do utilizador (E12-T10).
