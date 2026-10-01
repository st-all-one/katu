# P-03 · Caminho `memory.write` / gate — protocolo e resultado

## Pergunta

O gate de escrita de memória (`pre_write` → capacidade → política → commit) tem o custo onde se
pensava? (Item P-03 do `OPTIMIZATION_PLAN.md` §3: "`memory_write` 347 µs + gate 221 µs (dev); em
release 9,9 µs — falta atribuição **release**".)

## Fórmula

O caminho §42 de uma escrita de memória:

```
gate = pre_write + política + record
pre_write = índice + proposta (peneira de postings + dice sobre os candidatos)
record    = proposta + commit (nota + evento)
índice    = Index::from_store   (lê e tokeniza TODAS as notas)
```

Duas coisas podem ser medidas: **onde** o tempo está (atribuição) e **o que** o reuso do índice
poupa. A hipótese do plano: o `record` reconstrói o índice que o `pre_write` do **mesmo gate** acabou
de construir — trabalho repetido, e por isso eliminável sem mudar semântica.

## Cenário

Bases de conhecimento reais escritas no store (100/500/1 000 notas, vocabulário denso e partilhado
como o de um projeto), em disco (não `MemFs`), medidas em **release**. Cada medição usa uma
afirmação **nova** (o `pre_write` decide `Create`; uma afirmação rejeitada pelo dedup nunca chegaria
ao commit e mediria um gate negado — o artefacto publica a decisão medida).

O gate é medido com um adaptador **fresco** por repetição: em produção o índice é invalidado pela
escrita anterior, e um adaptador quente deixaria o caminho antigo a beneficiar de um cache que não
invalidou (medição contaminada). O custo de abrir entra nos **dois** lados.

## Como correr

```sh
KATU_MEMORY_OUT=$PWD/bench/e18/memory/raw.json \
  cargo test -q --release -p katu --bin katu -- --ignored ab_memory_write
cargo test -q -p katu --bin katu memory::    # invariantes (dedup, invalidação do índice)
```

## Resultado (artefacto `raw.json`, mediana de 20)

| Notas | gate (antes) | gate (P-03) | **ganho** | `index_from_store` | `propose` | `open` |
|---|---|---|---|---|---|---|
| 100 | 6 600 µs | 3 386 µs | **48,7 %** | 3 076 µs | 156 µs | 58 µs |
| 500 | 23 937 µs | 14 128 µs | **41,0 %** | 11 268 µs | 597 µs | 57 µs |
| 1 000 | 46 921 µs | 25 784 µs | **45,1 %** | 22 585 µs | 1 741 µs | 60 µs |

A atribuição fecha: `gate` = `open` + `pre_write` (índice + proposta) + política + `record`
(proposta + commit) ≈ 0,06 + 22,6 + 1,7 + 1,7 + 0,09 ms ≈ 26 ms, e o caminho antigo paga **mais um**
`index_from_store` (22,6 ms) — o que se vê nos 46,9 ms.

O que **não** é o gargalo (e por isso não foi mexido): a política (~40 µs), a escrita da nota
(~90 µs) e a peneira de postings — o custo é ler e tokenizar todas as notas para reconstruir o
índice, que é `O(notas)` e cresce com a base.

## Custos

- **Nenhum token de prompt**, nenhuma mudança *model-visible*: o commit é o mesmo (a mesma nota, o
  mesmo evento) — muda só **de onde** vem o índice que o `propose` consulta.
- O índice em cache é o **mesmo** que a decisão viu, pelo que o reuso também torna o caminho
  coerente (antes, a decisão e o commit podiam olhar para índices diferentes).
- A escrita **invalida** o cache: a operação seguinte volta a ler do disco. A janela de validade é
  uma operação do adaptador, e a invalidação está travada por teste
  (`a_write_is_visible_to_the_next_decision`).

## Adoção

**Adotado**: critério do plano (≥ 20 % no alvo) cumprido com 41–49 % e a atribuição publicada.
O ganho cresce com a base (é um `O(notas)` que deixa de correr duas vezes).

## Limites

- A base é sintética: notas com vocabulário partilhado, escritas direto no store (o setup não é a
  medição). Não inclui o custo de embeddings (o dreno é outro caminho, E20-T20).
- O `gate_legacy` é uma **réplica congelada** do commit anterior (`Knudge::write_context()`), não
  uma segunda implementação: é o mesmo `write` do knudge, com o índice reconstruído do store.
- O que resta é `Index::from_store` (ler + tokenizar N notas). Reduzi-lo exige índice incremental ou
  persistido **no knudge** (submódulo `crates/knudge`, outro projeto): fica registado como próximo
  passo, não como promessa.
- Medido com o adaptador in-process; via MCP (E08) o custo é o do servidor.
