# NNNN — Título curto do incidente

## Executive summary

Duas ou três frases: o que se partiu, durante quanto tempo, e qual foi a causa.

## Impact

O que ficou errado para o utilizador ou para o sistema; âmbito e duração.

## Timeline

- `HH:MM` — deteção.
- `HH:MM` — mitigação.
- `HH:MM` — correção.

## Root cause

A causa **mecânica**, não a narrativa. Porque é que o guardrail existente não apanhou isto?

## Guardrails added

- Teste que falha sem a correção:
  [`verify`](../../crates/katu/src/agent/tests/verify.rs).

## Lessons

O que muda no processo (não no prompt). O que deixámos de poder assumir.
