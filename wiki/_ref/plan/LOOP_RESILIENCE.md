# LOOP_RESILIENCE — plano de implementação: interrupção, fim de turno e sessão

> **Plano de resiliência do loop do katu**, derivado da comparação com o goose
> ([`GOOSE_VS_KATU_LOOP.md`](../brainstorm/goose-rs/GOOSE_VS_KATU_LOOP.md),
> [`GOOSE_LOOP.md`](../brainstorm/goose-rs/GOOSE_LOOP.md)) e regido pelo método do
> [`OPTIMIZATION_PLAN.md`](OPTIMIZATION_PLAN.md) §0 e pelos princípios do
> [`.agents/skill/rust/`](../../../.agents/skill/rust/SKILL.md).
>
> **Tese.** O loop do katu tem de ser **interrompível em qualquer instante, sempre com o log
> fechado e o utilizador nunca sem resposta**. O que falta não é arquitectura (o kernel está
> correcto) — é o **controlo do turno**: um modelo de execução cancelável e uma política de
> terminação que ensina. Este plano **não** abre superfície nova (G7); reduz turnos falhados,
> interrupções penduradas e estados de log inconsistentes.
>
> **Evidência de partida:** [`GOOSE_VS_KATU_LOOP.md`](../brainstorm/goose-rs/GOOSE_VS_KATU_LOOP.md) §3
> (lacunas L1–L7) e o código citado item a item.

---

> **Estado (execução).** **WL1** (L-Q1 · L-Q5 · L-Q6), **WL2** (L-Q2 · L-Q3 · L-Q4) e **WL3**
> (L-P1 · L-P2 · L-P3) estão implementados e travados por teste; **WL4** fecha L-S1 e rejeita L-S2/L-S3
> com a razão escrita. `cargo xtask check` verde. O estado por item vive em
> [`OPTIMIZATION_PLAN`](OPTIMIZATION_PLAN.md) §0bis (o seu lar). Os artefactos `bench/e18/cancel/`
> (e o `gate:cancel`) ficam por medir: nenhum número sem base.
>
> **Sucessão.** A rejeição de L-S2 (worker) foi **substituída** por [`KERNEL_SURFACE`](KERNEL_SURFACE.md):
> a separação kernel/superfície resolve os pontos que ficaram abertos (cancelamento dentro de tools,
> fuga de I/O, visibilidade, truncagem) sem duplicar motores.

## 0. Hierarquia e método

**Hierarquia (em caso de conflito, o de cima vence):**

1. **Q — qualidade da execução do modelo** (dados · superfície · transparência · segurança ·
   guardrails · **estabilidade**). Um turno que fecha limpo vale mais que qualquer micro-ganho.
2. **P — performance bruta** (latência, threads, CPU, alocação), sempre com artefacto.
3. **S — simplificação** (menos código, menos superfície, um facto um lar).

**Método (herdado do E18 §0.3):** cada item exige **fórmula + artefacto + teste que o trava**;
**A/B** com recorte cru; **adotar-ou-reverter** (≥ 20 % no alvo **ou** remoção de complexidade;
senão reverter e escrever a rejeição). As **invariantes** (L-Q1/L-Q3) não têm reversão: são
correcções. Determinismo: sem RNG, ordem canónica, `total_cmp`.

**Restrições inflexíveis:** zero `unwrap/expect/panic` em `src/`; um ponto de `unsafe` (ADR 0018);
ficheiros `src/` ≤ 400 linhas (`file-length`); `make check` + job `msrv` (1.97.0) verdes; ids de
`diag` no catálogo; `Model-visible ⟺ logged` (E04); superfície só cresce subindo o teto em PR.

**Lição de método (do próprio goose).** O goose tem **dois** motores de loop e paga a paridade; o
katu tem **um** e mantém-no. Nada aqui duplica caminhos: a resiliência entra no `run_turn_with`
existente, com degraus explícitos.

---

## 1. Forma estável de estar (o contrato do loop)

> **Definição.** O loop está **estável** quando, em repouso (nenhum turno a correr), o log, o
> estado e a projecção satisfazem as invariantes abaixo — **e** quando qualquer interrupção leva a
> esse repouso em tempo limitado, sem deixar o processo, a thread ou o log pendurados.

### 1.1 Invariantes (impostas e travadas por teste)

