# Relatório — latência do provider (E12-T07)

Resumo humano do artefacto [`latency.json`](latency.json). Base e artefacto por número em
[`bench/published.toml`](../published.toml); método em [`PROTOCOL.md`](PROTOCOL.md).

## Offline (determinístico, o número do gate)

Corpus canónico de **512 deltas** (~47 KB de SSE) servido pelo `MockTransport`, 30 repetições:

| Percentil | Nanos | ms |
|---|---|---|
| p50 | 1 925 598 | ~1,9 |
| p95 | 2 181 985 | ~2,2 |
| p99 | 2 263 630 | ~2,3 |

Orçamento (`budget.toml`): p95 ≤ 5 ms. **Margem ~2,3×** — trava regressões grosseiras, não hardware.

## Live (`llama-server` local, Qwen2.5-Coder-1.5B Q4_K_M, CPU)

30 repetições do mesmo turno (keep-alive):

| | p50 | p95 |
|---|---|---|
| **TTFT** | 48 ms | 51 ms |
| total | 454 ms | 489 ms |

`usage`: `input=21`, `output=11`, `cached=20` → razão de acerto **0,95** (`provider_reported`).
É reutilização de prompt do servidor local, não cache de prefixo de gateway.

## Negativos visíveis

- **Compressão do pedido:** pouparia bytes, mas os endpoints rejeitam `content-encoding: gzip`
  (opencode `401`, llama `415`); preterida pela latência — linha `unpriced` em `published.toml`
  (ADR 0013).
- **HTTP/2:** bloqueado pelo `ureq` (HTTP/1.1); sem multiplexagem, o ganho não paga a stack.
- **Razão de cache do gateway:** medida em `deepseek-v4.1-flash` (2.º turno `cached=896/1004`), mas
  não publicada aqui por depender de chave — ver ADR 0013.
