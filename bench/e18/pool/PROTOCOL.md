# E1 · Curva USL do pool de lote — protocolo e resultado

## Pergunta

O teto `MAX_PARALLEL_CALLS = 8` foi escolhido por um limite de **custo** (PTC: no máximo 8 calls
`Shared` num lote), não por medição de escalabilidade. Quantas threads é que o mecanismo
`spawn`/`join` realmente aproveita, e a partir de onde a curva satura?

## Fórmula

Speedup de Amdahl e eficiência, com `T` threads e `N = 64` tarefas independentes:

```
S(T) = T_wall(1) / T_wall(T)          speedup
E(T) = S(T) / T                      eficiência (1 000 ‰ = escala linear)
saturação = primeiro T com E(T) < 500 ‰
```

Mediana de **7** repetições, sem RNG: a carga é a mesma nos sete ensaios e o número tem de
sobreviver ao ruído do agendador.

## O que se mede (e o que não)

O **mecanismo**: `std::thread::scope` com spawn *eager* — exactamente o de
[`in_parallel`](../../../crates/katu/src/agent/turn/batch.rs) — sobre uma carga sintética calibrada
(200 000 iterações de FNV por tarefa, sem ponto flutuante, sem alocação).

O `dispatch` real é uma avaliação de política de **microssegundos**: medir a escalabilidade dele
seria medir o ruído. O número publicado é sobre o *pool*, não sobre a tool.

## Resultado (`raw.json`)

| threads | wall-clock (µs) | speedup | eficiência |
|---|---|---|---|
| 1 | 18 935 | 0,94 | 942 ‰ |
| 2 | 19 613 | 0,93 | 467 ‰ |
| 3 | 19 301 | 0,92 | 307 ‰ |
| 4 | 19 734 | 0,94 | 236 ‰ |
| 6 | 20 086 | 0,88 | 147 ‰ |
| 8 | 20 908 | 0,85 | 110 ‰ |

**Leitura honesta: a máquina não escala.** `available_parallelism()` diz 16 e a allowed list é
`0-15`, mas **2 threads não dão nem 1,5×** (`speedup_at_2_threads_milli = 978`) e a curva é plana
(até *decrescente*) a partir daí. O artefacto regista isso como `scaling_suspect: true` — o número
mede o **ambiente** (cgroup/sandbox a estrangular a CPU), não a escalabilidade do pool do katu.

## Decisão (escrita)

**O teto não se mexe, e o item não fecha como ganho.** Três razões, todas com o número:

1. O teto de 8 é um limite de **custo** (PTC), não uma afirmação de paralelismo; reduzi-lo a 2 com
   uma máquina estrangada seria afinar a constante à máquina de desenvolvimento.
2. Com `scaling_suspect: true`, qualquer delta medido aqui (20 908 µs a 8 threads contra 19 613 µs a
   2) é **ruído de agendador**, não escalabilidade.
3. O invariante que se pode travar hoje é de outra espécie: **a curva tem de cobrir a produção** —
   `the_production_ceiling_is_inside_the_measured_ladder` falha se `MAX_PARALLEL_CALLS` sair da
   escada medida.

**Condição para rever.** Repetir o A/B numa máquina onde `speedup(2) ≥ 1,5×`. Se aí a curva saturar
antes das 8, o teto passa a ser `min(8, available_parallelism())` — e essa mudança é de código, não
de constante.

## Como correr

```sh
KATU_POOL_OUT=$PWD/bench/e18/pool/raw.json \
  cargo test -q -p katu --release --bins -- --ignored ab_pool_usl
```

## Limites

- Mede o mecanismo, não o `dispatch` (ver acima).
- A carga é sintética: um tool real é misto (I/O + CPU), e o ponto de saturação de I/O é outro.
- `available_parallelism()` reflete a máquina, não a quota efectiva: num contentor com cgroup
  limitado os dois divergem, que é exactamente o que se vê aqui.