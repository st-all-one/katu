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

## Alocações por quadro (E18-T10) — a afirmação «render sem alocações» é **falsa**

O plano escrevia «falta verificar zero alocações no render». **Verificado e medido**: o render
aloca **1136–1139 alocações e ~154 kB por quadro** no pior caso (120×40, conversa no teto).

### Como se mede

`crates/katu/tests/render_alloc.rs` instala um `#[global_allocator]` de contagem (8 quadros de
aquecimento + 40 medidos; a bandeira só conta dentro da janela). É o **segundo ponto `unsafe` do
projecto**, registado em `EXCEPTIONS` (`xtask/src/unsafe_check.rs`) ao lado do `kill(2)`: o
invólucro limita-se a repassar cada `Layout` intacto para `System` e a somar dois atómicos, e tem
quatro testes próprios (silêncio fora da janela, contagem dentro, `alloc_zeroed` utilizável,
`realloc` a preservar conteúdo). Vive num **alvo de integração** — não toca o binário — e o
`katu-tui` mantém `#![forbid(unsafe_code)]` sem escape hatch.

```sh
KATU_RENDER_ALLOC_OUT=$PWD/bench/render/alloc.json \
  cargo test -q -p katu --test render_alloc
```

### O que se trava

O orçamento em [`budget.toml`](budget.toml) (`frame_allocations`, `frame_alloc_bytes`) é um **teto
de crescimento**, não um alvo de zero: o teste falha se o render ultrapassar. As contagens variam
±1 entre execuções (o `TestBackend` reconstrói o buffer) e os bytes ~0,2 %, por isso o orçamento
tem folga; a afirmação de zero alocações fica registada como **falsa** no artefacto
(`zero_allocations: false`).

### Decisão

Não se refaz o render com dados emprestados para a v0.1. O custo real por quadro é o **tempo**
(medido no `gate:render`), não o número de alocações: 1137 alocações de ~135 B cada dão ~0,7 ms,
e o orçamento de latência já é um gate. Refazer `ui/transcript/trash/overlay/menu` para `&'static
str`/`Cow` é uma alteração wide-spread num caminho já verificado por testes de render — fica
registado como opção, com o número que teria de bater, e só entra com A/B próprio.

### Limites

- Mede-se o render **puro** sobre `TestBackend`; um terminal real acrescenta o flush do driver.
- A contagem é por quadro depois do aquecimento: o primeiro quadro aloca muito mais (caches do
  `ratatui`), e esse custo de arranque não está no número.
