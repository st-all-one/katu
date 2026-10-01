# Tempos atómicos do katu (E19-T03)

> **Gerado** por medição do caminho real; artefacto cru: [`raw.json`](raw.json). Para comparação
> futura, comparar tabelas/`raw.json` entre commits (`git diff bench/e18/atomics/`). Método em
> [`PROTOCOL.md`](PROTOCOL.md).

## 1. Cobertura de instrumentação (por função)

| | cobertas | total | % |
|---|---:|---:|---:|
| funções **instrumentáveis** (não-`const`, fora de testes, com corpo) | 1322 | 1322 | 100.0 % |
| com `const fn` no denominador (não instrumentáveis) | 1322 | 1458 | 90.6 % |

`katu-policy` excluída por desenho (firewall: instrumentada **pelo chamador**). `const fn` = 136.

## 2. Tempo atómico POR FUNÇÃO — caminho MVK (µs)

Fonte: `examples/measure_mvk`, sink agregador, **instrumentação ligada**, build `dev`. As durações
**incluem** o custo do `diag` aninhado (só existe quando ligado). O IC 95 % é da média (W7).

| função | span | n | p50 | p95 | máx | IC 95 % |
|---|---|---:|---:|---:|---:|---|
| `kernel::session::memory_write` | `memory.write` | 502 | 632.77 | 708.90 | 945.39 | [632.77, 643.34] |
| `kernel::memory_gate::enforce_memory_write` | `memory.write` | 502 | 274.13 | 307.17 | 482.33 | [274.13, 278.72] |
| `kernel::pipeline::dispatch_with` | `katu.fn` | 502 | 256.88 | 287.68 | 460.89 | [256.88, 261.27] |
| `kernel::pipeline::delta` | `katu.fn` | 502 | 221.19 | 249.97 | 309.40 | [220.23, 223.50] |
| `report::to_delta` | `toon.emit` | 501 | 213.93 | 241.87 | 300.88 | [213.93, 216.65] |
| `kernel::pipeline::with_estimated_cost` | `katu.fn` | 501 | 208.62 | 235.86 | 391.68 | [208.62, 212.31] |
| `report::to_toon` | `toon.emit` | 1002 | 207.43 | 235.51 | 385.53 | [207.43, 210.55] |
| `toon::colunar::emit` | `toon.emit` | 1002 | 141.85 | 160.64 | 306.89 | [141.85, 143.82] |
| `kernel::session::open` | `katu.fn` | 503 | 68.31 | 81.02 | 152.81 | [68.31, 70.11] |
| `kernel::session::open_with_cap` | `katu.fn` | 503 | 64.88 | 76.13 | 148.49 | [64.88, 66.56] |
| `kernel::session::open_with_cost` | `store.load` | 503 | 61.04 | 72.29 | 132.98 | [61.04, 62.78] |
| `kernel::session::apply` | `katu.fn` | 2511 | 56.22 | 79.06 | 232.85 | [56.14, 57.33] |
| `kernel::session::apply_at` | `kernel.transition` | 2511 | 52.80 | 75.50 | 229.43 | [52.76, 53.94] |
| `toon::colunar::byte_len` | `katu.fn` | 1002 | 51.05 | 61.53 | 123.48 | [51.05, 52.50] |
| `toon::colunar::emit_rows` | `toon.emit` | 2004 | 50.01 | 67.68 | 206.87 | [40.73, 50.01] |
| `write::execute` | `tool.write` | 501 | 32.97 | 41.07 | 57.13 | [32.97, 34.25] |
| `report::envelope` | `katu.fn` | 1002 | 28.15 | 35.83 | 54.41 | [27.43, 28.16] |
| `toon::project::project` | `toon.project` | 1002 | 25.98 | 32.41 | 69.56 | [25.98, 26.86] |
| `toon::project::project_map` | `toon.project` | 1002 | 22.49 | 27.45 | 41.84 | [22.49, 23.28] |
| `kernel::log::append` | `katu.fn` | 2511 | 19.14 | 26.26 | 43.93 | [18.95, 19.27] |
| `kernel::log::append` | `log.append` | 2511 | 15.85 | 22.28 | 39.60 | [15.54, 15.85] |
| `write::record_report` | `katu.fn` | 501 | 15.71 | 19.07 | 35.69 | [15.71, 16.49] |
| `kernel::cost::check` | `cost.check` | 1507 | 13.90 | 16.41 | 165.04 | [13.59, 14.05] |
| `kernel::session::identity::load_meta` | `katu.fn` | 503 | 11.87 | 13.76 | 28.70 | [11.87, 12.24] |
| `kernel::session::snapshot::load` | `katu.fn` | 503 | 11.45 | 13.34 | 28.01 | [11.45, 11.96] |
| `kernel::cost::check_layers` | `katu.fn` | 1507 | 10.83 | 12.78 | 161.75 | [10.29, 10.83] |
| `kernel::log::read_records_with_len` | `log.replay` | 503 | 8.52 | 10.34 | 21.16 | [8.52, 8.82] |
| `memory::fake::record` | `katu.fn` | 501 | 8.17 | 9.99 | 21.09 | [8.17, 8.57] |
| `kernel::step::step` | `kernel.step` | 2511 | 7.96 | 11.11 | 32.34 | [7.81, 8.01] |
| `toon::colunar::emit_cell` | `katu.fn` | 11022 | 7.54 | 9.15 | 128.79 | [5.26, 7.54] |
| `kernel::step::tool_call` | `katu.fn` | 1004 | 5.80 | 7.68 | 27.17 | [5.80, 6.26] |
| `ports::fs::mem::append` | `katu.fn` | 2511 | 5.24 | 6.57 | 21.79 | [5.24, 5.45] |
| `kernel::cost::commit` | `katu.fn` | 1507 | 4.89 | 6.08 | 125.37 | [4.89, 5.35] |
| `ports::fs::mem::exists` | `katu.fn` | 503 | 4.89 | 6.22 | 17.81 | [4.89, 5.26] |
| `kernel::cost::new` | `katu.fn` | 503 | 4.82 | 5.73 | 16.20 | [4.82, 5.00] |
| `kernel::log::resume` | `katu.fn` | 503 | 4.82 | 5.73 | 17.18 | [4.82, 5.10] |
| `toon::colunar::push_sanitized` | `katu.fn` | 6012 | 4.82 | 5.80 | 123.06 | [4.82, 5.08] |
| `memory::fake::pre_write` | `katu.fn` | 502 | 4.75 | 5.59 | 18.02 | [4.75, 4.91] |
| `ports::fs::mem::read` | `katu.fn` | 1006 | 4.75 | 5.66 | 20.46 | [4.75, 4.95] |
| `kernel::step::tool_result` | `katu.fn` | 1004 | 3.70 | 6.63 | 19.14 | [3.70, 4.07] |
| `policy::evaluate` | `policy.evaluate` | 502 | 2.44 | 3.35 | 12.36 | [2.44, 2.79] |
| `kernel::pipeline::facts_from` | `katu.fn` | 502 | 1.96 | 2.44 | 14.18 | [1.96, 2.09] |
| `kernel::log::session_path` | `katu.fn` | 1006 | 1.89 | 2.79 | 15.37 | [1.89, 2.05] |
| `memory::fake::default` | `katu.fn` | 503 | 1.89 | 2.38 | 13.48 | [1.89, 2.00] |
| `kernel::state::initial` | `katu.fn` | 503 | 1.82 | 1.96 | 2.86 | [1.64, 1.82] |
| `kernel::step::turn_start` | `kernel.transition` | 503 | 1.82 | 1.96 | 19.98 | [1.65, 1.83] |
| `kernel::event::new` | `katu.fn` | 1506 | 1.47 | 1.96 | 16.27 | [1.47, 1.57] |
| `kernel::memory_gate::capabilities_for` | `katu.fn` | 502 | 1.47 | 1.96 | 2.44 | [1.47, 1.61] |
| `kernel::memory_gate::memory_write_use` | `katu.fn` | 1004 | 1.47 | 1.96 | 14.18 | [1.47, 1.64] |
| `kernel::session::context::log_outcome` | `katu.fn` | 502 | 1.47 | 1.96 | 13.69 | [1.47, 1.71] |
| `kernel::session::identity::meta_path` | `katu.fn` | 503 | 1.47 | 1.96 | 7.61 | [1.47, 1.66] |
| `memory::types::new` | `katu.fn` | 501 | 1.47 | 1.96 | 13.48 | [1.47, 1.71] |
| `ports::fs::mem::new` | `katu.fn` | 503 | 1.47 | 1.96 | 13.90 | [1.47, 1.70] |
| `toon::colunar::cell_len` | `katu.fn` | 11022 | 1.47 | 5.31 | 25.77 | [1.47, 2.75] |
| `toon::colunar::push` | `katu.fn` | 2004 | 1.47 | 1.96 | 13.97 | [1.47, 1.59] |
| `kernel::budget::check` | `katu.fn` | 1507 | 1.47 | 1.96 | 4.75 | [1.47, 1.53] |
| `kernel::budget::commit` | `katu.fn` | 1507 | 1.47 | 1.89 | 14.39 | [1.45, 1.51] |
| `kernel::cost::check_per_tool` | `katu.fn` | 1004 | 1.47 | 1.96 | 150.37 | [1.41, 1.99] |
| `kernel::cost::cost_charge_for` | `katu.fn` | 2511 | 1.47 | 1.96 | 19.14 | [1.45, 1.50] |
| `kernel::cost::with_usage` | `katu.fn` | 503 | 1.47 | 1.96 | 7.12 | [1.47, 1.58] |
| `kernel::pipeline::outcome` | `katu.fn` | 1004 | 1.47 | 1.96 | 12.85 | [1.47, 1.55] |
| `kernel::pipeline::report` | `katu.fn` | 1003 | 1.47 | 1.96 | 12.08 | [1.47, 1.52] |
| `kernel::session::maybe_snapshot` | `katu.fn` | 2511 | 1.47 | 1.89 | 15.64 | [1.46, 1.50] |
| `kernel::session::snapshot::snapshot_path` | `katu.fn` | 503 | 1.47 | 1.96 | 2.44 | [1.47, 1.57] |
| `memory::fake::recorded` | `katu.fn` | 502 | 1.47 | 1.96 | 23.26 | [1.45, 1.64] |
| `ports::fs::mem::lock` | `katu.fn` | 4020 | 1.47 | 1.96 | 16.27 | [1.47, 1.52] |
| `report::push_next` | `katu.fn` | 1002 | 1.47 | 1.96 | 18.72 | [1.47, 1.58] |
| `report::to_i64` | `katu.fn` | 1503 | 1.47 | 1.96 | 12.43 | [1.45, 1.50] |
| `report::with_id` | `katu.fn` | 501 | 1.47 | 1.96 | 2.44 | [1.47, 1.54] |
| `toon::colunar::has_control_byte` | `katu.fn` | 6012 | 1.47 | 1.96 | 39.67 | [1.47, 1.59] |
| `toon::colunar::int_len` | `katu.fn` | 4008 | 1.47 | 1.96 | 14.88 | [1.47, 1.51] |
| `toon::colunar::new` | `katu.fn` | 2004 | 1.47 | 1.89 | 14.88 | [1.46, 1.52] |
| `toon::colunar::optional` | `katu.fn` | 1002 | 1.47 | 1.89 | 13.83 | [1.44, 1.50] |
| `toon::project::cell` | `katu.fn` | 1002 | 1.47 | 1.96 | 3.28 | [1.46, 1.49] |
| `toon::str` | `katu.fn` | 501 | 1.47 | 1.89 | 4.33 | [1.45, 1.50] |
| `kernel::cost::check_kill` | `katu.fn` | 1507 | 1.40 | 1.89 | 12.92 | [1.40, 1.46] |
| `kernel::step::require_open` | `katu.fn` | 1004 | 1.40 | 1.89 | 12.64 | [1.40, 1.47] |
| `memory::fake::failure` | `katu.fn` | 1003 | 1.40 | 1.89 | 11.59 | [1.40, 1.50] |
| `memory::types::as_str` | `katu.fn` | 1002 | 1.40 | 1.89 | 18.58 | [1.40, 1.50] |
| `toon::colunar::text` | `katu.fn` | 4008 | 1.40 | 1.89 | 15.71 | [1.40, 1.48] |
| `toon::project::is_scalar` | `katu.fn` | 2004 | 1.40 | 1.89 | 11.73 | [1.40, 1.46] |
| `kernel::pipeline::skipped_outcome` | `katu.fn` | 2 | 0.91 | 1.89 | 1.89 | [0.91, 1.89] |

