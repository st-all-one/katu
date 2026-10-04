# PI_GAINS — plano de implementação: melhorias do loop derivadas do pi

> **Plano de melhorias do loop do katu**, derivado do dossiê do `pi`
> ([`PI_VS_KATU_LOOP.md`](../brainstorm/pi-rs/PI_VS_KATU_LOOP.md),
> [`PI_LOOP.md`](../brainstorm/pi-rs/PI_LOOP.md)) e regido pelo método do
> [`OPTIMIZATION_PLAN.md`](OPTIMIZATION_PLAN.md) §0 e pelos princípios do
> [`.agents/skill/rust/`](../../../.agents/skill/rust/SKILL.md).
>
> **Tese.** O `pi` tem, no controlo do turno, **poucos** mecanismos que o katu ainda não tem —
> `terminate` por resultado de tool, modelo por passo, streaming do output das tools e uma decisão
> de fim que a superfície pode pedir. Todos cabem no kernel de **uma thread**, no log
> **append-only** como fonte da verdade e no protocolo **in-process** (ADR 0027). O que **não**
> cabe — RPC CBOR, *lease* de sessão, hooks que reescrevem resultados, árvore de sessão — fica
> rejeitado ou deferido, com a razão escrita.
>
> **Evidência de partida:** [`PI_VS_KATU_LOOP.md`](../brainstorm/pi-rs/PI_VS_KATU_LOOP.md) §3
> (diferenças D1–D7) e §4 (candidatos priorizados).

---

> **Estado (execução).** **Q1, Q2, P1 e S1 implementados** e verdes (`cargo xtask check`); **S2**
> deferido. Testes: `agent::tests::terminate` (Q1), `agent::tests::step_model` (Q2),
> `exec::tests::streaming_delivers_tool_output_chunks` + `ports::process::tests` (P1),
> `agent::tests::continue_step` (S1).
>
> **Nota honesta de Q2.** O **mecanismo** (resolver do modelo por passo) está implementado e
> testado, mas a política do kernel (derivar da **fase**) é hoje **inerte**: `runtime.phase()` é
> constante dentro de um turno porque nenhum `Event::PhaseTransition` é emitido em `crates/*/src/`
> (a máquina de fases existe mas não avança). O critério de adoção `cost_ratio < 1` fica por medir;
> o mecanismo reativa-se quando a máquina de fases passar a avançar (sem tocar no `drive`).
>
> Cada item só entra com teste que o trava e `cargo xtask check` verde.

## 0. Hierarquia e método

**Hierarquia (em caso de conflito, o de cima vence):**

1. **Q — qualidade da execução do modelo** (dados · superfície · transparência · estabilidade).
2. **P — performance/UX** (latência percebida, output visível), sempre com artefacto.
3. **S — simplificação/contrato** (menos superfície, um facto um lar).

**Método (herdado do E18 §0.3):** cada item exige **fórmula + artefacto + teste que o trava**;
**adotar-ou-reverter**. As correcções de invariante não têm reversão.

**Restrições inflexíveis:** zero `unwrap/expect/panic` em `src/`; ficheiros `src/` ≤ 400 linhas
(`file-length`); `cargo xtask check` + job `msrv` (1.97.0) verdes; ids de `diag` no catálogo;
`Model-visible ⟺ logged` (E04); ordem §42 e fail-closed intactos; superfície só cresce subindo o
tecto em PR.

**Lição de método (do próprio pi).** O pi tem **duas** camadas de loop (`Agent` e `Harness`) e paga
a paridade; o katu tem **um** `drive` e mantém-no. Nada aqui duplica caminhos: cada mecanismo entra
no `run_turn_with` existente.

---

## 1. Forma estável de estar (invariantes)

| # | Invariante | Onde é imposta | Como é travada |
|---|---|---|---|
| **PG-I1** | Nenhuma melhoria introduz estado model-visible não logado | `kernel/session/mod.rs` | `Model-visible ⟺ logged` (existe) + replay |
| **PG-I2** | O kernel continua **uma** thread e **um** dono do `Runtime`/`Provider` | `kernel.rs::run` | teste de composição (existe) |
| **PG-I3** | §42 intacto: nenhuma tool corre sem `ToolCall` logado | `kernel/session/mod.rs::tool_call` | `Session::verify` |
| **PG-I4** | A superfície continua **cliente** (não acede ao `Runtime`) | `api/handle.rs` | o handle não expõe o runtime (existe) |
| **PG-I5** | O fluxo de tool continua **efémero** (nunca entra no log/contexto) | `agent/turn.rs::Activity` | teste de `LIVE_FLOW` (existe) |
| **PG-I6** | `cargo xtask check` verde (replay, guard, política, superfície) | CI | gate |

