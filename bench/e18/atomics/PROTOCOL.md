# Protocolo — relatório de tempos atómicos (E19-T03)

Como reproduzir e comparar [`REPORT.md`](REPORT.md) / [`raw.json`](raw.json).

## 0. Pré-requisitos

- `katu` compilado com a instrumentação: `cargo build -p katu --features profile`.
- Modelo local (opcional, para a secção 3): `llama-server` em `127.0.0.1:8080/v1`.
- Símbolos de depuração não são necessários aqui (a medição é por spans, não por amostragem; para
  amostragem ver [`docs/profiling.md`](../../../docs/profiling.md)).

## 1. Cobertura (secção 1)

```bash
cargo run -q -p xtask -- diag:coverage
# diag-coverage: 1322/1322 instrumentáveis (100.0%); com 136 const fn: 1322/1458 (90.6%)
```

## 2. Por função (secção 2) — `measure_mvk`

```bash
cargo run -q -p katu --features profile --example measure_mvk /tmp/atomics-functions.json
```

Grava um JSON com `timings[]` por `(event, function)` (o sink agregador imprime o rótulo de
função). Converter para a tabela com `µs = ns/1000`.

## 3. Por operação (secção 3) — execuções reais

```bash
BIN=target/debug/katu
for c in "prime" "sessions" "config list" "memo doctor"; do
    KATU_INSTRUMENT=1 $BIN $c --log-level trace 2>/tmp/atomics-$c.log >/dev/null
done
KATU_INSTRUMENT=1 $BIN run "Leia main.rs..." --log-level trace \
    2>/tmp/atomics-turn.log >/dev/null
```

Agregar as linhas `kind=span.end` (campo `dur_ns=Some(N)`) por `event`:

```bash
grep -h 'kind=span.end' /tmp/atomics-*.log \
  | sed -E 's/.*event=([a-z0-9_.]+) dur_ns=Some\(([0-9]+)\).*/\1 \2/' \
  | sort -k1,1
```

Percentis por *nearest-rank*; `katu.run`/`cli.run`/`kernel.turn`/`provider.request` são totais com
filhos.

A partir de Q-09, os spans de função trazem o rótulo real no campo `function=…` (o id `katu.fn` é
propositalmente genérico, S-02). Para atribuir **por função**, agregar pelo rótulo:

```bash
# o `function=` só existe nos spans de função; os eventos pontuais mantêm a linha antiga
grep -h 'kind=span.end' /tmp/atomics-*.log \
  | sed -nE 's/.*function=([^ ]+).*dur_ns=Some\(([0-9]+)\).*/\1 \2/p' \
  | sort -k1,1
```

## 4. Baseline release (secção 4)

Lido de [`bench/mvk/raw.json`](../../mvk/raw.json) (release, instrumentação **desligada**). Não é
re-gerado aqui; para o refazer: `make measure` + `cargo xtask gate:bench`.

## 5. Gates (secção 5)

```bash
cargo run -q -p xtask -- gate:render
cargo run -q -p xtask -- gate:provider
```

## 6. Comparação entre commits

O `raw.json` tem chaves ordenadas (`sort_keys`) para *diff* estável:

```bash
git diff bench/e18/atomics/raw.json
```

Regra de leitura: comparar a mesma secção e o mesmo `profile`/`instrument`; `dev`+ligado (2-3) e
`release`+desligado (4) **não** são comparáveis entre si.
