# B-01 — lote concorrente de tool calls `Shared`

**Objetivo.** Absorver do PTC a parte que interessa sem runtime novo: **classificar** as tools por
concorrência (fail-closed) e executar as calls `Shared` de um passo num **pool limitado**, com a
ordem de commit do modelo intacta.

**Alvo (E18 §0.3).** Wall-clock de **N leituras independentes** (N=8, view `summary`, ficheiros de
~1,2 MiB). Adotar se ≥ 20 %.

## Desenho

Três fases, para preservar `Model-visible ⟺ logged` e a §42:

1. **preparar** (sequencial, ordem do modelo): rota + `Session::begin_call` (loga o pedido);
2. **executar** (paralelo, `std::thread::scope`, ≤ `MAX_PARALLEL_CALLS` = 8): `pipeline::dispatch`
   contra um *snapshot* imutável do estado;
3. **commitar** (sequencial, ordem do modelo): `Session::settle_call` + atividade + *retry*.

Uma call **exclusiva** é uma **barreira**: esvazia o lote antes de correr. Fail-closed: só entra no
lote o que [`Concurrency::Shared`](../../../crates/katu-tools/src/schema/mod.rs) declara
(`read`/`grep`/`find`/`ls`); escritas, `bash`, `plan`, `memory` e nomes desconhecidos são exclusivos.

O lote **não** é preparado se não couber inteiro no cost governor (`Session::can_afford_tool_calls`):
um débito recusado a meio deixaria `ToolCall` sem `ToolResult` no log (violaria §42) — nesse caso cai
no caminho sequencial.

## Protocolo

```sh
cargo test -p katu --bin katu ab_end_to_end_batch -- --ignored --nocapture   # A/B end-to-end
cargo test -p katu --bin katu ab_read_effect -- --ignored --nocapture       # efeito da tool
```

Ambos são `#[ignore]` (medem wall-clock, não determinismo). Três cenários, 5 repetições, mediana:
**fixo** (1 passo sem tools), **lote** (8 reads num passo), **sequencial** (8 passos com 1 read).
Ficheiros sintéticos, page-cache aquecida.

## Resultado (medido)

| Cenário | p50 |
|---|---|
| fixo (sem tools) | 0,31 ms |
| **lote** (8 reads, 1 passo) | **112,0 ms** |
| sequencial (8 passos) | 472,3 ms |
| **ganho** | **+76,3 %** |

O efeito isolado da tool confirma a origem do ganho: `ReadTool::execute` (view `summary`) 8×
sequencial 467 ms → paralelo 110 ms (**+76,5 %**). O custo fixo do turno é 0,28 ms, logo o A/B mede
as leituras e não a maquinaria.

**Adotado** (≥ 20 %). Artefacto cru: [`raw.json`](raw.json).

## O erro de medição que quase rejeitou B-01 (lição)

A primeira versão do A/B deu **−5 %** e o plano mandava reverter. A causa era um *refactor* sugerido
pelo `clippy::needless_collect`: encadear

```rust
routed.iter().map(|c| scope.spawn(..)).map(ScopedJoinHandle::join).collect()
```

**entrelaça** `spawn` e `join` por elemento — cada thread só nascia quando a anterior terminava, pelo
que o lote ficava serializado (medido: 547 ms em vez de 107 ms). O `collect` intermédio é
**semanticamente necessário** (o `spawn` tem de ser *eager*) e está marcado com `#[allow]` e a razão.

Moral: um *lint* de estilo não pode reescrever uma medição sem a repetir. O `#[ignore]` do A/B fica no
repositório precisamente para isso.

## Limites (honestidade)

- Efeitos **I/O-bound** ganham; efeitos puramente CPU-bound competem por núcleos (aqui a view
  `summary` é CPU + `memcpy`, e mesmo assim ganha ~4× em 6 núcleos).
- O pool é de **threads de SO** por lote (sem `unsafe`, sem dependências): para lotes de 1 call o
  caminho é sequencial (evita o custo de *spawn*).
- O paralelismo **não** muda autoridade: a política, o log e a §42 são os mesmos; só muda *quando* o
  efeito corre.
