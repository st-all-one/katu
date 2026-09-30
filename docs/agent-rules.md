# Regras do agente

Índice das regras que governam o comportamento do agente. Cada regra vive no seu **lar**
autoritativo (plano, ADR ou `MODULE.md`); este ficheiro só encaminha (E14-T03).

## Por tema

- [Falha fechada](topics/fail-closed.md) — o que não for permitido, nega-se.
- [Instrumentação](topics/instrumentation.md) — um facto, um id.
- [Desempenho](topics/performance.md) — orçamentos e portões medidos.
- [Memória e `unsafe`](topics/memory-and-unsafe.md) — `forbid(unsafe_code)`, tipos proibidos.

## Contratos

- Política: [`../plan/03-contrato-de-politica.md`](../plan/03-contrato-de-politica.md).
- Porta Memory: [`../plan/04-contrato-da-porta-memory.md`](../plan/04-contrato-da-porta-memory.md).
- Kernel/loop: [`../plan/05-kernel-loop.md`](../plan/05-kernel-loop.md).
- Providers: [`../plan/13-providers.md`](../plan/13-providers.md).
- Testes e qualidade: [`../plan/14-testes-e-qualidade.md`](../plan/14-testes-e-qualidade.md).
- Governança de superfície: [`../plan/15-governanca-superficie.md`](../plan/15-governanca-superficie.md).

## Decisões

- [`adr/README.md`](adr/README.md) — decisões registadas com alternativas.
