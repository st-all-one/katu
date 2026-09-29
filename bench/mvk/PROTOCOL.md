# Protocolo de medição do MVK (E05-T06)

> Mede o **atrito** que o enforcement do protocolo de memória introduz no caminho real. Nenhum
> número sem base tipada e artefacto (DF5). A linha em que o katu **não** ganha fica visível.

## Casos (não removíveis)

| Caso | Recall antes? | Escreve? | Esperado |
|---|---|---|---|
| **positivo** | sim | sim | 1 commit, sem recusa |
| **negativo** | não | sim | 0 commits, `Denied` (`mem-recall-before-write`) |
| **no-op** | sim | não | 0 commits, sem recusa (baseline de leitura) |

Remover o caso negativo ou o no-op torna o gate cego — a lista é **fixa**.

## Métricas

- **Contadores** (determinísticos, independentes da máquina): commits e tipo de recusa por caso.
- **Tempo** (dependente da máquina, base `measured`): percentis do span `memory.write` sobre 500
  repetições do caso positivo (sink agregador, E19-T02; *nearest-rank*, sem vírgula flutuante).

## Base de evidência (DF5)

Cada número publicado em `bench/published.toml` cita `bench/mvk/raw.json`. A comparação com
`pi + knudge-mcp` está marcada `unpriced` (zero) até ser medida — **não** se publica um ganho
adivinhado.

## Reproduzir

```sh
cargo run -p katu --features profile --example measure_mvk
cargo run -q -p xtask -- gate:bench
```

O artefacto declara `os`/`arch`; percentis variam por máquina, contadores não.
