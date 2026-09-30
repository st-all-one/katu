# Protocolo — render por quadro da TUI (E15-T01 / E10-T03)

## O que se mede

O custo de **um quadro completo** da TUI (`katu_tui::render`) num `TestBackend` de 120×40, com a
conversa no teto de E10-T03 (200 entradas) e o painel de atividade preenchido. Mede o caminho de
render puro — o mesmo que corre no terminal real; não inclui I/O.

## Como se mede

- `cargo run -q -p xtask -- bench-render` gera `bench/render/frame.json` (amostras cruas +
  p50/p95/p99/max) e imprime um resumo JSON.
- `cargo run -q -p xtask -- gate:render` **re-mede** 400 quadros e falha se o p95 exceder
  `bench/render/budget.toml`.

O gate é determinístico (sem rede, sem terminal): o `ratatui` aplica o diff de células sobre o
backend de teste.

## Honestidade (DF5)

O número publicado em `bench/published.toml` cita este artefacto; a base é `measured`. O render é
CPU-local, pelo que o p95 varia com a máquina — o orçamento tem folga para o CI. O artefacto é
regenerado **na máquina declarada** antes de atualizar o número.