| # | Invariante | Onde é imposta | Como é travada |
|---|---|---|---|
| **I1** | Todo `TurnStart` tem `TurnEnd`; em repouso `turn_open == false` | `runtime::record_turn_end`, `assemble` | teste de turno fechado em erro/cancel/teto (já existe para o teto) |
| **I2** | Nenhum `ToolCall` sem `ToolResult` no log | fecho do turno (novo `L-Q1`) | `Session::verify` + varredura `orphan.tool_calls == 0` |
| **I3** | O pedido ao modelo é `derive_messages(log)` + `system` reconstruível | `turn/request.rs::build_request` | teste `Model-visible ⟺ logged` (existe) + projecção sem órfãos |
| **I4** | Cancelar produz `cancelled = true`, texto parcial logado, turno fechado | `run_turn_with` | teste de cancelamento sem deltas e a meio de tool |
| **I5** | `Esc`/`Ctrl-C` são vistos em ≤ `T_cancel` (alvo **250 ms**) mesmo sem deltas | `katu-tui` + worker | `bench/e18/cancel/` + `gate:cancel` |
| **I6** | O turno termina por fim natural, `StopReason`, teto/loop conversacional ou cancelamento — nunca em silêncio | `drive` (novo `L-Q2`) | testes dos 4 finais |
| **I7** | Determinismo: mesma entrada + mesmo log ⇒ mesmo estado | kernel | `replay` byte-a-byte (existe) |
| **I8** | §42 e fail-closed intactos; nenhum byte model-visible muda sem artefacto | `session::tool_call` | `make check` |

### 1.2 Ciclo de vida do turno (estados e transições)

```
                         submit
   ┌──────┐ ──────────────────────────────► ┌─────────┐
   │ Idle │                                  │ Running │◄──────────────┐
   └──────┘ ◄──────────── TurnEnd ─────────  └────┬────┘               │
      ▲                                           │                    │
      │                    sem tool calls / StopReason terminal         │
      │                                           ▼                    │
      │                                      ┌─────────┐   approval   ┌─┴──────────────┐
      │                                      │ Closing │◄─────────────│ AwaitingApproval│
      │                                      └────┬────┘              └────────────────┘
      │                          Esc / Ctrl-C      │                    │ Esc / Ctrl-C
      │                                           ▼                    ▼
      │                                      ┌────────────┐  settle pending  ┌──────────┐
      └──────────────────────────────────────│ Cancelling │─────────────────►│ Closing  │
                                             └────────────┘                  └──────────┘

   kill (SIGKILL) em qualquer estado ─► recuperação na retomada: fecha turno + reconcilia (L-Q1)
```

**Garantia de progresso:** de **qualquer** estado há caminho para `Idle`; `Closing` fecha sempre o
log; `Cancelling` liquida as calls pendentes antes de `Closing`.

### 1.3 Modelo de execução alvo

| Peça | Hoje | Alvo |
|---|---|---|
| Thread do turno | a da UI (`handler.handle`) | **worker** de escopo (`std::thread::scope`), UI livre |
| Observação | `Painter` directo (só em deltas) | **canal** `Activity` + `Painter` na UI |
| Cancelamento | só em deltas | `Arc<AtomicBool>` verificado em cada fronteira e no transporte |
| Teclas | só `Esc` | `Esc` e `Ctrl-C` |
| Corpo do provider | sem teto | **idle timeout** curto (L-P1) |
| Fecho | erro pode deixar pendente | reconciliação (L-Q1) |

---

## 2. Diagnóstico (as lacunas, com evidência)

| # | Lacuna | Evidência |
|---|---|---|
| D1 | UI bloqueia no turno; input só em deltas | `tui/handler.rs:202`; `katu-tui/src/run.rs:57,89,248` |
| D2 | `Ctrl-C` engolido durante o turno | `run.rs:113` (`on_key` só `Esc`) |
| D3 | Corpo HTTP sem teto | `katu-providers/src/http.rs:80` (`timeout_recv_body(None)`) |
| D4 | `ToolCall` órfão permitido no `TurnEnd` | `kernel/step/mod.rs:256` (não exige `pending` vazio) |
| D5 | `StopReason` descartado | `agent/turn/run.rs:247` (`stream_step` só usa `usage`) |
| D6 | Tool call truncada aborta | `openai/decode.rs:139` (`ProviderError::Decode`) |
| D7 | Resposta vazia = `Update::Error` seco | `tui/handler.rs:249` |
| D8 | Tecto/loop = erro fatal | `agent/mod.rs` (`TooManySteps`→`internal`, `LoopDetected`→`conflict`) |
| D9 | Sem lock de turno por sessão | ausente (goose: `try_start_turn`) |
| D10 | Sem deteção de eco TOON/JSON | `declared.rs` só parseia JSON |

