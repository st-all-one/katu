# P-04 · Transporte do provider (cliente) — protocolo e resultado

## Pergunta

O parser de `text/event-stream` aloca no caminho quente? (Item P-04 do `OPTIMIZATION_PLAN.md` §3:
"overhead do cliente no gate ~2,3–3,6 ms p95; buffer/parse sem alocação por chunk (como `sse`/`retry`)".)

## Fórmula

Um turno é ~1 `data:` por delta. O parser anterior alocava **duas `String` por delta**:

```
linha   → data_value(line) -> Option<String>      (1 alocação por linha `data:`)
payload → self.data.strip_suffix('\n')…to_string() (1 alocação por evento)
```

Ambas são evitáveis: a linha é uma fatia de `pending` e o payload é uma fatia de `self.data`.

```
parser = Σ_deltas (cópia da linha + cópia do payload)   → 0 cópias por delta
```

## Cenário

Corpus SSE canónico (46 912 B, 512 deltas de texto + tool call + usage + `[DONE]`), entregue em
fragmentos de 4 096 B (12 fragmentos), como o transporte real entrega. Dois parsers correm no
**mesmo processo**, com a ordem **alternada** por repetição (quem corre primeiro beneficia do cache
quente) e o **mínimo** de 1 000 passagens como medida (o sinal com menos ruído de alocador).

O teste de CI (`both_parsers_see_the_same_events`, `the_ab_reports_the_same_events_for_both`) exige
que os dois produzam **exatamente os mesmos eventos** — a otimização não pode mudar bytes.

## Como correr

```sh
cargo run -q --release -p xtask -- bench-provider    # escreve bench/providers/latency.json
cargo run -q -p xtask -- gate:provider               # trava a regressão do overhead p95
cargo test -q -p xtask provider_bench                # equivalência dos dois parsers
```

## Resultado (artefacto `latency.json`, campo `sse`)

| | produção (P-04) | réplica anterior | ganho |
|---|---|---|---|
| mínimo (1 000 passagens) | **27 099 ns** | 37 924 ns | **28,5 %** |
| mediana | 28 565 ns | 39 460 ns | 27,6 % |
| eventos entregues | 515 | 515 | idênticos |

Overhead de cliente **ponta a ponta** (encode + decode + eventos, mesmo corpus):
p50 **189 689 ns**, p95 **236 763 ns**, p99 267 773 ns — contra um orçamento de 5 000 000 ns
(`bench/providers/budget.toml`, aplicado por `gate:provider`).

## Custos

- Nenhum token, nenhuma mudança *model-visible*: o callback recebe os **mesmos** bytes (afirmado por
  teste).
- O `pending` continua a ser drenado por fragmento (sem cópia do corpo inteiro): o parser nunca
  bufferiza a resposta.

## Nota de método (baseline)

O artefacto de referência do repositório (`bench/providers/latency.json` da commit `020e6f3`) media
p50 ≈ 1,93 ms para o **mesmo** corpus. Ao re-medir o **código anterior** nesta máquina, hoje, dá
p50 ≈ 0,20 ms: aquele número foi medido noutra sessão, com a máquina carregada (servidores de modelo
e de embeddings a correr), e **não é comparável**. Por isso o A/B do P-04 é feito *dentro* da mesma
execução, com os dois parsers e ordem alternada — a única forma de atribuir a diferença ao código.

## Adoção

**Adotado**: critério (≥ 20 % no alvo) cumprido (28,5 % no parser) e a saída é byte-idêntica.

## Limites

- O corpus é sintético (deltas de ~90 B). Com deltas muito maiores o ganho relativo cai (a cópia
  pesa mais, mas o `serde_json` domina): o que se mede é o parser, não o decode.
- A medição é de CPU num só núcleo, sem rede: o TTFT real depende do servidor (medido por
  `provider-smoke`, dev-only).
- O `serde_json` por evento (~512 por turno) continua a ser a maior parte do overhead restante;
  evitá-lo exigiria um decoder incremental — fora do escopo deste item.