---

## 2. Diagnóstico (as diferenças, com evidência)

| # | Diferença | Evidência (pi) | Evidência (katu) |
|---|---|---|---|
| D1 | A superfície não decide "continua uma vez mais" | `finishTurn → {end\|continue}` (`agent-loop.ts:264`) | `Termination` fixo (`agent/mod.rs:114`) |
| D2 | Sem troca de modelo/thinking a meio do run | `prepareNextTurn` (`agent-loop.ts:179`) | `TurnOptions.model` fixado no `submit` (`kernel.rs:256`) |
| D3 | Sem `terminate` por resultado de tool | `shouldTerminateToolBatch` (`agent-loop.ts:687`) | `run_calls` executa tudo (`agent/turn.rs:139`) |
| D4 | Sem streaming do output das tools | `tool_execution_update` (`agent-loop.ts:783`) | `Activity::Tool`/`ToolDone` (`agent/turn.rs:30`) |
| D5 | Sessão linear vs árvore | `Entry.parentId` (`session/types.ts:18`) | log append-only (`kernel/session/`) |
| D6 | Durabilidade mais fina | `driveOperation` (`drive.ts:29`) | log + `reconcile_pending` |
| D7 | Protocolo in-process vs RPC de serviço | `protocol.ts` (`PROTOCOL_VERSION = 8`) | `Command`/`Event` (ADR 0027) |

---

## 3. Eixo Q — qualidade (prioridade 1)

### Q1 · `terminate` por resultado de tool

- **Problema (D3).** `run_calls` (`agent/turn.rs:139`) executa todas as calls do passo; não há sinal
  de "parar aqui" vindo da tool. Um verbo terminal (ex.: `finish`/`submit_plan`) não consegue cortar
  o turno sem um passo extra ao modelo.
- **Mecanismo pi.** `AgentToolResult.terminate` (`types.ts:420`); o batch só termina cedo se
  **todas** as calls finalizadas tiverem `terminate` (`agent-loop.ts:687`).
- **Mudança katu.**
  1. `ToolOutput` (`kernel/pipeline/mod.rs:19`) ganha `terminate: bool` (por omissão `false`); a
     tool que quer terminar preenche-o.
  2. `Dispatch` expõe `terminate()` (como `delta()`, `:113`); `CallOutcome`
     (`agent/mod.rs:236`) ganha `terminate: bool`.
  3. `run_calls` devolve um resultado de três estados em vez de `bool` — ex.
     `CallsOutcome { Continue, Terminate, Cancelled }` — onde `Terminate` exige que **todas** as
     calls do passo o pediram (regra do pi).
  4. `drive` (`run.rs:88`) mapeia `Terminate` para uma nova variante
     `Termination::Terminal` (fim **normal**: `round_exit() = 0`, `as_str() = "terminal"`).
- **Fórmula.** `steps_saved = nº de passos que o modelo **não** teve de dar por a tool terminar`
  (contado no log). Adoptar se `steps_saved > 0` num cenário real; senão manter o mecanismo
  inerte (sem consumidor) e registar.
- **Teste.** (a) passo com **todas** as calls `terminate` → o turno fecha após as calls, sem novo
  pedido ao provider; (b) passo **misto** → continua (espelha `shouldTerminateToolBatch`); (c) o
  `TurnEnd` é logado e `verify()` fica verde.
- **Adoção.** Correção de mecanismo (não de invariante): adoptar com um consumidor real; reverter se
  ficar inerte.
- **Mapa.** PG-I3 · `agent/turn.rs::run_calls` · `agent/turn/run.rs::drive` · `agent/mod.rs`
  (`Termination`) · `PI_VS_KATU_LOOP` D3/Q1.

### Q2 · Modelo e pensamento por passo

- **Problema (D2).** `TurnOptions.model` é fixado no `submit` (`kernel.rs:256-273`); a
  `TierPolicy::model_for` (`kernel.rs:258`) só corre no início do turno. Planear com um modelo
  barato e executar com outro exige **dois** `Submit`.
- **Mecanismo pi.** `prepareNextTurn` (`agent-loop.ts:179`) substitui `model`/`thinkingLevel` entre
  turnos **dentro do mesmo run**; `prepareRequest` (`:210`) antes de cada pedido.
