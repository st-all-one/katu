# Perfilar o katu (E18-T10)

> **Objetivo.** Obter **tempo atómico por função** para alimentar o E18 (base de dados de
> medição) e evitar otimizações às cegas, sem custo no caminho de produção. Complementa o
> diagnóstico transversal (E19): o `diag` mede **operações**; o profiler de amostragem mede
> **qualquer função**, incluindo as que não abrem span.

## 1. Dois instrumentos, dois papéis

| Instrumento | O que mede | Custo quando desligado | Código? |
|---|---|---|---|
| `diag` (`fn_span!`/`span!`) | operações instrumentadas, agrupadas por `(evento, função)` | **zero** (feature `instrument` fora do binário) | sim |
| Profiler de amostragem (`perf`/`samply`) | **toda** função no stack, sem instrumentar | zero (não está no binário) | não |

O `diag` é a fonte da **métrica de operação** (histogramas determinísticos, `AggregatingSink`); o
profiler é a fonte do **tempo atómico por função** e do *call graph*. Usam-se os dois: o primeiro
para o número publicado (DF5), o segundo para achar o culpado.

## 2. Caminho A — profiler de amostragem (recomendado para "por função")

**Pré-requisitos** (instalar o que faltar):

```bash
# sistema
sudo apt install linux-perf            # ou: sudo pacman -S perf
# ou o profiler do Firefox, sem root:
cargo install samply                   # https://github.com/mstange/samply
```

**Compilar com símbolos** (o perfil `release` do katu faz `strip = "symbols"`; anule-o para
perfilar):

```bash
CARGO_PROFILE_RELEASE_DEBUG=1 CARGO_PROFILE_RELEASE_STRIP=none \
    cargo build --release -p katu
```

**Registar e reportar:**

```bash
# samply (abre uma UI no browser; melhor para stacks agênticos longos)
samply record ./target/release/katu run -- ... 

# perf (CLI; --call-graph dwarf para stacks com LTO)
perf record -F 999 -g --call-graph dwarf -o /tmp/katu.perf \
    ./target/release/katu run -- ...
perf report -i /tmp/katu.perf
```

O wrapper [`scripts/profile.sh`](../../../scripts/profile.sh) deteta o que estiver instalado e faz o
mesmo, com instruções quando não há nenhum:

```bash
./scripts/profile.sh -- ./target/release/katu run
```

## 3. Caminho B — diag agrupado por função (números com artefacto)

Ligue a instrumentação em tempo de compilação (`--features profile`) e em execução
(`KATU_INSTRUMENT=1`), opcionalmente restringindo um subsistema (`KATU_INSTRUMENT_FILTER`):

```bash
# span.end de tudo o que é do kernel, em stderr
KATU_INSTRUMENT=1 KATU_INSTRUMENT_FILTER=kernel. \
    cargo run -q -p katu --features profile -- run --log-level trace 2>/tmp/diag.log

# agregação por (evento) — o rótulo da função vem no início do span
awk '/span\.end/ { for (i=1;i<=NF;i++) { if ($i ~ /^event=/) e=$i; if ($i ~ /^dur_ns=/) d=$i } \
     print e, d }' /tmp/diag.log | sort | uniq -c | sort -rn | head
```

O `AggregatingSink` (`katu_core::diag::aggregate`) produz percentis determinísticos
(min/p50/p95/p99/max) **por `(event, function)`** e é o que os artefactos crus usam
(`bench/mvk/raw.json`, via `examples/measure_mvk.rs`). Para o reproduzir:

```bash
make measure          # regenera bench/mvk/raw.json (com o campo `function`)
cargo run -q -p xtask -- gate:bench
```

## 4. Porque não `criterion`/`dhat` como dependência

`criterion` (e o mesmo para `dhat`) é **não-objetivo** do projeto — a decisão está registada no
`knudge` (E13-T09/R43) e mantida no katu (consistente com E19-T02): as bancadas do katu são
**zero-dep** (`examples/measure_mvk.rs`, `xtask bench-render`, `xtask gate:*`), correm no perfil de
teste e não inflacionam o workspace. Para micro-benchmarks, use `perf stat`/`hyperfine` sobre o
binário já construído, ou uma bancada `examples/` dedicada.

## 5. Reprodutibilidade (fecho do E18-T10)

- ≥ 3 repetições e IC 95 % por métrica publicada; a média não chega (a cauda mente). O resumo vive
  em [`katu_core::stats`](../../../crates/katu-core/src/stats.rs) — zero-dep, determinístico (normal para
  `n ≥ 30`, *bootstrap* com índices derivados de `n` abaixo disso) — e os artefactos ganham o campo
  `ci95`; `gate:render`/`gate:provider` comparam o **limite superior** com o orçamento.
- Todo o número publicado cita um artefacto cru em `bench/` (base `measured`/`provider_reported`),
  validado por `xtask gate:bench` (DF5).
- Negativos visíveis: `unpriced` com valor zero, nunca um número adivinhado.
- Máquina declarada no relatório (`bench/e18/REPORT.md`): os percentis variam por CPU.
