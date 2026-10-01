# 00 · A tese do katu

> Documento que destrincha a tese. Não é um plano nem um manual: é a **afirmação** que o projecto
> sustenta, escrita de forma a poder ser refutada. Cada afirmação remete para o artefacto que a
> mede ou para o gate que a trava; o que não tem medição está marcado como tal.

## 1. A afirmação

O katu é um **agente de engenharia local cujas acções são decididas por um kernel de política
puro, cuja memória é um store local persistente, e cujo custo é medido antes de ser optimizado**.

Três pilares, nesta ordem de importância:

1. **Guardrails como kernel, não como filtro.** A decisão de permitir ou recusar acontece *dentro*
   do kernel event-sourced, antes de qualquer efecto, e é ela que entra no log. Um filtro aplicado
   depois do efecto é um filtro que pode ser contornado por um caminho novo.
2. **Memória local e persistente como.store de primeira classe.** O agente não "puxa contexto" de
   um serviço remoto: consulta um índice local, versionado, auditável e substituível.
3. **Optimização como prática medida.** Nenhuma melhoria entra sem fórmula, artefacto, teste de
   CI e decisão escrita — incluindo as **rejeições**, que são tão registadas como as adopções.

## 2. Por que esta ordem

A ordem não é preferência estética; é uma consequência de onde está o custo real. Medido no
baseline E18 (`wiki/_ref/docs/../../bench/e18/PROTOCOL.md`), o turno e2e era **360 ms**, dos quais
**72,3 ms (201 ‰) eram do projecto** e o resto era o provider. Ou seja: o projecto nunca controlou
o relógio do utilizador; controla o **que é permitido** e o **quanto se repete**. Optimizar o
caminho do provider seria optimizes de outra coisa.

Daí a ordem dos pilares: primeiro o que é irredutível (autorização), depois o que dá contexto
(memória), e só então o que dá velocidade (performance), porque só depois de 1 e 2 é que se sabe
**que** cervix se deve optimizar.

## 3. As três teses em forma falsificável

| # | Tese | Como se refuta | Estado |
|---|---|---|---|
| T1 | **Nada acontece sem decisão registada.** O texto que o modelo vê é exactamente o texto que o log guarda, e a decisão de permitir precede o efeito. | Um caso em que `derive_messages(log) ≠ prompt enviado`, ou um tool call executado sem `PolicyEvaluated` no log. | **Provada por invariante**: `model_visible_is_exactly_the_logged_messages` + `Session::verify()`; o conteúdo do tool entra no delta em **uma** função (`ToolReport::to_delta`), pelo que log e pedido não podem divergir por construção. |
| T2 | **A memória é local, persistente e auditável.** | Consultar contexto de fora do projecto; ou não conseguir reconstruir quem Recordou o quê, quando. | **Provada**: o adaptador in-process recusa arrancar sem memória saudável; o índice de auditoria é reconstruível; a nota é `append-only`. |
| T3 | **O custo é medido antes de ser optimizado.** | Um número publicado sem artefacto, ou uma melhoria adoptada sem A/B. | **Provada pelo gate**: `check:bench` falha sem base tipada e sem artefacto existente; `unpriced` tem de valer zero. |

Cada tese tem uma secção própria:
[01 · guardrails](01-guardrails.md) · [02 · memória](02-memoria.md) ·
[03 · performance](03-performance.md).

## 4. O que a tese **não** afirma

Escrever a tese inclui escrever o seu perímetro, senão ela vira retórica:

- **Não** afirma que o katu é mais rápido que um agente sem kernel. Medido: 92,6 % do turno é o
  provider. O que se ganhou foi a **fracção controlável** (201 ‰ → 57 ‰) e os bytes para o modelo
  (−37 %).
- **Não** afirma que a memória melhora o resultado do modelo. O `knudge-core` tem utilidade
  demonstrada; a ponte `memória → qualidade da resposta` está por medir, e a fixture de confiança
  é **sintética** por isso declarado.
- **Não** afirma que a contenção é uma fronteira de segurança. É *soft containment*: oADR 0004
  adia o `kill(2)` de grupo e o jail de SO real para [E17](../_ref/plan/18-jail-futuro.md).
  Um agente com acesso a FUSE, `/proc` ou ao utilizador não está contido — está **observado**.
- **Não** afirma cobertura estatística onde não há base. O conformal foi implementado, medido e
  **rejeitado** ([`bench/e18/conformal/`](../../bench/e18/conformal/PROTOCOL.md)): com resíduos
  correlacionados a cobertura cai até 37 ‰ abaixo do nominal.

O perímetro completo, com os números que faltam, está em
[04 · falsabilidade e limites](04-falsabilidade.md).

## 5. A tese em cinco linhas

1. A política é o kernel: avalia antes do efeito, decide com evidência estruturada e escreve no log.
2. A memória é um store local com porta, adaptador substituível e abertura *fail-closed*.
3. O modelo vê exactamente o que o log guarda — nem mais, nem menos.
4. Nenhum número entra no ledger sem base tipada e artefacto que o produziu.
5. O que não foi medido fica escrito como não medido, e o que foi rejeitado deixa o número e não
   deixa código.

## 6. Como ler este directório

- Se quer **o que o projecto garante**: [01](01-guardrails.md), [02](02-memoria.md).
- Se quer **o que o projecto ganhou**: [03](03-performance.md).
- Se quer **onde o projecto pode estar errado**: [04](04-falsabilidade.md).
- Se quer **como o mede**: [05](05-metodo.md).
- Se quer **as decisões concretas**: [`wiki/_ref/adr`](../_ref/adr/README.md).