- **Mudança katu.**
  1. `drive` (`run.rs:88`) deixa de usar `options.model` fixo: em cada passo, se o utilizador **não**
     fixou modelo (`turn_model == None`), deriva o modelo da **fase** corrente via
     `TierPolicy::model_for` (o modelo explícito continua a vencer — E12-T10).
  2. A fase é **já logada** (`Event::PhaseTransition`) e o controlo é logado (`Event::Control`,
     E12-T10), pelo que a derivação é determinística e reproduzível: `Model-visible ⟺ logged`
     mantém-se **sem** evento novo. Se a fase não mudar no turno, o comportamento é idêntico ao de
     hoje.
  3. `TurnSummary`/envelope passa a reportar o **último** modelo usado (ou a lista, se mudou).
- **Fórmula.** `cost_ratio = custo(turno com rota por fase) / custo(turno com modelo único)` a
  partir do `usage` no log; adoptar se `cost_ratio < 1` sem regressão de `steps`/qualidade.
- **Teste.** (a) turno que atravessa `PhaseTransition` → o pedido do passo seguinte usa o modelo do
  tier da nova fase; (b) replay reproduz a mesma sequência de modelos; (c) modelo explícito
  (`--model`) nunca é sobreposto.
- **Adoção.** `cost_ratio < 1` sem piorar o resultado; senão reverter e escrever o número.
- **Mapa.** PG-I1 · `agent/turn/run.rs::drive` · `agent/turn/request.rs::build_request` ·
  `tier.rs::TierPolicy` · `PI_VS_KATU_LOOP` D2/Q2.

---

## 4. Eixo P — UX/latência

### P1 · Streaming do output das tools

- **Problema (D4).** Um `bash` longo mostra os **argumentos** (`Activity::Tool`, `agent/turn.rs:30`)
  e só o **resumo** no fim (`Activity::ToolDone`); parece parado. O `drain_stream` já recebe o
  output, mas a porta `Process::run` (`ports/process.rs:58`) devolve o `ExecResult` inteiro.
- **Mecanismo pi.** `onUpdate` da tool vira `tool_execution_update` (`agent-loop.ts:783`); o `bash`
  emite *checkpoints* a cada 2 s (`harness/tools/bash.ts`).
- **Mudança katu.**
  1. `Process` ganha um método de streaming — ex.
     `run_with_output(&self, request, cancel, on_chunk: &mut dyn FnMut(&str))` — com implementação
     por omissão que delega em `run` (retrocompatível com `MemProcess` e as *fakes*).
  2. `Ports` carrega um observador opcional de output (um `&dyn FnMut(&str)`), passado a
     `ExecTool` (`katu-tools/src/exec.rs`).
  3. `Activity` ganha `ToolOutput { name, chunk }`; o `BusSink` (`kernel/sink.rs`) mapeia-o para
     `Live`; o CLI/TUI mostram as últimas N linhas (como o texto em `LIVE_FLOW`).
  4. **Efémero** (PG-I5): nunca entra no log, na transcrição nem no contexto; o teto de linhas é
     aplicado no painel (como `STREAM_TAIL_BYTES`).
- **Fórmula.** `time_to_first_output = Δ até ao primeiro `ToolOutput`` (alvo: < 2 s para um `bash`
  de 30 s); artefacto `bench/e18/tool-stream/raw.json`.
- **Teste.** (a) uma `Process` *fake* que emite chunks → `Activity::ToolOutput` na ordem; (b) o
  log e o contexto **não** mudam (comparação byte-a-byte); (c) o `MemProcess`/as *fakes* continuam
  a compilar sem implementar o método.
- **Adoção.** Reduz o tempo até ao primeiro output sem alterar o log; senão reverter.
- **Mapa.** PG-I5 · `ports/process.rs` · `katu-tools/src/exec.rs` · `agent/turn.rs` (`Activity`) ·
  `kernel/sink.rs` · `PI_VS_KATU_LOOP` D4/P1.

---

## 5. Eixo S — contrato/simplificação

### S1 · `Command::Continue` (decisão de fim da superfície)

- **Problema (D1).** `run_turn_with` fecha sempre no fim natural; a superfície não pode pedir
  "continua uma vez mais" (ex.: após uma compactação) sem um novo `Submit`.
- **Mecanismo pi.** `finishTurn → {action:"continue"}` garante **uma** requisição extra
  (`agent-loop.ts:271`).
- **Mudança katu.**
  1. `KernelBus` ganha uma flag partilhada `continue_once` (mesmo padrão do `cancel_flag`, K8);
     `Command::Continue` (`api/command.rs`) arma-a.
  2. `ActivitySink` ganha `continue_once(&mut self) -> bool`; `drive` consulta-o **antes** de
     fechar naturalmente; se `true`, faz **um** passo extra (a flag é consumida).
  3. O protocolo (`api/`) documenta que `Continue` só tem efeito durante um turno; fora dele é
     no-op (como `Steer`).
