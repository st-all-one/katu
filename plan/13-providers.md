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

### E12-T01 ☐ Port `Provider` e adaptador built-in
- **Entregáveis:** trait `Provider` (streaming normalizado, tool calling, contagem de custo/tokens);
  adaptador **built-in `opencode go/zen`** como provider por omissão do MVP — o hot path de E12-T06;
  o `llama.cpp` local é o segundo built-in (E12-T08/T09); os demais providers ficam para E12-T02
  (GDK).
- **Aceite:** o núcleo compila com a feature do provider desligada; `xtask check-layers` falha se
  um crate de provider entrar em `core`/`policy`/`tools`.

### E12-T02 ☐ Integração com GDK/declarativo (demais providers)
- **Entregáveis:** uso de `goose-provider-types`/`goose-providers` (declarativo: um JSON por
  provider OpenAI/Anthropic/Ollama-compatível) para os **demais** providers, isolados atrás do
  trait próprio; `goose-context-management` só como fonte de compaction **off-path**; versão
  pinada (`0.1.0-alpha.11`, R1).
- **Aceite:** trocar a fonte de commodity muda só o adaptador; nenhum tipo externo na API do katu;
  o caminho built-in (`opencode go/zen`) **não** passa pelo GDK.

### E12-T03 ☐ Custo/tokens e tiers
- **Entregáveis:** contabilização por chamada; seleção de tier pela política; `Metric` com base de
  evidência (`provider_reported` quando vier do provider, `inferred` quando estimado).
- **Aceite:** custo reportado usa a base correta; `unpriced` para modelo sem preço público; nunca
  inventar preço (DF5).

### E12-T04 ☐ Timeout, retry e cancelamento
- **Entregáveis:** timeout tipado; retry/backoff só em operação idempotente; cancelamento que
  atinge quiescência (§43).
- **Nota (OA17):** se surgir uma cadeia de fallback entre providers, o "primeiro que responde
  vence" usa um despacho `bail` no event bus — **só** com consumidor real (não antecipar).
- **Aceite:** provider que trava é cancelado sem vazar tarefa; retry não duplica efeito.

### E12-T05 ☐ Testes com provider fake e snapshot
- **Entregáveis:** provider fake determinístico para o loop (E04/E05); replay de sessão gravada
  sem chave; política explícita "inference is cheap here — não racionar" nos e2e com chave.
- **Aceite:** todo teste de loop corre sem rede; o e2e com chave auto-*skip* sem credencial.

### E12-T06 ☐ Adaptador built-in `opencode go/zen` (hot path)
- **Entregáveis:** cliente dos quatro dialetos do gateway (`zen/v1/{responses,messages,
  chat/completions,models/<id>}` e `zen/go/v1/…`) normalizados no trait `Provider`; transporte
  HTTP/SSE **e** WebSocket; keep-alive/pooling, `TCP_NODELAY`/HTTP2, sem `Accept-Encoding`,
  `chunkTimeout` próprio; header `x-opencode-session` para afinidade; streaming incremental por
  delta (texto parcial) e tool calls **completas** (paridade com `MessageStream` do GDK);
  cancelamento imediato; zero re-encode; reconexão explícita.
- **Aceite:** TTFT dentro do orçamento (E12-T07); nenhum buffer integral da resposta; cancelar
  interrompe o stream e não vaza conexão/tarefa; a sessão mantém afinidade via
  `x-opencode-session`.

### E12-T07 ☐ Benchmark de comunicação e orçamento de latência
- **Entregáveis:** `criterion`/`hyperfine` **por dialeto** (Responses/Anthropic/chat/Google) e
  **por transporte** (HTTP/SSE vs WebSocket); métricas de **TTFT**, tokens/s, overhead por turno
  e bytes de rede; `Metric` com base tipada (DF5) — tokens `provider_reported` quando vierem do
  provider, `inferred` quando estimados.
- **Aceite:** artefacto commitado por número; regressão > X% falha o gate (E15); a linha em que a
  compressão teria poupado bytes mas foi preterida pela latência fica **visível** (negativo/
  `unpriced`), conforme DF5.

### E12-T08 ☐ Provider local `llama.cpp` via `llama-server` (L1)
- **Entregáveis:** `llama-server` tratado como provider OpenAI-compatible (mesmo trait, sem
  exceções); configuração de endpoint/modelo; health/readiness; mesmo caminho de política,
  budget e benchmark. Sem `unsafe`.
- **Aceite:** funciona offline; o mesmo teste de provider fake corre contra ele; `xtask check-layers`
  mantém o núcleo sem dependência de inferência local.

### E12-T09 ☐ In-process `llama.cpp` (L2, opcional, feature-gated)
- **Entregáveis:** binding in-process (`llama-cpp-rs`/C API) atrás de `provider-llama-inproc`,
  executado no **worker bloqueante** (E01-T09); `unsafe` confinado num módulo com `// SAFETY:`;
  streaming por callback do `llama.cpp` normalizado no mesmo `Provider`; sem rede no hot path.
- **Aceite:** TTFT in-process ≤ ao caminho HTTP/WebSocket (E12-T07) na mesma máquina; desligar a
  feature remove o crate do grafo; Miri/geiger verdes (E13-T04).

### E12-T10 ☐ Controlo de modelo e grau de pensamento
- **Objetivos:** cumprir o core §1.1 #11 — alterar **modelo** e **grau de pensamento**
  (reasoning/thinking) em runtime, sem reiniciar a sessão.
- **Entregáveis:** `Control::{SetModel, SetThinking}` no kernel; mapeamento por dialeto
  (`reasoning.effort` no OpenAI Responses, `thinking.budget_tokens` no Anthropic, `thinking_config`
  no Google, equivalente no `llama.cpp`); só os built-in (`opencode go/zen`, `llama.cpp`) expõem a
  capacidade — os demais via GDK quando suportarem; **o utilizador** aciona; o agente **não** se
  auto-escala (custo/qualidade) e, se pedir, vira `NeedsHuman`.
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
