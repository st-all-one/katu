# Postmortems

Um postmortem é escrito quando um bug é **sútil, sistémico e caro** (E14-T02). O objetivo não é
culpar ninguém: é deixar um **guardrail novo com teste** que impeça a repetição.

## Template

Copie [`0000-template.md`](0000-template.md) para `NNNN-<slug>.md` e preencha. As secções
seguintes são obrigatórias (verificadas por `cargo xtask check-docs`):

- `## Executive summary` (leitura de 30 s)
- `## Impact`
- `## Timeline`
- `## Root cause`
- `## Guardrails added`
- `## Lessons`

`## Guardrails added` tem de ligar pelo menos um **teste** (`.rs`): um postmortem sem guardrail é
uma história, não uma correção.

## Índice

- [0001 — `changed_files` absoluto faz o gate de verificação falhar aberto](0001-fail-open-changed-files.md)