---

## 3. Eixo Q — estabilidade e terminação (prioridade 1)

### Q-A. Invariantes de fecho

#### L-Q1 · Reconciliação de calls pendentes no fecho e na retomada
- **Problema (D4).** `turn_end` não exige `pending` vazio; o caminho de erro/cancel/kill deixa
  `ToolCall` sem `ToolResult`, e o próximo pedido ao modelo leva uma conversa malformada.
- **Proposta.** No fecho do turno (`record_turn_end` no `run_turn_with` e no `assemble` ao fechar
  um turno aberto), para cada call em `state.pending`, aplicar
  `Event::ToolResult { outcome: Unavailable { control: "interrupted" }, delta: Some(texto) }` com
  um `delta` que ensina (`tool_content` reutilizado). Alternativa estrita avaliada: `turn_end`
  **recusa** com `pending` não vazio — rejeitada por partir a retomada após kill (o goose escolhe a
  reconciliação em `cancellation_response`).
- **Medida/artefacto.** `orphan.tool_calls` = nº de `ToolCall` sem `ToolResult` após kill simulado;
  artefacto `bench/e18/orphan/raw.json` (esperado **0**).
- **Teste.** (a) log com `ToolCall` pendente → `Session::resume` → `verify()` verde e projeção
  sem órfãos; (b) cancelamento a meio de um lote `Shared` → todas as calls cometidas; (c) o
  `TurnEnd` normal não acrescenta eventos.
- **Adoção.** Correcção de invariante — sem critério de reversão.
- **Mapa.** I2/I3 · `kernel/session/mod.rs::record_turn_end` · `runtime.rs::assemble` ·
  `GOOSE_VS_KATU_LOOP` L2.

#### L-Q2 · `StopReason` no controlo do turno
- **Problema (D5/D6).** O loop ignora `ProviderOutcome.stop`; uma tool call truncada
  (`finish_reason == "length"`) faz `flush_tools` devolver `ProviderError::Decode` e o turno aborta,
  em vez de o modelo se corrigir.
- **Proposta.**
  1. Propagar `stop` de `stream_step` para `Step`/`Accum` (`katu-core/src/provider.rs:170`).
  2. No decoder (`openai/decode.rs:139`): em `Length`, emitir a tool call com um marcador de erro
     (novo `ProviderEvent` ou `ToolCall` com `arguments` sentinela) que o loop converte em
     `ToolResult { Unavailable { control: "length" } }` — **sem** executar.
  3. `Length` + texto → mensagem visível a dizer que a resposta foi truncada e sugerir
     `--max-tokens`; `ContentFilter` → mensagem explícita.
- **Fórmula.** `aborted_by_bad_json = 0`; `length.corrected = nº de turnos em que o modelo reformula
  após o erro de truncagem` (contado no log).
- **Medida/artefacto.** `bench/e18/length/raw.json`: `aborted_by_bad_json`, `corrected`,
  `length_turns`; antes/depois.
- **Teste.** Fixture de tool call truncada → o turno **não** aborta, o modelo recebe o erro;
  `Length` sem calls → aviso; bytes do caso normal inalterados.
- **Adoção.** ≥ 20 % dos turnos falhados por JSON truncado evitados **ou** correcção de invariante
  (I6); senão reverter e escrever o número.
- **Mapa.** I6 · `GOOSE_VS_KATU_LOOP` L3/L4 · goose `formats/openai.rs:1234`.

### Q-B. Terminação que ensina

#### L-Q3 · Fim de turno conversacional (vazio, teto, loop)
- **Problema (D7/D8).** Resposta vazia/só raciocínio vira `Update::Error` seco; `TooManySteps` e
  `LoopDetected` são erros fatais (exit 70 / conflict) que o utilizador lê como falha.
