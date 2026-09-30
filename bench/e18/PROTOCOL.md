# Protocolo — baseline de arranque do E18 (otimização profunda)

> Mede o **custo do projeto inteiro** no caminho real, para ancorar as frentes F2–F9 do
> [`plan/19`](../../plan/19-otimizacao-profunda.md) antes de otimizar nada (§0.3: medir → A/B →
> adotar-ou-reverter). Nenhum número sem base tipada e artefacto (DF5/E15-T02).
>
> Este é o **baseline** do método: fixa a linha de partida; as otimizações comparam-se contra ele.
> Não substitui o *harness* estatístico formal (E18-T10) — hoje é medição ad-hoc reproduzível.

## Ambiente (máquina declarada)

- **OS/arch:** Linux x86_64.
- **CPU:** AMD Ryzen 5 5500U (12 threads), `-t 4` nos servidores locais.
- **Build:** `target/release` (`lto=fat`, `codegen-units=1`, `panic=abort`) com `--features profile`
  (instrumentação DF9/E19 ligada por `KATU_INSTRUMENT=1`).
- **Servidores (systemd `--user`, duradouros):**
  - **geral:** `katu-llama.service` → `llama-server` `qwen2.5-coder-1.5b-instruct-q4_k_m` em
    `http://127.0.0.1:8080/v1` (llama.cpp b11269).
  - **embeddings:** `knudge-embed.service` → `granite-embedding-97m-multilingual-r2` (384 dim) em
    `http://127.0.0.1:8889/v1`.
  - **remoto:** `opencode-go` `longcat-2.5-preview-free` em `https://opencode.ai/zen/go/v1`
    (chave efémera em `KATU_OPENCODE_KEY`, nunca commitada).

## O que se mede

| Grupo | Amostras | Como |
|---|---|---|
| Arranque | 100×`katu --version`, 30×`katu prime` | relógio de parede (µs) |
| Turno e2e | 10× `katu run` (llama) e 10× (opencode-go), quente | relógio de parede (ms) |
| Provider | 5× `xtask provider-smoke --warm` por provider | TTFT e total (ms) |
| Embeddings | 30× `POST /v1/embeddings` | `curl -w time_total` (ms) |
| Fase do turno | 5× `katu run --log-level trace`, spans agregados por `event` | duração dos `span!` (ns) |

A **fase do turno** parte o turno em: `provider.request`, `log.append`, `fs.write`,
`session.open`, `kernel.transition` e o restante (**overhead fora do provider** = soma de todos os
spans exceto `katu.run`/`kernel.turn`/`provider.request`). É o que permite atribuir o custo a cada
subsistema e detetar pontos cegos.

## Reproduzir

```sh
# servidores duradouros
systemctl --user start katu-llama.service knudge-embed.service

# medição
cargo build -p katu --release --features profile
cargo run -q -p xtask -- provider-smoke --provider llama --base http://127.0.0.1:8080/v1 --turns 5 --warm
KATU_OPENCODE_KEY=<chave> cargo run -q -p xtask -- provider-smoke --provider opencode-go --turns 5 --warm
KATU_INSTRUMENT=1 target/release/katu run "responda com uma palavra" --provider llama --log-level trace --json
```

O gerador do artefacto cru é descrito em [`REPORT.md`](REPORT.md) §Reprodução. O artefacto
`raw.json` declara `os`/`arch`/`cpu`/`rustc`; os percentis variam por máquina, os spans são
determinísticos.

## Honestidade (DF5)

- Cada número publicado em `bench/published.toml` cita `bench/e18/raw.json`; a base é `measured`.
- O **cold start** do modelo local (primeiro `provider.request` sem prefixo em cache) **não** é
  escondido: na primeira execução após o arranque, o `prompt_tokens` é processado sem cache
  (`cached=0`) e domina o turno; o baseline reporta o **quente** (`cached≈3254/3265`).
- O que **não** está instrumentado é declarado em [`REPORT.md`](REPORT.md) §Pontos cegos — não se
  estima por fora.
