# Desempenho

Orçamentos **versionados** e medidos em CI, não a olho.

## Portões de desempenho

- `cargo xtask gate:render` — orçamento de render (`bench/render/budget.toml`).
- `cargo xtask gate:provider` — orçamento de provider.
- `cargo xtask gate:bench` — regressão de micro-benchmarks.

## Regra de evidência

Um número publicado cita o artefacto que o mede (DF5). Ver
[`../../plan/16-performance-benchmarks.md`](../../plan/16-performance-benchmarks.md).
