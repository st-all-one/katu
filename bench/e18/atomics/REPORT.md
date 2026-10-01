# Tempos atómicos do katu (E19-T03)

> **Gerado** por medição do caminho real; artefacto cru: [`raw.json`](raw.json). Para comparação
> futura, comparar tabelas/`raw.json` entre commits (`git diff bench/e18/atomics/`). Método em
> [`PROTOCOL.md`](PROTOCOL.md).

## 1. Cobertura de instrumentação (por função)

| | cobertas | total | % |
|---|---:|---:|---:|
| funções **instrumentáveis** (não-`const`, fora de testes, com corpo) | 1187 | 1194 | 99.4 % |
| com `const fn` no denominador (não instrumentáveis) | 1187 | 1315 | 90.2 % |

`katu-policy` excluída por desenho (firewall: instrumentada **pelo chamador**). `const fn` = 121.

## 2. Tempo atómico POR FUNÇÃO — caminho MVK (µs)

Fonte: `examples/measure_mvk`, sink agregador, **instrumentação ligada**, build `dev`. As durações
**incluem** o custo do `diag` aninhado (só existe quando ligado).

| função | span | n | p50 | p95 | máx |
|---|---|---:|---:|---:|---:|
| `kernel::session::memory_write` | `memory.write` | 502 | 347.74 | 454.67 | 706.80 |
| `kernel::memory_gate::enforce_memory_write` | `memory.write` | 502 | 221.89 | 281.04 | 418.70 |
| `kernel::pipeline::dispatch_with` | `katu.fn` | 502 | 205.05 | 262.32 | 390.97 |
| `kernel::pipeline::with_estimated_cost` | `katu.fn` | 501 | 157.00 | 201.63 | 297.11 |
| `report::to_toon` | `toon.emit` | 501 | 153.72 | 197.37 | 291.52 |
| `toon::colunar::emit` | `toon.emit` | 501 | 89.12 | 122.08 | 171.53 |
| `kernel::session::open` | `katu.fn` | 503 | 64.88 | 94.50 | 139.61 |
| `kernel::session::open_with_cap` | `katu.fn` | 503 | 61.53 | 89.12 | 134.79 |
| `kernel::session::open_with_cost` | `store.load` | 503 | 58.18 | 84.37 | 127.11 |
| `toon::colunar::emit_rows` | `toon.emit` | 1002 | 51.96 | 76.06 | 119.01 |
| `kernel::session::apply` | `katu.fn` | 2511 | 50.70 | 86.32 | 160.78 |
| `kernel::session::apply_at` | `kernel.transition` | 2511 | 47.49 | 82.69 | 155.47 |
| `write::execute` | `tool.write` | 501 | 31.50 | 47.00 | 78.71 |
| `toon::project::project` | `toon.project` | 501 | 29.12 | 41.07 | 80.11 |
| `toon::project::project_map` | `toon.project` | 501 | 25.77 | 36.25 | 73.96 |
| `report::envelope` | `katu.fn` | 501 | 21.86 | 32.48 | 79.27 |
| `kernel::log::append` | `katu.fn` | 2511 | 19.14 | 32.97 | 70.40 |
| `kernel::log::append` | `log.append` | 2511 | 15.64 | 28.91 | 66.07 |
| `write::record_report` | `katu.fn` | 501 | 15.16 | 21.93 | 49.73 |
| `kernel::cost::check` | `cost.check` | 1507 | 13.27 | 16.41 | 48.47 |
| `kernel::session::identity::load_meta` | `katu.fn` | 503 | 11.31 | 16.41 | 30.03 |
| `kernel::session::snapshot::load` | `katu.fn` | 503 | 10.89 | 14.46 | 42.12 |
| `kernel::cost::check_layers` | `katu.fn` | 1507 | 10.13 | 12.64 | 44.21 |
| `kernel::log::read_records_with_len` | `log.replay` | 503 | 8.03 | 10.41 | 29.82 |
| `memory::fake::record` | `katu.fn` | 501 | 7.68 | 10.55 | 26.68 |
| `kernel::step::step` | `kernel.step` | 2511 | 7.54 | 13.27 | 49.73 |
| `toon::colunar::emit_cell` | `katu.fn` | 5511 | 7.47 | 9.92 | 57.20 |
| `kernel::step::tool_call` | `katu.fn` | 1004 | 6.63 | 10.82 | 39.11 |
| `ports::fs::mem::append` | `katu.fn` | 2511 | 5.10 | 7.12 | 28.15 |
| `toon::colunar::push_sanitized` | `katu.fn` | 3006 | 4.89 | 6.78 | 30.52 |
| `kernel::cost::commit` | `katu.fn` | 1507 | 4.75 | 6.78 | 30.52 |
| `kernel::log::resume` | `katu.fn` | 503 | 4.75 | 5.87 | 26.82 |
| `ports::fs::mem::exists` | `katu.fn` | 503 | 4.75 | 6.29 | 24.58 |
| `kernel::cost::new` | `katu.fn` | 503 | 4.61 | 5.80 | 15.16 |
| `toon::project::cell` | `katu.fn` | 501 | 4.33 | 5.73 | 8.52 |
| `memory::fake::pre_write` | `katu.fn` | 502 | 4.33 | 5.73 | 25.42 |
| `ports::fs::mem::read` | `katu.fn` | 1006 | 4.33 | 6.08 | 27.24 |
| `kernel::step::tool_result` | `katu.fn` | 1004 | 3.35 | 7.26 | 23.26 |
| `None` | `policy.evaluate` | 502 | 2.44 | 5.24 | 19.14 |
| `kernel::pipeline::facts_from` | `katu.fn` | 502 | 1.96 | 2.93 | 22.49 |
| `kernel::log::session_path` | `katu.fn` | 1006 | 1.89 | 3.35 | 22.56 |
| `memory::fake::default` | `katu.fn` | 503 | 1.89 | 2.44 | 18.23 |
| `kernel::state::initial` | `katu.fn` | 503 | 1.47 | 2.37 | 4.40 |
| `toon::colunar::has_delimiter` | `katu.fn` | 3006 | 1.47 | 2.38 | 21.09 |
| `kernel::step::turn_start` | `kernel.transition` | 503 | 1.47 | 2.38 | 18.02 |
| `kernel::pipeline::skipped_outcome` | `katu.fn` | 2 | 1.47 | 1.96 | 1.96 |
| `kernel::session::identity::meta_path` | `katu.fn` | 503 | 1.47 | 1.96 | 9.01 |
| `kernel::session::snapshot::snapshot_path` | `katu.fn` | 503 | 1.47 | 1.96 | 8.94 |
| `memory::types::new` | `katu.fn` | 501 | 1.47 | 1.96 | 15.85 |
| `kernel::budget::check` | `katu.fn` | 1507 | 1.40 | 1.89 | 2.44 |
| `kernel::budget::commit` | `katu.fn` | 1507 | 1.40 | 1.89 | 19.14 |
| `kernel::cost::check_kill` | `katu.fn` | 1507 | 1.40 | 1.89 | 18.16 |
| `kernel::cost::check_per_tool` | `katu.fn` | 1004 | 1.40 | 1.89 | 6.22 |
| `kernel::cost::cost_charge_for` | `katu.fn` | 2511 | 1.40 | 1.89 | 22.07 |
| `kernel::cost::with_usage` | `katu.fn` | 503 | 1.40 | 1.96 | 2.86 |
| `kernel::event::new` | `katu.fn` | 1506 | 1.40 | 1.96 | 20.39 |
| `kernel::memory_gate::capabilities_for` | `katu.fn` | 502 | 1.40 | 1.96 | 16.34 |
| `kernel::memory_gate::memory_write_use` | `katu.fn` | 1004 | 1.40 | 1.96 | 28.70 |
| `kernel::pipeline::outcome` | `katu.fn` | 1004 | 1.40 | 1.89 | 16.55 |
| `kernel::pipeline::report` | `katu.fn` | 501 | 1.40 | 1.89 | 2.37 |
| `kernel::session::context::log_outcome` | `katu.fn` | 502 | 1.40 | 1.96 | 27.94 |
| `kernel::step::require_open` | `katu.fn` | 1004 | 1.40 | 1.89 | 2.44 |
| `memory::fake::failure` | `katu.fn` | 1003 | 1.40 | 1.89 | 17.74 |
| `memory::fake::recorded` | `katu.fn` | 502 | 1.40 | 1.89 | 2.38 |
| `memory::types::as_str` | `katu.fn` | 1002 | 1.40 | 1.89 | 17.53 |
| `ports::fs::mem::lock` | `katu.fn` | 4020 | 1.40 | 1.89 | 22.00 |
| `ports::fs::mem::new` | `katu.fn` | 503 | 1.40 | 1.96 | 6.63 |
| `report::push_next` | `katu.fn` | 501 | 1.40 | 1.89 | 2.38 |
| `report::with_id` | `katu.fn` | 501 | 1.40 | 1.89 | 17.39 |
| `toon::colunar::new` | `katu.fn` | 1002 | 1.40 | 1.96 | 31.01 |
| `toon::colunar::optional` | `katu.fn` | 501 | 1.40 | 1.89 | 2.44 |
| `toon::colunar::push` | `katu.fn` | 1002 | 1.40 | 1.96 | 19.21 |
| `toon::colunar::text` | `katu.fn` | 2505 | 1.40 | 1.89 | 20.67 |
| `toon::project::is_scalar` | `katu.fn` | 1002 | 1.40 | 1.89 | 16.76 |
| `toon::str` | `katu.fn` | 501 | 1.40 | 1.89 | 13.62 |

