# Q-14 · Prewarm do prefixo a frio — protocolo e resultado (**revertido**)

## Pergunta

Vale a pena aquecer o prefixo canónico antes do primeiro turno? (Item Q-14 do
`OPTIMIZATION_PLAN.md` §Q-E: "o 1.º turno local paga 3265 tokens a `cached=0` (~40 s)".)

## Fórmula

O primeiro turno a frio paga a avaliação do prefixo (`system` + tools):

```
frio  = avaliar(prefixo)            (~29-32 s para 2774 tokens no servidor local)
quente = avaliar(prefixo − cache)   (~86 ms: 1 token)
```

O prewarm manda esse mesmo prefixo num pedido mínimo e corta no primeiro evento, para que o turno
seguinte o encontre na cache. **O custo não desaparece: desloca-se** — para o arranque (síncrono) ou
para uma thread concorrente.

## Cenário

`llama-server` local (llama.cpp b11269, `qwen2.5-coder-1.5b`, `n_ctx = 4096`), prefixo canónico de
**2774 tokens** (prime + `AGENTS.md` + catálogo de skills + 11 tools). O servidor é **reiniciado**
entre as medições para garantir cache vazia. As durações vêm do **próprio servidor**
(`prompt eval time`), não do relógio do cliente — o wall do processo varia com a carga da máquina.

## Como correr

```sh
systemctl --user restart katu-llama.service && sleep 3
katu run "diz apenas: olá" --json --provider llama --model qwen   # frio: cached = 0
katu run "diz apenas: olá" --json --provider llama --model qwen   # quente: cached = 2773
```

## Resultado (artefacto `raw.json`)

| | avaliação do prefixo | `cached` | wall do processo |
|---|---|---|---|
| **frio** (cache vazia) | **32 383 ms** (11,67 ms/token) | 0 | 30,71 s |
| **quente** (mesmo prefixo) | **86 ms** (1 token) | 2 773 | 1,03 s |
| **prewarm + turno imediato** | — | — | **92,69 s** e o turno voltou **vazio** |

O prémio é real: o mesmo turno passa de ~30 s para ~1 s quando o prefixo está na cache. Mas a
terceira linha é o que decide: com o prewarm em voo **ao mesmo tempo** que o turno, o servidor não
consegue servir os dois — `n_ctx = 4096` e **2 × 2774 tokens** não cabem na cache KV:

```
decode: failed to find a memory slot for batch of size 1024
slot operator(): need to evaluate at least 1 token for each active slot (n_past = 2774, task.n_tokens() = 2774)
```

## Decisão: **revertido**

- A implementação existiu (thread própria, pedido mínimo com o prefixo exato, `behavior.prewarm`
  opt-in, testes de forma do pedido e de "não entra no log") e foi **medida**.
- Critério do plano (≥ 50 % do frio): cumprido **só** quando o prewarm termina antes do turno. Com o
  turno imediato o resultado é **pior e incorreto** (92,7 s, resposta vazia). Um prewarm que quebra o
  primeiro turno não se deixa atrás de uma flag que ninguém lê: o código foi removido.
- O que fica é a **medição** (esta página e `raw.json`) e a explicação do porquê.

## Limites e o que ficaria de pé

- O ganho exige que o prewarm **acabe** antes do primeiro turno: só um cliente que espere por ele (ou
  um servidor com contexto suficiente para os dois pedidos) tira proveito.
- Num servidor com `n_ctx` maior (ou com *slots* independentes a sério) a contenção desaparece; num
  endpoint remoto a cache de prefixo é gerida pelo fornecedor e o custo do prewarm é **pago em
  tokens** — a decisão teria de ser re-medida lá.
- Alternativa não explorada (fica registada): **esperar** pelo prewarm no arranque em vez de o
  concorrer — desloca o custo para o sítio onde o utilizador já espera, mas não o elimina; não há
  ganho líquido sem trabalho concorrente do utilizador (digitar, ler o ecrã).