- **Proposta.**
  1. **Vazio:** detetar (sem texto, sem calls, sem cancelamento) e fazer **retry limitado**
     (`MAX_EMPTY_RETRIES = 2`) com nudge `agent-only` ("devolve a resposta final em texto"); no
     limite, **mensagem do assistente visível** que explica (orçamento de saída vs. raciocínio) e
     sugere `--max-tokens`/modelo.
  2. **Teto (`TooManySteps`):** fechar o turno com `TurnEnd` e uma mensagem do assistente
     ("atingi o limite de N passos; queres que continue?") — o erro fica **só** no envelope de
     máquina. Espelha o `MAX_TURNS_MESSAGE` do goose.
  3. **Loop (`LoopDetected`):** mesma forma — mensagem que **nomeia** o que se repetiu (já há
     `alarm.reason()`), turno fechado, erro no envelope.
- **Fórmula.** `abnormal_ends_with_visible_message / abnormal_ends` = **1,0** (alvo).
- **Medida/artefacto.** `bench/e18/ends/raw.json` por classe (`empty`, `max_steps`, `loop`):
  `visible_message` (0/1), `turns_before`/`turns_after`.
- **Teste.** Três testes de turno: vazio → retry + mensagem; teto → mensagem + `TurnEnd`; loop →
  mensagem + `TurnEnd`; o envelope de máquina continua a distinguir o erro.
- **Adoção.** 100 % dos finais anormais com mensagem visível; sem esconder o erro no envelope.
- **Mapa.** I1/I6 · `agent/mod.rs` (variantes) · `tui/handler.rs:249` · goose `MAX_TURNS_MESSAGE`.

### Q-C. Robustez do I/O

#### L-Q4 · Deteção de eco (TOON/JSON cru)
- **Problema (D10).** Um modelo fraco devolve o `ToolResult.delta` como resposta final; o
  `declared.rs` só parseia JSON, não TOON.
- **Proposta.** Comparação **determinística** entre o texto final do passo e os `delta` recentes do
  turno (normalização: trim + colapso de whitespace; igualdade ou prefixo longo). Em caso de eco:
  nudge `agent-only` + retry (1×); nunca aceitar o eco como resposta. Não substitui `declared`.
- **Fórmula.** `echo.detected`, `echo.false_positive` (resposta legítima que menciona o conteúdo).
- **Medida/artefacto.** `bench/e18/echo/raw.json` com um corpus de respostas legítimas (controlo
  negativo) e ecos sintéticos.
- **Teste.** Eco → nudge; resposta que cita um excerto → **não** dispara (controlo negativo = 0).
- **Adoção.** ≥ 20 % dos ecos evitados **com** falsos positivos = 0; senão reverter.
- **Mapa.** D10 · `agent/turn/declared.rs` · goose `fix_conversation`.

#### L-Q5 · Projeção normalizada para o modelo
- **Problema.** Mesmo com L-Q1, uma projeção antiga pode conter um par desalinhado; o goose
  normaliza com 9 passos antes de enviar.
- **Proposta.** Uma passagem pura em `derive_messages` (ou imediatamente antes do provider) que
  (a) descarta `ToolCall` sem `ToolResult` e vice-versa, (b) garante que a lista começa em `User`.
  Mínimo necessário — **não** copiar os 9 passos.
- **Teste.** Projeção nunca tem órfão; começa em `User`; bytes do caso normal inalterados.
- **Adoção.** Correcção de invariante (I3).
- **Mapa.** I3 · `kernel/project.rs` · goose `conversation.rs:273`.

### Q-D. Concorrência de sessão

#### L-Q6 · Lock de turno por sessão
- **Problema (D9).** Dois `katu run --resume` sobre a mesma sessão escrevem no mesmo log em paralelo.
- **Proposta.** Um lock em `.katu/` (ficheiro com PID + `turn_open`), ou reutilizar `turn_open`:
  `begin_turn` recusa se já houver turno aberto **noutro processo**. Recusa com erro claro
  (categoria `conflict`) que diz qual o PID/idade.
- **Teste.** Dois processos/turnos concorrentes → o segundo é recusado; o lock é libertado no fim
  (mesmo em erro).
- **Adoção.** Correcção de invariante — sem reversão.
- **Mapa.** I1 · `runtime.rs::begin_turn` · goose `try_start_turn`.

---

## 4. Eixo P — latência de interrupção (prioridade 2)

#### L-P1 · Cancelamento de primeira classe (worker + canal + flag)
- **Problema (D1/D2).** O turno corre na thread da UI; o input só é sondado em deltas; `Ctrl-C` é
  engolido; um `bash` longo não é interrompível.
