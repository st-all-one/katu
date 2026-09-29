# Architecture Decision Records (ADRs)

> **E14-T01.** Uma decisão arquitetural sem o que **venceu** convida a re-litigá-la. Cada ADR
> regista o contexto, a decisão e — obrigatoriamente — as **alternativas consideradas**.

## Convenções

- **Ficheiro:** `NNNN-titulo-em-kebab.md`, com `NNNN` zero-padded (`0001`, `0002`, …). Números
  **nunca** se reutilizam.
- **Secções obrigatórias:** `## Contexto`, `## Decisão`, `## Alternativas consideradas`,
  `## Consequências`.
- **Imutáveis:** uma ADR **não** se edita para outra decisão. Substitui-se por uma nova e liga-se
  a ambas (`Supersedes` / `Superseded by`).
- **Verificação:** `xtask check-docs` (em `make check`) falha se faltar `## Alternatives considered`.
- **Relação com o plano:** as decisões **fundacionais** vivem em
  [`plan/01-decisoes-fundacionais.md`](../../plan/01-decisoes-fundacionais.md) (`DFxx`); a ADR
  regista a **decisão de fase** que as materializa (ex.: um gate que passa). Um facto, um lar.

## Índice

| ADR | Título | Estado |
|---|---|---|
| [0001](0001-mvk-gate-aprovado.md) | MVK aprovado — o loop possuído (DF1) torna-se compromisso | aceite |
| [0002](0002-ferramentas-ai-first.md) | Ferramentas AI-first: envelope + views + TOON (core por medição) | aceite |
| [0003](0003-vocabulario-v2-contencao.md) | Vocabulário de política v2: leitura sensível e acesso fora do workspace | aceite |
| [0004](0004-sem-ffi-kill-grupo-e17.md) | Sem FFI no MVP: kill do grupo de processos fica para a jail (E17) | aceite |

## Template

```markdown
# ADR NNNN — <título>

- **Estado:** proposto | aceite | substituído por ADR NNNN
- **Data:** AAAA-MM-DD
- **Decisões fundacionais:** DFxx, …
- **Épicos:** Enn, …

## Contexto

O que obrigou a decidir; a evidência disponível (com artefacto, DF5).

## Decisão

O que fica decidido, em uma ou duas frases verificáveis.

## Alternativas consideradas

1. **<alternativa>.** Porque foi rejeitada.
2. …

## Consequências

- **Positivas:** …
- **Negativas / dívida:** …
- **Travas:** testes/gates que impedem a regressão.
```
