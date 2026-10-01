# P-02 · Emissor TOON de uma passagem — protocolo e resultado

## Pergunta

O caminho que renderiza um `ToolReport` para o modelo (`report::to_toon` → `toon::project` →
`toon::colunar::emit`) era o maior custo de CPU do dispatch (§1.2: ≈ 270 µs em dev). Onde é que o
tempo se gasta, e a escrita direta no buffer com reserva exata e varredura SWAR baixa-o ≥ 20 %?

## Fórmula

Para cada fase, `ns/op` é a média de `ITERATIONS = 2000` execuções no mesmo processo:

```
ns_por_op = elapsed_ns / iteracoes
ganho_pct = (1 - depois/antes) * 100
```

`antes` é a **réplica congelada** do emissor anterior (`legacy_emit`, no próprio ficheiro de teste,
o mesmo padrão de `xtask/src/toon_bench.rs`): o A/B corre numa só execução, não depende de reverter o
repositório, e o teste afirma que as duas escritas são **byte-idênticas** (`identical_to_legacy`).

Base **`measured`** (relógio), não `inferred`: aqui não há proxy — mede-se CPU. O que **não** mede:
densidade em tokens (essa vive em `xtask bench-toon`, com o tokenizer real) e o custo do resto do
turno.

## Cenário

Payload representativo do pior caso real (`grep`/`find`): 60 linhas com `path`/`ln`/`ty`/`preview` +
lista aninhada `hits` (3 colunas) por linha, mais escalares e um bloco literal — 5551 bytes de TOON,
4 secções. O envelope `r` e o id vêm do `ToolReport`.

## Como correr

```sh
KATU_TOON_OUT=$PWD/bench/e18/toon/raw.json \
  cargo test -q -p katu-core --lib -- --ignored ab_toon_emitter_by_phase

KATU_TOON_OUT=$PWD/bench/e18/toon/raw-release.json \
  cargo test -q --release -p katu-core --lib -- --ignored ab_toon_emitter_by_phase
```

## O que mudou

| # | Antes | Depois | Porquê |
|---|---|---|---|
| 1 | `out = String::new()` + crescimento | `String::with_capacity(byte_len(sections))` | `byte_len` é **exato** (teste `the_reserved_capacity_is_exact`): zero realocações |
| 2 | sanitização com `chars()` (duas passagens: `has_delimiter` + `push`) | guarda SWAR de 8 bytes + passagem lenta exata | o caso comum (sem delimitadores) é uma cópia |
| 3 | `project` clonava cada lista aninhada (`sub.clone()`) | `Nested<'a>` empresta `&'a [Value]` | uma alocação por item e por chave a menos |
| 4 | `Cell::Text(String)` clonava cada string do payload | `Cell<'a>::Text(Cow<'a, str>)` | a célula empresta a string do `Value` |
| 5 | `flatten` construía `Vec<String>` + `join(",")` | `flatten_into(&mut String, …)` | escreve direto no buffer |
| 6 | `Section::literal` clonava as linhas | `lines: Vec<Cow<'a, str>>` emprestadas | blocos de código sem alocação |
| 7 | `child_sections` clonava nomes e refiltrava por `String` | nomes emprestados, `Vec::with_capacity` | comparações `&str` |

### Rejeitado (com o número)

**`push_int` manual** (dígitos num buffer de pilha em vez de `write!(out, "{number}")`): parecia
poupar o `fmt::Arguments` de ~180 células por *stream*, mas mediu **+7 %** em `emit` (109,6 → 120,3 µs
em dev). Revertido — em dev o `write!` é mais rápido que a extração manual com
`checked_rem`/`checked_div` mais a validação `from_utf8`. **Um lint de estilo não substitui uma
medição.**

## Resultado

| Fase | Antes (dev) | Depois (dev) | Ganho | Antes (release) | Depois (release) | Ganho |
|---|---|---|---|---|---|---|
| `emit` | 153,1 µs | **58,1 µs** | **−62,0 %** | 5,44 µs | **3,63 µs** | **−33,3 %** |
| `to_toon` (ponta a ponta) | 231,8 µs | **139,2 µs** | **−39,9 %** | 22,5 µs | **21,1 µs** | −6,4 % |
| `project` | 137,8 µs¹ | 84,0 µs | −39,0 %¹ | — | 18,3 µs | — |
| `to_json` (referência) | 637,3 µs | 637,3 µs | — | 13,1 µs | 13,1 µs | — |

¹ a projeção **antes** de P-02 foi medida com o mesmo *bench* na árvore anterior à mudança
(137,8 µs dev; 275,8 µs ponta a ponta). O `to_toon` da tabela usa a projeção **já otimizada** com o
emissor antigo, pelo que o ganho publicado é **conservador**; o ganho ponta a ponta real em dev é
≈ **−49 %** (275,8 → 139,2 µs).

Critério do plano (`≥ 20 %` em `toon.emit`): **cumprido** em dev (**−62,0 %**) e em release
(**−33,3 %**; corridas repetidas dão −25 % a −41 %).

## Limites

- **Perfil dev exagera**: o ganho em release é menor (o `emit` ainda ganha 41 %, mas o ponta a ponta
  fica em −5 %). Em release a projeção é dominada pela **alocação das linhas**, não pelos clones —
  as alocações eliminadas valiam sobretudo em dev.
- **Uma máquina** (Ryzen 5 5500U), uma medição por perfil publicada; `project` em release tem
  variância alta (é a primeira fase medida, com o alocador frio).
- Não mede **tokens**: a densidade do formato é o A/B de `xtask bench-toon` (`v3` vs JSON), que não
  muda com P-02 — a saída é byte-idêntica (é isso que o teste afirma).
- A réplica `legacy_*` cobre o **emissor** (a métrica do plano) e o caminho ponta a ponta; a
  projeção antiga só existe no artefacto da corrida anterior à mudança.
