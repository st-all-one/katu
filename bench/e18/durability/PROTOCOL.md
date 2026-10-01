# P-01 · *Group commit* do log (ADR 0024) — protocolo e resultado

## Pergunta

Vale a pena trocar a barreira por evento por uma barreira por turno? (Item P-01 do
`OPTIMIZATION_PLAN.md` §3: "`log.append` faz `sync_data()` por evento → ~44 ms/turno (escala com
tools)".)

## Fórmula

Cada evento anexado ao log é uma escrita seguida de barreira. O custo é o número de barreiras:

```
event = N_eventos × fsync            (default histórico)
turn  = N_turnos  × fsync            (group commit: a barreira fecha no `TurnEnd`)
razão = N_turnos / N_eventos ≈ 1/5   (5 eventos por turno no caminho real)
```

A política é **dado** (`behavior.durability = event|turn`, ADR 0024), com a janela de perda escrita:
`event` perde o evento em voo, `turn` perde **o turno em curso**. O default **não muda**.

## Cenário

20 turnos × 5 eventos (TurnStart + pedido + tool call + resultado + TurnEnd) = 100 eventos, escritos
por `Session::apply` em **disco real** (`target/durability-bench`, não `tmpfs`: em `tmpfs` o `fsync`
é grátis e a medição não diria nada — o *bench* recusa um diretório sob `/tmp`). Mediana de 3
passagens, release.

## Como correr

```sh
KATU_DURABILITY_DIR=$(pwd)/target/durability-bench \
KATU_DURABILITY_OUT=$(pwd)/bench/e18/durability/raw.json \
  cargo test -q --release -p katu --bin katu -- --ignored ab_durability
cargo test -q -p katu --bin katu durability   # os dois modos escrevem o mesmo log
```

## Resultado (artefacto `raw.json`)

| Modo | 100 eventos | por evento | por turno |
|---|---|---|---|
| `event` (default) | **384 773 881 ns** | 3 847 738 ns | 19,2 ms |
| `turn` (P-01) | **84 329 942 ns** | 843 299 ns | 4,2 ms |

**Ganho: −78,1 %** no caminho de anexação (384,8 → 84,3 ms), com o **mesmo** conteúdo de log
(`log_bytes` = 8 462 em ambos; teste `the_two_modes_write_the_same_log` compara byte a byte).

O número fecha com o E18: a baseline media `log.append` **18,3 ms/turno** (≈ 3,7 ms por barreira) e
o modo `turn` mede 4,2 ms/turno — uma barreira por turno em vez de cinco.

## Custos e contrato

- **Contrato:** a janela de perda passa a ser o turno em curso. Está escrito na ADR 0024 e a política
  é **opt-in** (`behavior.durability = turn`); o default (`event`) mantém o comportamento histórico.
- **Recuperação:** um crash pode deixar a **última** linha rasgada (sem `\n`); o leitor descarta-a e
  regista `log.recovered` no diagnóstico. Um ficheiro terminado em `\n` com linha inválida continua a
  ser **corrupção** (fail-closed), e um salto de `seq` continua a ser erro.
- Nada muda no que o modelo vê: o log tem os mesmos eventos, na mesma ordem.

## Adoção

**Adotado** (opt-in): critério do plano (≥ 20 % no overhead do turno) cumprido com −78 % no caminho
de anexação, que é o componente medido em `log.append` no E18.

## Limites

- O ganho depende do número de eventos por turno (mais tool calls ⇒ mais barreiras poupadas) e do
  disco: num `tmpfs` ou num NVMe com `fsync` barato a diferença reduz-se (o *bench* recusa `tmpfs`
  precisamente por isso).
- Não cobre as **escritas atómicas** (`fs.write`: notas, snapshots, transcrição), que continuam com
  uma barreira por escrita — são outro item (`P-01` só fala de log e snapshots).
- O modo `turn` **perde o turno em curso** num crash; é a troca explícita, não um efeito colateral.