## 3. Tempo atómico POR OPERAÇÃO — execuções reais (ms)

Fonte: `prime`, `config list`, `memo doctor` e `run` (llama local), **instrumentação ligada**,
build `dev`. Spans "wrapper" (`katu.run`, `cli.run`, `kernel.turn`, `provider.request`) são
**totais com filhos** (incluem o modelo). O IC 95 % é da média (W7).

| operação | n | mín | p50 | p95 | p99 | máx | total | IC 95 % |
|---|---:|---:|---:|---:|---:|---:|---:|---|
| `katu.fn` | 870 | 0.001 | 0.004 | 0.482 | 11.262 | 29967.997 | 209773.318 | [0.004, 419.054] |
| `provider.request` | 10 | 0.002 | 29948.420 | 29949.241 | 29949.241 | 29949.241 | 179693.772 | [8984.739, 29948.420] |
| `cli.run` | 4 | 0.558 | 29966.014 | 29966.606 | 29966.606 | 29966.606 | 89899.203 | [7491.925, 29966.458] |
| `katu.run` | 4 | 0.226 | 0.767 | 29968.084 | 29968.084 | 29968.084 | 29970.107 | [0.427, 22476.255] |
| `kernel.turn` | 1 | 29954.312 | 29954.312 | 29954.312 | 29954.312 | 29954.312 | 29954.312 | [29954.312, 29954.312] |
| `provider.chunk` | 102 | 0.001 | 0.133 | 0.310 | 0.399 | 0.409 | 13.544 | [0.111, 0.155] |
| `bootstrap.init` | 8 | 0.016 | 0.376 | 1.349 | 1.349 | 1.349 | 4.676 | [0.234, 0.980] |
| `fs.write` | 8 | 0.446 | 0.479 | 0.926 | 0.926 | 0.926 | 4.580 | [0.478, 0.697] |
| `skill.discover` | 1 | 4.573 | 4.573 | 4.573 | 4.573 | 4.573 | 4.573 | [4.573, 4.573] |
| `kernel.transition` | 8 | 0.003 | 0.573 | 1.129 | 1.129 | 1.129 | 4.288 | [0.302, 0.780] |
| `log.append` | 7 | 0.002 | 0.499 | 1.002 | 1.002 | 1.002 | 3.679 | [0.305, 0.723] |
| `config.load` | 73 | 0.001 | 0.002 | 0.423 | 0.475 | 0.475 | 3.454 | [0.002, 0.074] |
| `fs.read` | 13 | 0.012 | 0.158 | 0.566 | 0.566 | 0.566 | 2.394 | [0.093, 0.290] |
| `session.open` | 1 | 2.362 | 2.362 | 2.362 | 2.362 | 2.362 | 2.362 | [2.362, 2.362] |
| `cli.memo` | 2 | 0.934 | 0.934 | 0.943 | 0.943 | 0.943 | 1.877 | [0.934, 0.943] |
| `cli.config` | 2 | 0.661 | 0.661 | 0.671 | 0.671 | 0.671 | 1.332 | [0.661, 0.671] |
| `kernel.stop` | 1 | 0.818 | 0.818 | 0.818 | 0.818 | 0.818 | 0.818 | [0.818, 0.818] |
| `fs.list` | 2 | 0.021 | 0.021 | 0.467 | 0.467 | 0.467 | 0.488 | [0.021, 0.467] |
| `memory.open` | 2 | 0.215 | 0.215 | 0.250 | 0.250 | 0.250 | 0.465 | [0.215, 0.250] |
| `policy.load` | 2 | 0.084 | 0.084 | 0.372 | 0.372 | 0.372 | 0.456 | [0.084, 0.372] |
| `context.build` | 4 | 0.001 | 0.002 | 0.439 | 0.439 | 0.439 | 0.445 | [0.002, 0.330] |
| `skill.read` | 8 | 0.036 | 0.043 | 0.069 | 0.069 | 0.069 | 0.369 | [0.041, 0.054] |
| `store.load` | 1 | 0.365 | 0.365 | 0.365 | 0.365 | 0.365 | 0.365 | [0.365, 0.365] |
| `katu.shutdown` | 4 | 0.061 | 0.067 | 0.078 | 0.078 | 0.078 | 0.274 | [0.063, 0.075] |
| `log.replay` | 3 | 0.034 | 0.073 | 0.124 | 0.124 | 0.124 | 0.232 | [0.034, 0.124] |
| `cli.prime` | 3 | 0.008 | 0.092 | 0.117 | 0.117 | 0.117 | 0.217 | [0.008, 0.117] |
| `context.trim` | 2 | 0.012 | 0.012 | 0.200 | 0.200 | 0.200 | 0.213 | [0.012, 0.200] |
| `fs.mkdir` | 3 | 0.010 | 0.019 | 0.148 | 0.148 | 0.148 | 0.177 | [0.010, 0.148] |
| `kernel.step` | 6 | 0.013 | 0.013 | 0.033 | 0.033 | 0.033 | 0.107 | [0.013, 0.025] |
| `memory.status` | 2 | 0.015 | 0.015 | 0.024 | 0.024 | 0.024 | 0.039 | [0.015, 0.024] |
| `scope.load` | 1 | 0.031 | 0.031 | 0.031 | 0.031 | 0.031 | 0.031 | [0.031, 0.031] |
| `cost.check` | 1 | 0.031 | 0.031 | 0.031 | 0.031 | 0.031 | 0.031 | [0.031, 0.031] |
| `cli.input` | 2 | 0.003 | 0.003 | 0.013 | 0.013 | 0.013 | 0.016 | [0.003, 0.013] |
| `policy.audit` | 1 | 0.014 | 0.014 | 0.014 | 0.014 | 0.014 | 0.014 | [0.014, 0.014] |
| `model.project` | 4 | 0.001 | 0.002 | 0.002 | 0.002 | 0.002 | 0.007 | [0.002, 0.002] |

