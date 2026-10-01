# Protocolo de medição da latência do provider (E12-T07 / F4-E18-T04)

> Mede o **custo de comunicação** do caminho built-in (DF8) e publica o número com base tipada e
> artefacto cru (DF5). Nenhum número sem artefacto; o negativo da compressão fica visível.

## Duas frentes

| Frente | Como | O que mede | Determinística? |
|---|---|---|---|
| **Offline** | `MockTransport` com um SSE canónico (512 deltas + tool call + usage) | *Overhead de cliente por turno* (encode do pedido + decode do stream + eventos) | **sim** (sem rede) |
| **Live** | `UreqTransport` contra `llama-server` ou o built-in `opencode` | **TTFT**/total/`usage`; razão de acerto de cache (`cached/input`) | não (rede + gateway) |

O número **do gate** é o offline: é reprodutível e trava **regressões grosseiras** (ex.: reintroduzir
um clone/buffer O(história) por turno). Os percentis live variam por máquina e entram no artefacto
como base `measured`, não como limite rígido (E15).

## Orçamento (dado, não constante)

O limite vive em [`budget.toml`](budget.toml) e é versionado. `xtask gate:provider` falha se o
`p95` offline exceder `client_overhead_p95_nanos`. A folga evita um portão de hardware; o alvo é
**adotar-ou-reverter** (§0.3 do [`plan/19`](../../wiki/_ref/plan/19-otimizacao-profunda.md)).

## Percentis

*Nearest-rank*, inteiro, sem vírgula flutuante na decisão (`rank = ceil(p·n/100)`). O artefacto
declara `os`/`arch`; os contadores não variam por máquina, os tempos sim.

## Reproduzir

```sh
# artefacto cru (offline + live contra o llama-server local)
cargo run -q -p xtask -- bench-provider --reps 30 --live --provider llama

# gate do orçamento (offline, entra no `make check`)
cargo run -q -p xtask -- gate:provider
```