- **Fórmula.** `extra_requests = nº de passos extra pedidos pela superfície` (contado no log);
  adoptar se resolver um caso real (ex.: pós-compactação) sem loops (a flag é one-shot).
- **Teste.** (a) `Continue` durante um turno natural → um passo extra e depois fecha; (b) `Continue`
  fora do turno → no-op; (c) `Continue` duas vezes → no máximo um passo extra por armamento.
- **Adoção.** Só se um caso real o exigir; é mudança de contrato — avaliar antes. O `Termination`
  actual cobre a maioria dos fins.
- **Mapa.** PG-I1 · `api/command.rs` · `api/handle.rs` · `kernel.rs::run` · `agent/turn.rs`
  (`ActivitySink`) · `PI_VS_KATU_LOOP` D1/S1.

### S2 · Árvore de sessão (branching) — deferido

- **Problema (D5).** O log é linear; não se experimentam dois caminhos a partir do mesmo ponto.
- **Decisão.** **Deferido** (não rejeitado). O `trash`/`restore` cobre o "voltar atrás" simples; o
  *branching* exigiria `parent`/`branch` no log e um `BranchSummaryEntry`, com impacto no replay e
  na projecção. Fica registado como decisão consciente, reavaliável se aparecer um caso de uso
  (ex.: A/B de estratégias).
- **Mapa.** `PI_VS_KATU_LOOP` D5/S2.

---

## 6. Ordem de implementação (fases)

| Fase | Itens | Gate de saída |
|---|---|---|
| **PG1 — Q** | Q1 (`terminate`) + Q2 (modelo por passo) | testes dos itens verdes; replay e `Model-visible ⟺ logged` intactos; `check` verde |
| **PG2 — P** | P1 (streaming do output das tools) | `bench/e18/tool-stream/` com número; log inalterado |
| **PG3 — S** | S1 (`Command::Continue`) | teste de um passo extra; contrato documentado; `check` verde |
| — | S2 (branching) | deferido (decisão registada) |

Cada fase é um commit próprio, com `cargo fmt --all` antes de `check`. Nenhuma fase sobe o tecto de
superfície sem o PR que o justifica (`surface.toml`).

---

## 7. O que **não** copiar do pi (reafirmado)

- **RPC CBOR + servidor + *lease* de sessão** — sem segundo front-end (ADR 0027; ver
  [`PI_VS_KATU_LOOP`](../brainstorm/pi-rs/PI_VS_KATU_LOOP.md) D7). Manter os tipos `serde`-prontos.
- **Hooks arbitrários que reescrevem o resultado** (`beforeToolCall`/`afterToolCall`) — a política
  do katu é pura e fail-closed; reescrever o `ToolResult` quebraria §42 e o `Model-visible ⟺ logged`.
- **Duas camadas de loop** (`Agent` + `Harness`) — o katu mantém **um** `drive`.
- **`AgentMessage` extensível por declaration merging** — em Rust, enum fechado com `Custom`.
- **Máquina de estados durável de 2 000 linhas** — o katu tem o log como fonte; só se o modo
  durável/remoto se justificar.

---

## 8. Referências

**Plano:** [`OPTIMIZATION_PLAN.md`](OPTIMIZATION_PLAN.md) §0 (método) e §0bis (estado por item) ·
[`KERNEL_SURFACE.md`](KERNEL_SURFACE.md) (kernel de uma thread, superfícies como clientes) ·
[`LIVE_FLOW.md`](LIVE_FLOW.md) (fluxo efémero) · [`LOOP_RESILIENCE.md`](LOOP_RESILIENCE.md)
(invariantes I1–I8).

**pi:** [`PI_VS_KATU_LOOP.md`](../brainstorm/pi-rs/PI_VS_KATU_LOOP.md) §3 (D1–D7) e §4 (candidatos) ·
[`PI_LOOP.md`](../brainstorm/pi-rs/PI_LOOP.md) §3 (loop), §4 (turnos), §6 (tools), §9 (protocolo).

**katu:** `agent/turn/run.rs` (`:88` `drive`, `:356` `finish`), `agent/turn.rs` (`:139` `run_calls`,
`:30` `Activity`), `agent/mod.rs` (`:61` `TurnOptions`, `:114` `Termination`, `:236` `CallOutcome`),
`kernel.rs` (`:67` `run`, `:247` `submit`), `kernel/pipeline/mod.rs` (`:19` `ToolOutput`, `:72`
`Dispatch`), `katu-core/src/ports/process.rs` (`:58` `Process`), `tier.rs` (`TierPolicy`),
`katu-core/src/api/{command,event}.rs`.
