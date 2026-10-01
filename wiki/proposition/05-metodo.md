# 05 · Método: como o projecto mede e se trava

> O método é a parte da tese que sobrevive a todos os pilares: sem ele, os números dos capítulos
> anteriores seriam opinião com formatação. Este documento descreve as regras, quem as executa e
> onde cada uma está no código.

## 1. DF5 — um número é uma afirmação com base

Regra estrutural do projecto: um número publicado carrega

- o **nome** estável da métrica,
- o **valor** e a **unidade** (`Unit::Millis`, `Unit::Nanos`, `Unit::Ratio`, `Unit::Count`, …),
- a **base de evidência** (`EvidenceBasis`),
- o **artefacto cru** que o produziu — obrigatório quando a base o exige.

Bases: `measured` (relógio ou contador), `inferred` (agregação ou modelo), `provider_reported` (o
endpoint disse), `benchmark_counterfactual`, `observed`, `verified`, `unpriced`.

Duas regras que daí decorrem e que valem mais do que o enum:

- **`unpriced` vale zero.** Não há `unpriced = 0,9`. O que não se mediu não se inventa; a linha fica
  visível para se saber que a ausência é conhecida.
- **A base não sobe numa agregação.** Se um número é agregação de `measured` e `inferred`, é
  `inferred` — a média de um número medido com um inferido não é um número medido.

O gate `xtask gate:bench` falha se um número publicável não tiver artefacto, se o artefacto não
existir no repositório, ou se uma linha `unpriced` tiver valor diferente de zero.

## 2. Onde o número é produzido

| família | artefacto | quem executa | o que mede |
|---|---|---|---|
| composição do prompt | [`bench/e18/prompt/`](../../bench/e18/prompt/PROTOCOL.md) | `gate:prompt` | bytes e tokens do prompt, com tetos versionados |
| turno e2e | [`bench/e18/`](../../bench/e18/PROTOCOL.md) · [`pos/`](../../bench/e18/pos/PROTOCOL.md) | `scripts/bench-pos.sh` | o turno real, por eixo, com atribuição declarada |
| lote de tools | [`bench/e18/batch/`](../../bench/e18/batch/PROTOCOL.md) | `cargo test --ignored` | A/B sequencial vs. paralelo |
| provider | [`bench/providers/latency.json`](../../bench/providers/latency.json) | `gate:provider` | TTFT/overhead contra orçamento, offline e determinístico |
| render | [`bench/render/`](../../bench/render/PROTOCOL.md) | `gate:render` · `--test render_alloc` | p95 por quadro e alocações por quadro |
| política/confiança | [`bench/e18/confidence/`](../../bench/e18/confidence/PROTOCOL.md) | `policy:confidence` | veredicto por regra, com evidência |
| taint | [`bench/e18/taint/`](../../bench/e18/taint/PROTOCOL.md) | `cargo test --ignored` | ataques bloqueados e custo do envelope |

Cada `PROTOCOL.md` tem: **pergunta · fórmula · método · resultado · decisão · limites**. A secção
"limites" não é cortesia: é onde se declara o que a medição *não* cobre.

## 3. Invariantes que são testes, não comentários

| invariante | onde é testado | o que falha se quebrar |
|---|---|---|
| `Model-visible ⟺ logged` | `session/tests/invariants.rs` | o prompt deixou de ser reconstruível a partir do log |
| zero `unwrap`/`expect`/`panic` em `src/` | lints de clippy + política | um caminho de erro panica em vez de recusar |
| um único `unsafe` de produção (mais um, de teste, registado) | `check-unsafe` | o ponto de excepção cresceu sem decisão |
| nada fora do projecto excepto a config global | `check-paths` | um ficheiro de log apareceu fora da raiz |
| regra de política sem teste | `check-rule-coverage` | uma regra nova sem rede de segurança |
| superfície ≤ tecto | `check-surface` | a superfície cresceu por acidente |
| links e ADR com alternativas | `check-docs` | a documentação deixou de ser navegável ou uma ADR perdeu o "porque não" |
| prompt ≤ tecto | `gate:prompt` | o custo por turno subiu sem ninguém decidir |

## 4. O ciclo de uma mudança com número

```
1. Pergunta      o que muda, e como se vai saber que mudou
2. Fórmula       o critério é calculável antes de haver número
3. Baseline      mede-se o "antes" com o mesmo método
4. A/B           aplica-se a mudança e mede-se o "depois"
5. Decisão       adopta, rejeita ou deixa pendente — com o número
6. Artefacto     o bruto vai para bench/; o resumo, para published.toml
7. CI            um teste trava o comportamento; um gate trava o número
8. Escrita       a decisão entra no plano, não na conversa
```

O passo 5 é o que distingue o projecto: no ciclo W10, **sete das doze decisões foram rejeições ou
"medido sem mudança"** (DPP, C2, E18-T05, E18-T08, E18-T09, E1, Q-02b/DPP), e cada uma tem fórmula e
número. Um item rejeitado que deixa código é dívida; um item rejeitado que deixa o número é
conhecimento — e o C2 chegou ao ponto de ter a implementação removida de `src/` por essa regra.

## 5. Limites do método

- **Proxies sintéticos.** Vários itens medem a estatística do mecanismo com séries determinísticas
  (SplitMix64) em vez do mundo. É o que torna o CI determinístico, e é também a forma mais fácil de
  enganar a si próprio: o conformal falhou exactamente assim.
- **Máquinas diferentes.** Um número medido numa máquina e publicado sem qualificar é uma infidelidade. A
  correcção adoptada é o `attribution` no artefacto, com a lista explícita do que é comparável.
- **Bases sinuosas.** `inferred` cobre muita coisa, desde uma divisão simples até um contrafactual de
  benchmark. Um dia valeria a pena distinguir `derived` de `modelled`.
- **O ledger mede o que foi medido.** Não mede o que o utilizador sente. A distância entre os dois é
  o risco residual do projecto, e está escrita em [04](04-falsabilidade.md).

Volta ao [00 · tese](00-tese.md).