## 4. Baseline versionado — release, instrumentação **desligada** (ns)

Fonte: [`bench/mvk/raw.json`](../../mvk/raw.json) (caminho zero-custo: `diag` fora do binário).

| operação | n | p50 | p95 |
|---|---:|---:|---:|
| `kernel.transition` | 2511 | 14388 | 21023 |
| `memory.write` | 502 | 9918 | 10477 |
| `log.append` | 2511 | 6635 | 11315 |
| `policy.evaluate` | 502 | 2374 | 2445 |
| `kernel.step` | 2511 | 2305 | 3353 |
| `tool.write` | 501 | 1816 | 1956 |
| `log.replay` | 1006 | 1466 | 1886 |

## 5. Gates (observado na geração)

| gate | p95 medido | orçamento |
|---|---:|---:|
| `gate:render` (quadro) | 933932 ns | 3000000 ns |
| `gate:provider` (overhead cliente) | 1862137 ns | 5000000 ns |

## 6. Limitações (honestidade)

- **Por função em execução real**: o `StderrSink` imprime `Record.function` (Q-09), pelo que os
  spans por função existem no `stderr`; a secção 2 usa o `AggregatingSink`
  (`crates/katu-core/src/diag/aggregate.rs`, chave `(event, function)`) via `measure_mvk`.
- **Dev vs release**: secções 2 e 3 são `dev` (com overhead do `diag` ligado); secção 4 é `release`
  sem instrumentação.
- **`const fn`** (136) e **`katu-policy`** ficam fora da métrica.
- Números de máquina (Ryzen 5 5500U, CPU); percentis variam com carga.

## 7. Artefacto cru

[`raw.json`](raw.json) — `schema: 1`, com `coverage`, `functions`, `operations`,
`baseline_release` e `gates`, chaves ordenadas para *diff* estável; cada resumo de tempo leva o
IC 95 % da média (`ci95`, W7/E18-T10).
