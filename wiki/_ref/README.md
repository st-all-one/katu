# `wiki/_ref` — material de referência

> Tudo o que o projecto **decidiu, planeou ou registou**, fora do caminho do dia-a-dia. Um facto, um
> lar: aqui vivem as ADRs, os planos, o brainstorm e a documentação derivada. O que o agente usa no
> arranque está em [`agent-rules`](docs/agent-rules.md); o que é tese está em
> [`wiki/proposition`](../proposition).

| Directório | O que é | Quem lê |
|---|---|---|
| [`adr/`](adr/README.md) | decisões registadas (26 ADRs, com *alternativas considered*) | quem implementa uma mudança |
| [`plan/`](plan/README.md) | épicos E01–E20, objectivos, gates de aceitação | quem planeia |
| [`plan/OPTIMIZATION_PLAN.md`](plan/OPTIMIZATION_PLAN.md) | programa de optimização Q/P/S, com fórmulas e decisões | quem mexe em performance |
| [`plan/IMPLEMENTATION_PLAN.md`](plan/IMPLEMENTATION_PLAN.md) | ordem de implementação e ondas | quem pega num épico |
| [`plan/SURFACE_IMPLEMENTATION.md`](plan/SURFACE_IMPLEMENTATION.md) | reforma da superfície CLI/TUI (E20) | quem toca no CLI |
| [`plan/KERNEL_SURFACE.md`](plan/KERNEL_SURFACE.md) | kernel autocontido na sua thread; superfícies como clientes | quem mexe no loop ou num front-end |
| [`plan/LIVE_FLOW.md`](plan/LIVE_FLOW.md) | fluxo do turno (raciocínio, tools, execução) visível no CLI/TUI | quem mexe na apresentação do turno |
| [`brainstorm/`](brainstorm/katu-brainstorm-decisoes.md) | a origem das ideias: Alternative A, o *deepseek harness*, o PTC | quem procura o porque |
| [`brainstorm/PROVIDER_WIRE.md`](brainstorm/PROVIDER_WIRE.md) | o wire dos providers (goose/pi/katu) e o 400 do passo com N tool calls | quem mexe no encoder |
| [`docs/`](docs/) | regras, catálogo gerado, superfície do utilizador, tópicos, postmortems | o agente e quem investiga |

**Regra:** material movido para cá não volta ao topo do repositório. A raiz é o que se usa; isto é o
que se consulta.