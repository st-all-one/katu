# 0026 — Modo `batch` declarativo: manter o loop nativo (B-08)

- **Estado:** aceite
- **Épico:** W9-5 / B-08 (Anexo B)
- **Supersedes:** —
- **Relação:** materializa B-08 do Anexo B do `OPTIMIZATION_PLAN.md`; estende o lote concorrente
  B-01/B-02 (ADR 0015) e o contrato do PTC (Anexo B §B.1)

## Contexto

O PTC mode (Anexo B §B.1) permite que o modelo escreva um **programa** TypeScript contra um SDK
gerado a partir do registo de tools, em vez de emitir uma tool call por passo. O programa corre num
runtime isolado e chama N tools por dentro; só o que o programa **imprime ou devolve** volta ao
contexto. Os ganhos alegados: menos *round-trips*, menos tokens, composição (loop/branch/fan-out)
e paralelismo (`Promise.all` sobre chamadas *concurrency-safe*).

O katu já absorveu o **contrato** que interessa: a apresentação é ortogonal à autoridade, e ambas
passam pelo mesmo pipeline. O lote B-01/B-02 (`agent/turn/batch.rs`) executa tool calls `Shared` em
paralelo com pool limitado, preservando `Model-visible ⟺ logged` e a ordem de commit do modelo
(**+76,3 %** no alvo, `bench/e18/batch/`). O que falta é a **composição declarativa**: o modelo
declarar uma sequência de trabalho como dados, não como uma série de passos imperativos.

A questão: **compensa capturar ~80 % do ganho do PTC com um modo `batch` declarativo (não
Turing-completo) — passos sequenciais/paralelos, filtros, condicional simples — sem motor JS?**

## Decisão

**Manter o loop nativo (B-01/B-02). Não adotar o modo `batch` declarativo por agora.**

A decisão é conservadora porque o ganho é **alegado, não medido** e o custo é **arquitetural**:

1. **O ganho de ~80 % é uma claim do harness de referência**, não uma medição no katu. O Anexo B
   §B.3 regista que a própria nota do PTC admite "não há garantia incondicional de poupança" e que
   o *measured guidance* fica pós-ship. Sem uma medição no katu, adotar o modo declarativo seria
   otimizar às cegas — exatamente o que o método do §0 proíbe.
2. **O custo é um novo DSL + um novo executor + um proptest de determinismo.** O modo declarativo
   precisa de: uma gramática (passos, filtros, condicionais), um executor (que corre os passos
   respeitando a classificação de concorrência e a ordem de commit), e um proptest que prove que a
   execução é determinística. É uma capacidade nova, não uma otimização.
3. **O loop nativo já cobre o caso comum.** O lote B-01/B-02 executa tool calls `Shared` em paralelo
   com pool limitado; o modelo declara cada passo como uma tool call. O que o modo declarativo
   adiciona é a **composição** (loop/branch/fan-out sobre resultados) — que é exatamente a parte
   que o modelo já faz bem com tool calls nativas.
4. **As restrições são estritas.** Zero-dep (G7), determinismo (G3), superfície fechada (G3). Um
   novo DSL é uma superfície nova; um novo executor é um novo caminho de execução. Ambos têm de
   provar determinismo e não podem adicionar dependências.

**Condições para revisitar** (o ADR não se edita; abre-se uma nova ADR se estas se cumprirem):

1. **Mensurável:** uma medição no katu (com o harness de W7) mostra ≥ 20 % de *round-trips* ou
   tokens poupados num cenário canónico, em comparação com o loop nativo.
2. **Determinístico:** um proptest prova que a execução do modo declarativo é determinística
   (mesma entrada → mesma saída, sem RNG, sem relógio).
3. **Não Turing-completo:** o DSL não tem loops nem recursão (só passos sequenciais/paralelos,
   filtros e condicional simples).
4. **Sem dep nova:** o executor usa as portas existentes (`Fs`/`Process`/`Env`/`Clock`/`Memory`)
   e o pipeline de tools existente.

## Alternatives considered

1. **Adotar o modo `batch` declarativo agora.** Rejeitada: o ganho é alegado, não medido; o custo
   é um novo DSL + executor + proptest; e o loop nativo já cobre o caso comum. Adotar sem medição
   seria otimizar às cegas.
2. **Adotar um subconjunto (só passos sequenciais, sem filtros nem condicionais).** Rejeitada
   como substituto: sem composição, o ganho é marginal (o lote B-01/B-02 já faz paralelo); com
   composição, já é o modo declarativo completo. Não há meio-termo que valha o custo.
3. **Adotar o PTC completo (motor JS).** Rejeitada: contradiz o binário único, zero-dep e G7
   (registado no Anexo B §B.4). O modo declarativo é a alternativa "sem motor JS" — mas só se as
   condições se cumprirem.
4. **Manter o loop nativo e registar a dívida.** Aceite: o loop nativo é a base estável; a dívida
   (a composição declarativa) fica registada com as condições para a revisitar. O ADR 0015
   (loop de turnos) e o lote B-01/B-02 mantêm-se como a execução de referência.

## Consequências

- **Positivas:** a decisão é honesta sobre o que é medido e o que é alegado; o loop nativo mantém-se
  estável; as condições para revisitar são explícitas e verificáveis.
- **Negativas / dívida:** o ganho de ~80 % do PTC fica por capturar; o modelo continua a precisar
  de declarar cada passo como uma tool call (mais *round-trips* e tokens do que o modo declarativo
  promete). A dívida é registada no Anexo B e nas condições deste ADR.
- **Travas:** o lote B-01/B-02 continua a ser a execução de referência (`bench/e18/batch/`); o
  proptest de determinismo é uma condição para a revisita, não uma trava atual.
