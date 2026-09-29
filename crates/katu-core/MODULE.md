# `katu-core`

**Épico:** E04 · **Fase:** 2 (MVK) · **Crate puro** (kernel, sem providers).

O **kernel** do katu: a máquina de estados do agente (DF1). O estado é um valor, a transição é uma
função, o log é a fonte da verdade.

## Responsabilidade

- `State`, `Event`, `Refusal`, pipeline de tool call, log append-only.
- `derive_messages`/`snapshot` — projeções puras.
- Porta [`memory::Memory`](src/memory.rs) (tipos do katu, DF6).
- Diagnóstico transversal [`diag`](src/diag/mod.rs) (DF9/E19): log estruturado + métrica de tempo,
  custo zero por defeito; catálogo de eventos em [`diag::events`](src/diag/events.rs).

## Fronteira

- Depende de `katu-policy`; **não** depende de `katu-tools`/`katu-providers`/`katu-tui` nem de
  `knudge-core` (o adaptador vive no binário, E03).
- Invariante: `Model-visible ⟺ logged`.
