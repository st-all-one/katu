# ADR 0015 — Loop de turnos e roteador de tool calls

- **Estado:** aceite
- **Data:** 2026-09-30
- **Decisões fundacionais:** DF1 (loop possuído), DF2 (política pura), DF8 (provider commodity),
  DF12 (forma AI-first)
- **Épicos:** E12-T05, E10, E07-T02
- **Relaciona:** [ADR 0011](0011-porta-provider-e-builtin-opencode.md),
  [ADR 0012](0012-catalogo-dialetos-e-providers-declarativos.md), [plan/13](../plan/13-providers.md)

## Contexto

Com a porta `Provider` e os built-in estáveis (ADR 0011/0012), faltava o **elo** entre o provider e
o kernel: o modelo produz `name` + argumentos JSON, mas o kernel avalia um `ToolUse` **tipado** e
executa tools pela ordem §42. Sem esse elo o agente não corria ponta-a-ponta.

## Decisão

1. **O kernel é dono do loop; o provider é stateless.** O histórico enviado ao modelo é a projeção
   do log (`derive_messages`), nunca um `Vec` paralelo — a invariante `Model-visible ⟺ logged`
   (E04) mantém-se por construção.
2. **Ordem §42 inegociável:** `ToolCall` logado **antes** de executar → `evaluate` pura → só então o
   executor corre → `ToolResult` logado. Uma recusa da política nunca tem efeito.
3. **Roteador fail-closed** (`crates/katu/src/agent/router/`): os argumentos JSON viram `ToolUse`
   resolvido + executor. Caminhos são canonicalizados via `Fs` **antes** do veredicto (E07-T02);
   argumento em falta/inválido ou tool fora do catálogo **não** executam nada.
4. **O catálogo ao modelo deriva de `katu_tools::schema::SCHEMAS`** (fonte única já validada por
   `xtask check-schemas`): sem *drift* entre o que o modelo vê e o que o roteador aceita. O nome ao
   modelo (`bash`/`grep`/`find`/`ls`/`memory`) mapeia para o `ToolName` de política (`exec`/
   `search`/…); a tool `memory` passa pelos caminhos de recall/escrita do gate de E05.
5. **Afinidade pela sessão:** o header `x-opencode-session` (e o `prompt_cache_key`) derivam do
   `SessionId` do runtime — o mesmo histórico volta ao mesmo *shard* (F4/E18-T04).
6. **Bound explícito:** o turno tem um teto de passos (`max_steps`); excedê-lo para com
   `TooManySteps` em vez de rodar sem fim.
7. **`plan` fica indisponível (fail-closed recuperável).** O schema ao modelo só tem
   `goal`/`next_action`, mas o executor exige um `ScopeContract` (forbidden/rollback) que deve vir
   de `scope_contract.json` (E09-T04); sintetizá-lo seria inventar política. O roteador devolve
   `Unavailable { control: "scope-contract" }` (logado pela ordem §42, **sem** efeito) até essa
   peça existir, para o modelo se poder corrigir.

## Alternatives considered

1. **Guardar o histórico num `Vec<Message>` do loop.** Rejeitada: duplica a fonte da verdade e
   quebra `Model-visible ⟺ logged` (E04).
2. **Executar a tool e só depois logar.** Rejeitada: viola a ordem §42 e torna uma recusa
   observável em efeito.
3. **Resolver caminhos depois do veredicto.** Rejeitada: a política veria um caminho relativo/symlink
   por resolver (E07-T02); o invariante exige resolução **antes**.
4. **Deixar o provider orquestrar tools.** Rejeitada: o loop é do kernel (DF1) e o provider é
   cliente stateless do plano de dados (DF8).
5. **Sintetizar um `Plan` a partir de `goal`/`next_action`.** Rejeitada: inventaria um contrato de
   escopo; gated em E09-T04.
6. **Reutilizar um roteador do GDK.** Rejeitada com a ADR 0012 (tipos externos, `tokio`); o roteador
   próprio é pequeno e testável.

## Consequências

- **Positivas:** o agente corre ponta-a-ponta (`katu run`), com tool execution pela ordem §42 e
  histórico sempre fiel ao log; o caminho é testável sem rede (`FakeProvider` + `MemFs`).
- **Negativas / dívida:** `plan` fica indisponível até E09-T04; a TUI (E10) e os controlos
  interativos (E07-T05/E09-T03/E09-T07) ainda não existem; o `spawn_blocking`+timeout (E03-T04,
  worker bloqueante) continua gated.
- **Travas:** testes de loop sem rede (`agent::tests`), testes do roteador (resolução,
  fail-closed), `verify()` a seguir ao turno; e2e real verificado contra `llama-server` e
  `opencode-go`.
