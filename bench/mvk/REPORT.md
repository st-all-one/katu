# Relatório de atrito do MVK (E05-T06)

> Artefacto cru: [`raw.json`](raw.json) (`os = linux`, `arch = x86_64`, 500 repetições do caso
> positivo). Números publicados: [`../published.toml`](../published.toml). Protocolo:
> [`PROTOCOL.md`](PROTOCOL.md). Base de evidência: **DF5**.

## Enforcement (contadores — determinísticos)

| Caso | Commits | Recusa | `Denied` |
|---|---|---|---|
| positivo (recall → write) | **1** | não | não |
| negativo (write sem recall) | **0** | sim | **sim** (`mem-recall-before-write`) |
| no-op (só leitura) | **0** | não | não |

A regra custa **zero commits indevidos**: o caso negativo não chega a tocar a porta de memória.

## Custo no caminho quente (percentis, ns)

| Span | p50 | p95 | p99 | n |
|---|---|---|---|---|
| `memory.write` (gate + política + executor) | **9 918** | 10 477 | 20 324 | 502 |
| `policy.evaluate` (motor puro) | 2 374 | 2 445 | 2 864 | 502 |
| `kernel.step` | 2 305 | 3 353 | 8 102 | 2 511 |
| `log.append` | 6 635 | 11 315 | 16 693 | 2 511 |

O gate de memória (`memory.write`) fica na ordem dos **µs de um dígito**; a política pura é ~24 %
desse tempo. Nada disto é um gargalo face à latência de um provider.

## Linha vermelha (o que **não** está provado)

- **`mvk.cross_tool.gain_ratio` = `unpriced` (0).** A comparação com `pi + knudge-mcp` no mesmo
  cenário **ainda não foi medida**. Sem ela, o gate E05-T07 não fecha — e o número não se inventa
  (DF5, §62).
- O tempo é de uma máquina declarada no artefacto; não se compara com pisos de outras configs.

## Repro

```sh
cargo run -p katu --features profile --example measure_mvk
cargo run -q -p xtask -- gate:bench
```