- **Proposta.**
  1. `run_turn_with` corre num **worker** de escopo (`std::thread::scope`, o padrão de
     `batch.rs::in_parallel`); o `ActivitySink` passa a enviar `Activity` por um **canal**.
  2. A UI drena o canal e chama `painter.live(...)` (que sonda input) — a UI nunca bloqueia.
  3. `Arc<AtomicBool>` de cancelamento partilhado; verificado em cada fronteira (passo, call) e no
     `ChunkSink`; `Esc` **e** `Ctrl-C` põem-no a `true` (segundo `Ctrl-C` sai).
  4. `run_calls` verifica o flag **dentro** de cada tool (ver L-P3).
- **Fórmula.** `T_cancel` = tempo entre a tecla e o `TurnEnd` no log:
  `T_cancel.p95 ≤ 250 ms` **com** e **sem** deltas; `T_cancel.sem_deltas` é o número crítico.
- **Medida/artefacto.** `bench/e18/cancel/` (`PROTOCOL.md`, `raw.json`): `cancel.p95_ms`,
  `cancel.no_delta.p95_ms`, `turn.thread_overhead_us`; `gate:cancel` compara o **limite superior**.
- **Teste.** Cancelamento com stream parado (mock sem bytes) fecha em ≤ alvo; a UI processa input
  durante o turno; sem órfãos (I2); determinismo do cenário.
- **Adoção.** `cancel.no_delta.p95 ≤ 250 ms` **e** `orphan.tool_calls == 0`; senão reverter.
- **Custos.** 1 thread + 1 canal por turno (medido em `turn.thread_overhead_us`); canal *bounded*
  com descarte coalescido do painel (o painel é efémero).
- **Mapa.** I4/I5 · `katu-tui/src/run.rs` · `tui/handler.rs:202` · goose §7.

#### L-P2 · Teto de inactividade no corpo do provider
- **Problema (D3).** `timeout_recv_body(None)`: um stream parado bloqueia para sempre.
- **Proposta.** `timeout_recv_body(Some(idle))` configurável (default conservador); um `Timeout`
  **sem** cancelamento é *stall* recuperável — retry antes do 1.º delta (já existe a política) ou
  erro claro. *Spike obrigatório:* confirmar a semântica do `ureq` (o timeout aborta o pedido?).
- **Fórmula.** `stall.max_block_ms ≤ idle × 2` (com cancelamento a interromper em ≤ `idle`).
- **Medida/artefacto.** `bench/e18/stall/raw.json`: `max_block_ms`, `ttft` normal (sem regressão).
- **Teste.** Mock que nunca envia bytes → o loop acorda e cancela/erra em ≤ `idle`; TTFT normal
  inalterado dentro do orçamento do `gate:provider`.
- **Adoção.** Bloqueio limitado **sem** regressão de TTFT; senão reverter.
- **Mapa.** D3 · `katu-providers/src/http.rs:80`.

#### L-P3 · Cancelamento dentro das tools longas
- **Problema.** `run_calls` (`agent/turn.rs:139`) só verifica entre calls; `bash` longo ignora.
- **Proposta.** Passar o `Arc<AtomicBool>` às portas; `Process` termina o filho ao cancelar
  (kill + `wait`); a flag é lida no caminho de execução.
- **Fórmula.** `T_cancel_tool.p95 ≤ 250 ms` para um `bash` de 30 s.
- **Medida/artefacto.** `bench/e18/cancel/` (coluna `tool.p95_ms`).
- **Teste.** `bash` longo cancelado termina o filho (sem zombie) e fecha o turno.
- **Adoção.** Mesmo alvo do L-P1.
- **Mapa.** I4 · `ports/process.rs` · goose §7.

---

## 5. Eixo S — simplificação (prioridade 3)

- **L-S1 · Um só ponto de fecho.** `record_turn_end` passa a ser o **único** caminho que fecha o
  turno (normal, erro, cancel, teto), incluindo a reconciliação (L-Q1). Elimina a duplicação
  `finish`/erro em `run_turn_with`.
- **L-S2 · Um só `ActivitySink`.** Com o canal (L-P1), o `Painter` deixa de implementar o trait
  directamente; a UI e o `katu run` consomem o mesmo canal (o `NoActivity` desaparece).
- **L-S3 · Reutilizar `turn_open`.** O lock de sessão (L-Q6) reutiliza o estado já logado em vez de
  introduzir um ficheiro de lock paralelo (só se a semântica entre processos o permitir).

---

## 6. Sequência e dependências