## 3. Tempo atómico POR OPERAÇÃO — execuções reais (ms)

Fonte: `prime`, `config list`, `memo doctor` e `run` (llama local), **instrumentação ligada**,
build `dev`. Spans "wrapper" (`katu.run`, `cli.run`, `kernel.turn`, `provider.request`) são
**totais com filhos** (incluem o modelo).

| operação | n | mín | p50 | p95 | p99 | máx | total |
|---|---:|---:|---:|---:|---:|---:|---:|
| `katu.fn` | 522 | 0.001 | 0.004 | 0.395 | 20197.226 | 20206.577 | 141442.828 |
| `provider.request` | 10 | 0.002 | 20195.339 | 20196.631 | 20196.631 | 20196.631 | 121177.191 |
| `cli.run` | 4 | 1.009 | 20204.552 | 20205.614 | 20205.614 | 20205.614 | 60615.693 |
| `katu.run` | 4 | 0.639 | 1.616 | 20206.916 | 20206.916 | 20206.916 | 20210.478 |
| `kernel.turn` | 1 | 20200.339 | 20200.339 | 20200.339 | 20200.339 | 20200.339 | 20200.339 |
| `provider.chunk` | 53 | 0.001 | 0.194 | 0.680 | 1.001 | 1.241 | 14.750 |
| `config.load` | 45 | 0.001 | 0.002 | 0.786 | 0.922 | 0.922 | 4.290 |
| `cli.memo` | 2 | 1.417 | 1.417 | 1.436 | 1.436 | 1.436 | 2.853 |
| `cli.config` | 2 | 1.073 | 1.073 | 1.115 | 1.115 | 1.115 | 2.188 |
| `bootstrap.init` | 5 | 0.030 | 0.181 | 0.861 | 0.861 | 0.861 | 1.618 |
| `kernel.transition` | 7 | 0.002 | 0.279 | 0.381 | 0.381 | 0.381 | 1.480 |
| `session.open` | 1 | 1.325 | 1.325 | 1.325 | 1.325 | 1.325 | 1.325 |
| `katu.shutdown` | 4 | 0.119 | 0.261 | 0.265 | 0.265 | 0.265 | 0.777 |
| `kernel.stop` | 1 | 0.557 | 0.557 | 0.557 | 0.557 | 0.557 | 0.557 |
| `policy.load` | 2 | 0.113 | 0.113 | 0.369 | 0.369 | 0.369 | 0.482 |
| `cli.prime` | 3 | 0.010 | 0.160 | 0.251 | 0.251 | 0.251 | 0.421 |
| `log.append` | 5 | 0.039 | 0.074 | 0.141 | 0.141 | 0.141 | 0.415 |
| `store.load` | 1 | 0.397 | 0.397 | 0.397 | 0.397 | 0.397 | 0.397 |
| `memory.open` | 2 | 0.183 | 0.183 | 0.206 | 0.206 | 0.206 | 0.389 |
| `context.build` | 1 | 0.377 | 0.377 | 0.377 | 0.377 | 0.377 | 0.377 |
| `fs.write` | 7 | 0.010 | 0.025 | 0.145 | 0.145 | 0.145 | 0.319 |
| `log.replay` | 3 | 0.027 | 0.080 | 0.174 | 0.174 | 0.174 | 0.281 |
| `kernel.step` | 5 | 0.026 | 0.030 | 0.047 | 0.047 | 0.047 | 0.176 |
| `skill.discover` | 1 | 0.103 | 0.103 | 0.103 | 0.103 | 0.103 | 0.103 |
| `fs.read` | 4 | 0.016 | 0.021 | 0.031 | 0.031 | 0.031 | 0.083 |
| `cost.check` | 1 | 0.066 | 0.066 | 0.066 | 0.066 | 0.066 | 0.066 |
| `memory.status` | 2 | 0.028 | 0.028 | 0.033 | 0.033 | 0.033 | 0.061 |
| `fs.mkdir` | 3 | 0.007 | 0.016 | 0.037 | 0.037 | 0.037 | 0.060 |
| `fs.list` | 2 | 0.027 | 0.027 | 0.032 | 0.032 | 0.032 | 0.059 |
| `scope.load` | 1 | 0.054 | 0.054 | 0.054 | 0.054 | 0.054 | 0.054 |
| `context.trim` | 1 | 0.048 | 0.048 | 0.048 | 0.048 | 0.048 | 0.048 |
| `cli.input` | 2 | 0.001 | 0.001 | 0.007 | 0.007 | 0.007 | 0.009 |
| `model.project` | 3 | 0.001 | 0.001 | 0.002 | 0.002 | 0.002 | 0.005 |

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
| `gate:render` (quadro) | 1027438 ns | 3000000 ns |
| `gate:provider` (overhead cliente) | 2251760 ns | 5000000 ns |

## 6. Limitações (honestidade)

- **Por função em execução real**: o `StderrSink` não imprime `Record.function`; no `run`/CLI os
  spans de função colapsam em `katu.fn`. O mapeamento por função existe no `AggregatingSink`
  (`crates/katu-core/src/diag/aggregate.rs`, chave `(event, function)`) e é exercido via
  `measure_mvk` (secção 2).
- **Dev vs release**: secções 2 e 3 são `dev` (com overhead do `diag` ligado); secção 4 é `release`
  sem instrumentação.
- **`const fn`** (121) e **`katu-policy`** ficam fora da métrica.
- Números de máquina (Ryzen 5 5500U, CPU); percentis variam com carga (ex.: `gate:provider`
  oscila entre ~2,2 ms e ~5,7 ms).

## 7. Artefacto cru

[`raw.json`](raw.json) — `schema: 1`, com `coverage`, `functions`, `operations`,
`baseline_release` e `gates`, chaves ordenadas para *diff* estável.
