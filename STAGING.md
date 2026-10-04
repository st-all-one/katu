# STAGING — descobertas recentes e próximos passos

> Documento de **staging**: resumo volátil do que se descobriu e do que falta. Não é o lar de
> nenhum facto — cada item aponta para o seu lar (`wiki/_ref/...`). Quando um item fecha, sai daqui.
> Última atualização: **2026-10-04**. Gate: `cargo xtask check` **verde**.

## Feito (com prova)

### 1. Wire dos providers — um passo do assistente é **uma** mensagem
- **Sintoma:** `katu run` falhava com `HTTP 400 Upstream request failed: [invalid_request_error]`
  em passos com ≥2 tool calls. Mensagem real do upstream: *«An assistant message with `tool_calls`
  must be followed by tool messages responding to each `tool_call_id`»*.
- **Causa:** o encoder `chat/completions` emitia **um `assistant` por `ToolCall`** (a projeção
  mantém um `ToolCall` por evento, o que é correto no log). Com 2 calls no passo, o 1.º `assistant`
  ficava sem os `tool` correspondentes antes do 2.º.
- **Prova de terreno:** sonda direta ao gateway — forma dividida → **400**; forma agrupada
  (`asst(texto, tool_calls=[c1,c2]); tool; tool`) → **OK**.
- **Fix:** `katu_core::kernel::project::wire_messages` agrupa cada passo numa `WireMessage::Assistant`;
  o encoder OpenAI consome-a. Verificado end-to-end: passos com 3/4/2 calls, `termination=natural`.
- **Lar:** [`PROVIDER_WIRE`](wiki/_ref/brainstorm/PROVIDER_WIRE.md) ·
  [`OPTIMIZATION_PLAN`](wiki/_ref/plan/OPTIMIZATION_PLAN.md) §0bis (**PX-WIRE**).

### 2. A instrução corrente nunca sai do contexto
- **Sintoma:** o modelo respondia «Sem tarefa no pedido» a meio do turno.
- **Causa:** a política `suffix` descartava a **mensagem do utilizador** (a unidade mais antiga)
  quando os resultados das tools enchiam `raw_min`.
- **Fix:** `context::pin_unit` — a última `Message::User` é a única unidade que a montagem nunca
  deixa cair (reserva o custo, sem exceder `raw_min`).
- **Lar:** [`OPTIMIZATION_PLAN`](wiki/_ref/plan/OPTIMIZATION_PLAN.md) §0bis (**PX-PIN**) ·
  `crates/katu-core/src/context/select.rs`.

### 3. Teto cru do contexto configurável
- **Sintoma:** com a tarefa sempre visível, o modelo passou a **relê os mesmos ficheiros em ciclo**
  (`ARCHITECTURE.md` 21×, 145 calls, `termination=loop` no passo 52).
- **Causa:** thrashing de contexto — `raw_min = 4096` tokens (~15 KB) é pequeno demais para
  acumular investigação; com `auto_compact=false` o prefixo era descartado sem resumo. Medido: a
  tarefa só fecha com `input ≈ 39–48 k` tokens. **Ligar a compactação sozinha não resolve.**
- **Fix:** `DEFAULT_CONTEXT_BUDGET.raw_min` **4096 → 65 536** + chave `behavior.context_budget`.
  Verificado: `termination=natural`, `teste.md` criado (9 317 B).
- **Lar:** [`OPTIMIZATION_PLAN`](wiki/_ref/plan/OPTIMIZATION_PLAN.md) §0bis (**PX-BUDGET**) ·
  [`CLI_TUI_SURFACE`](wiki/_ref/docs/CLI_TUI_SURFACE.md).

## Próximos passos (pendente)

| # | Item | Porquê / critério |
|---|------|-------------------|
| **P1** | Agrupar o passo nos encoders **Anthropic** e **Google** | Mesma classe do PX-WIRE: ainda emitem um `assistant`/`model` por call e um `user` por tool result. Anthropic exige alternância de papéis e **um** `user` com todos os `tool_result`; Google idem para `functionResponse`. `wire_messages` já existe; falta ligá-la e agrupar os tool results por dialeto. **Responses** está OK. |
| **P2** | **Q2** (modelo por passo) está **inerte** | O mecanismo (`StepModel`/`PhaseModel`) está testado, mas nenhum `Event::PhaseTransition` é emitido em `src/`, logo a política por fase é um no-op. Reativa-se quando a máquina de fases avançar. Lar: [`PI_GAINS`](wiki/_ref/plan/PI_GAINS.md) §3. |
| **P3** | **Q1** (`terminate`) e **S1** (`Command::Continue`) sem consumidor na superfície | Mecanismos e testes existem, mas nenhuma tool de produção pede `terminate` e nenhum atalho da TUI envia `Continue`. Lar: [`PI_GAINS`](wiki/_ref/plan/PI_GAINS.md) §3/§5. |
| **P4** | `katu config list` não mostra defaults | A chave `behavior.context_budget` só aparece se estiver no ficheiro; o default (65 536) não é listado. Confirmar se é desejável expor defaults efetivos. |
| **P5** | `text`-stream duplicado | Em `--output text` o texto do assistente aparece ao vivo no `stderr` **e** repetido no resultado do `stdout`. Lar: [`LIVE_FLOW`](wiki/_ref/plan/LIVE_FLOW.md). |
| **P6** | Validar ao vivo `responses`/`messages`/`google` | Só o caminho OpenAI foi exercitado contra um provider real. |
| **P7** | **S2** (árvore de sessão) | Deferido: o log é linear e `trash`/`restore` cobre o simples. Lar: [`PI_GAINS`](wiki/_ref/plan/PI_GAINS.md) §5. |
| **P8** | **F4** (`katu serve`) | Rejeitado por [`ADR 0027`](wiki/_ref/adr/0027-fronteiras-de-transporte.md); reabre só com um 2.º front-end real. |

## Notas

- O binário corrigido está instalado em `~/.local/bin/katu` (via `make install`).
- Os testes de regressão dos PX: `openai::tests::a_step_with_two_calls_is_one_assistant_message`,
  `context::tests::instruction::the_instruction_survives_a_tool_flood`.
