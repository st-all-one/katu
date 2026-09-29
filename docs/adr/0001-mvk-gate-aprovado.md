# ADR 0001 — MVK aprovado: o loop possuído (DF1) torna-se compromisso

- **Estado:** aceite
- **Data:** 2026-09-29
- **Decisões fundacionais:** DF1 (loop possuído), DF8 (o provider é endpoint, não agente), DF5
  (evidência viaja com o número), DF10/DF11 (recusa acionável; pré-requisito crítico é muro)
- **Épicos:** E05 (gate), E04 (kernel), E02 (política), E03 (porta `Memory`)

## Contexto

O MVK existe para provar — ou refutar — a tese: as regras do protocolo de memória do knudge são
**fiscalizadas em runtime, pelo caminho real, com recusa acionável**, e o atrito é aceitável. É o
inverso do erro do `maxima`: medir antes de escalar.

Evidência recolhida (E05-T01…T05):

- `crates/katu/tests/mvk.rs` conduz o **loop real** (sessão + gate + política):
  (a) gravar sem recall → `Denied { mem-recall-before-write }` **sem** executor (0 commits);
  (b) duplicata ≥ 0,92 → `Denied { mem-no-duplicate }`; (c) fecho sem `outcome` →
  `Refusal::UnmetPrecondition { Closed }`. Cada cenário foi levado a **vermelho** por regressão
  injetada e revertido (§51.9).
- As **5** regras do protocolo são `Enforced` (nenhuma `Advisory`), expressas como
  `DenyCommand`/`RequireAfter` sobre **factos tipados** (E02-T07).
- Atrito medido com artefacto (`bench/mvk/raw.json`, DF5): `memory.write` p50 ≈ **9,9 µs**;
  `policy.evaluate` p50 ≈ 2,4 µs; o caso negativo **não toca** a porta de memória.

O que **não** foi medido: a comparação direta com `pi + knudge-mcp` (rácio cross-tool), que ficou
em `bench/published.toml` como `mvk.cross_tool.gain_ratio`, base `unpriced` (0).

## Decisão

**O gate E05-T07 passa.** As fases 3+ (Onda 4 em diante) ficam desbloqueadas. **DF1** (loop
possuído) deixa de ser hipótese e passa a **compromisso**: o katu possui o loop e trata o modelo
como endpoint (DF8).

A comparação cross-tool permanece **não medida** (`unpriced`) e **não** bloqueia o gate, por decisão
explícita do dono do projeto; fica como trabalho de E15/E18 se e quando houver um número.

## Alternatives considered

1. **Parar (K2) por falta do contrafactual cross-tool.** Rejeitada pelo dono: a evidência
   disponível — enforcement no caminho real, sensibilidade à regressão, atrito na ordem dos
   microssegundos — sustenta a tese sem o rácio. O plano nunca exigiu o contrafactual como
   condição *sine qua non*; transformá-lo em pré-requisito seria mover a fasquia.
2. **Pivot para o knudge (K1).** Rejeitada: **todas** as regras do protocolo são expressáveis
   (`Enforced`), pelo que K1 não dispara.
3. **Manter o gate aberto até medir o cross-tool.** Rejeitada: bloquearia valor já provado por uma
   medição que depende de um harness externo. A honestidade preserva-se registando `unpriced` em
   vez de inventar um ganho (DF5, §62).
4. **Escalar sem registar a decisão.** Rejeitada: a DoD de E05 exige ADR com alternativas; sem
   registo, a decisão re-litiga-se e perde-se a linhagem.

## Consequências

- **Positivas:** Onda 4 (E06 tools/capacidades, E07 contenção soft) desbloqueada; DF1 comprometido;
  o enforcement fica provado por testes sensíveis à regressão.
- **Negativas / dívida:** o rácio cross-tool permanece desconhecido; a linha vermelha fica visível
  em `bench/published.toml` (`mvk.cross_tool.gain_ratio`, `unpriced`).
- **Travas:** `make check` (inclui `xtask gate:bench`) e o job `msrv` continuam obrigatórios;
  nenhuma regra nova entra sem um teste que a inverta.