```
WL1 (invariantes)  L-Q1 → L-Q5 → L-Q6      (correcções; não esperam medição)
WL2 (terminação)   L-Q2 → L-Q3 → L-Q4      (fim de turno; precisa de L-Q1 para o fecho)
WL3 (interrupção)  L-P2 (spike) → L-P1 → L-P3   (o worker depende do idle timeout)
WL4 (simplificação)L-S1..L-S3              (absorve o que WL1–WL3 tornou redundante)
```

**Regra:** WL1 primeiro (fecha o log). Só depois o worker (WL3), que reescreve o caminho quente.
Cada PR: A/B (onde há alvo) + `make check` verde + `msrv`.

**Dependências externas:** nenhuma. O plano não muda o contrato do provider, a política, o log nem
a superfície de tools.

---

## 7. Métricas de sucesso (alvos, não promessas)

| Métrica | Hoje | Alvo | Artefacto |
|---|---|---|---|
| `T_cancel` sem deltas (p95) | não medido (ilimitado) | **≤ 250 ms** | `bench/e18/cancel/raw.json` |
| `T_cancel` a meio de tool (p95) | ilimitado | **≤ 250 ms** | idem |
| `orphan.tool_calls` após kill | ≥ 1 (possível) | **0** | `bench/e18/orphan/raw.json` |
| Turnos falhados por JSON truncado | > 0 | **0** | `bench/e18/length/raw.json` |
| Finais anormais com mensagem visível | parcial | **100 %** | `bench/e18/ends/raw.json` |
| Ecos de TOON/JSON aceites | > 0 | **0** | `bench/e18/echo/raw.json` |
| `turn.thread_overhead_us` | — | medido, dentro do overhead de 40 ms | `bench/e18/cancel/raw.json` |
| Overhead fora do provider | 72,3 ms | **≤ 40 ms** (não regride) | `bench/e18/raw.json` |

---

## 8. Não-objetivos

- Copiar o **loop monolítico** ou os **dois motores** do goose (o katu tem um só).
- Copiar `StateMachine<'a>` com operações que emprestam o agente.
- Substituir o log event-sourced por SQLite/timestamps.
- Auto-escalonamento de modelo/pensamento (DF8).
- Mover o loop para `async`/Tokio: o plano mantém o modelo síncrono com **uma** thread de turno.

---

## 9. Riscos

| Risco | Mitigação |
|---|---|
| Worker thread introduz *race* no `Runtime` | escopo (`thread::scope`); o worker é o **único** dono de `&mut Runtime`; a UI só lê o canal |
| Canal cresce sem limite | *bounded* + descarte coalescido (o painel é efémero) |
| Idle timeout corta streams lentos legítimos | default conservador + retry antes do 1.º delta + A/B de TTFT |
| Reconciliação altera bytes observáveis | só em turnos anormais; teste de bytes no caso normal |
| Mensagem conversacional esconde falhas | o erro mantém-se no envelope de máquina; o teste verifica ambos |
| Deteção de eco com falsos positivos | controlo negativo obrigatório no corpus; reverter se > 0 |

---

## 10. Definition of Done

- [ ] **I1–I8** travadas por teste; `orphan.tool_calls == 0` após kill simulado.
- [ ] L-Q1..L-Q6 com teste e (onde há alvo) artefacto; rejeições escritas com número.
- [ ] L-P1..L-P3 com `bench/e18/cancel/` e `gate:cancel` (limite superior ≤ 250 ms).
- [ ] `make check` e `msrv` (1.97.0) verdes em cada PR; `file-length` verde.
- [ ] Nenhum byte model-visible alterado fora do que foi medido; `Model-visible ⟺ logged` intacto.
- [ ] `OPTIMIZATION_PLAN.md` §0bis ganha uma linha por item feito (o lar do estado é lá).
- [ ] Este plano mantido como lar da resiliência do loop; `GOOSE_VS_KATU_LOOP` liga para cá.

---

## 11. Mapa sintoma → item

| Sintoma | Item |
|---|---|
| "`Esc` não faz nada" / "UI congela" | L-P1, L-P2 |
| "`Ctrl-C` não cancela" | L-P1 |
| "`bash` longo não para" | L-P3 |
| "depois de interromper, o próximo turno dá erro" | L-Q1, L-Q5 |
| "o modelo devolveu TOON/JSON cru" | L-Q2, L-Q4 |
| "não devolveu texto" | L-Q3 |
| "turno excedeu N passos" | L-Q3 |
| "duas execuções na mesma sessão" | L-Q6 |
