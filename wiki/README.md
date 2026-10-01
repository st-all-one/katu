# `wiki` — a tese, o método e a referência

> Duas coisas com natures diferentes, deliberadamente separadas: o que o projecto **defende**
> ([`proposition/`](proposition/)) e o que o projecto **decidiu** ([`_ref/`](_ref/README.md)).

## [`proposition/` — a tese destrinchada](proposition/00-tese.md)

Leitura para quem quer saber *o que o katu sustenta e onde pode estar errado*, sem percorrer os
planos.

| | |
|---|---|
| [00 · A tese](proposition/00-tese.md) | a afirmação em três pilares, em forma falsificável |
| [01 · Guardrails](proposition/01-guardrails.md) | a política como kernel: decidir antes do efeito, com evidência |
| [02 · Memória](proposition/02-memoria.md) | store local, persistente, auditável, substituível |
| [03 · Performance](proposition/03-performance.md) | o programa de optimização, com o que ganhou e o que rejeitou |
| [04 · Falsabilidade](proposition/04-falsabilidade.md) | o que o projecto **não** prova, com o número em falta |
| [05 · Método](proposition/05-metodo.md) | como se mede: DF5, artefactos, gates, invariantes |

## [`_ref/` — a referência](_ref/README.md)

ADRs, planos (épicos, `IMPLEMENTATION_PLAN`, `OPTIMIZATION_PLAN`, `SURFACE_IMPLEMENTATION`),
brainstorm e a documentação derivada. Material que se consulta; a raiz do repositório é o que se usa.

## Regra

Um facto, um lar: se um número, uma decisão ou um plano mudou de vez, muda de ficheiro e o link
segue-o. `cargo xtask check` falha com links mortos, títulos duplicados ou tópicos órfãos.
