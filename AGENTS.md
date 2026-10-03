# AGENTS.md — router

Um **router**, não um manual: cada facto tem **um lar**, aqui só ponteiros. Link quebrado é falha de
arranque (`cargo xtask check`).

## Começa aqui

- **Regras do agente** (≤ 2 saltos a qualquer regra): [`agent-rules`](wiki/_ref/docs/agent-rules.md).
- **Referência** (ADRs, planos, brainstorm, docs): [`_ref`](wiki/_ref/README.md).
- **Catálogo gerado**: [`catalog`](wiki/_ref/docs/catalog.md) ·
  **superfície CLI/TUI**: [`CLI_TUI_SURFACE`](wiki/_ref/docs/CLI_TUI_SURFACE.md).
- **Otimização (Q/P/S)**: [`OPTIMIZATION_PLAN`](wiki/_ref/plan/OPTIMIZATION_PLAN.md) ·
  [`leituras de hoje`](bench/e18/pos/PROTOCOL.md).
- **Kernel e loop**: [`KERNEL_SURFACE`](wiki/_ref/plan/KERNEL_SURFACE.md) ·
  [`LOOP_RESILIENCE`](wiki/_ref/plan/LOOP_RESILIENCE.md).

## Por crate (o lar de cada módulo)

[`katu-core`](crates/katu-core/MODULE.md) · [`katu-policy`](crates/katu-policy/MODULE.md) ·
[`katu-tools`](crates/katu-tools/MODULE.md) · [`katu-providers`](crates/katu-providers/MODULE.md) ·
[`katu-tui`](crates/katu-tui/MODULE.md) · [`katu`](crates/katu/MODULE.md)

## Regra de manutenção

Facto fora do seu lar: **move-se**, não se copia. O `cargo xtask check` rejeita títulos duplicados,
corpos idênticos, tópicos órfãos, links mortos e excesso de linhas no router.