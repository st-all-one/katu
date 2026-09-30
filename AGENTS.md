# AGENTS.md — router

Este ficheiro é um **router**, não um manual. Cada facto tem **um lar**; aqui só há ponteiros.
Um link quebrado aqui é uma falha de arranque (`cargo xtask check-docs`).

## Começa aqui

- **Regras do agente** (≤ 2 saltos até qualquer regra): [`docs/agent-rules.md`](docs/agent-rules.md).
- **Catálogo gerado**: [`docs/catalog.md`](docs/catalog.md).
- **Superfície de utilizador** (CLI + TUI): [`docs/CLI_TUI_SURFACE.md`](docs/CLI_TUI_SURFACE.md).
- **Reforma da superfície (E20)**: [`SURFACE_IMPLEMENTATION.md`](SURFACE_IMPLEMENTATION.md).
- **Postmortems**: [`docs/postmortems/README.md`](docs/postmortems/README.md).
- **Estado operacional**: [`STAGING.md`](STAGING.md).
- **Plano**: [`IMPLEMENTATION_PLAN.md`](IMPLEMENTATION_PLAN.md) · [`plan/README.md`](plan/README.md).
- **Decisões registadas**: [`docs/adr/README.md`](docs/adr/README.md).

## Por crate (o lar de cada módulo)

- [`crates/katu-core/MODULE.md`](crates/katu-core/MODULE.md)
- [`crates/katu-policy/MODULE.md`](crates/katu-policy/MODULE.md)
- [`crates/katu-tools/MODULE.md`](crates/katu-tools/MODULE.md)
- [`crates/katu-providers/MODULE.md`](crates/katu-providers/MODULE.md)
- [`crates/katu-tui/MODULE.md`](crates/katu-tui/MODULE.md)
- [`crates/katu/MODULE.md`](crates/katu/MODULE.md)

## Regra de manutenção

Se um facto não está no seu lar, move-o; **não** o copies para cá. O `cargo xtask check-docs`
rejeita títulos duplicados, corpos idênticos, tópicos órfãos e excesso de linhas no router.
