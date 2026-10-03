# KERNEL_SURFACE — kernel autocontido e separação das superfícies

> **Plano de refatoração estrutural.** O `katu` é um **kernel agêntico autocontido**: um backend
> determinístico que vive na **sua própria thread** e expõe **um protocolo de comandos/eventos**.
> O CLI, o TUI e qualquer front-end futuro são **clientes** — nunca partilham a thread do kernel,
> nunca possuem o `Runtime`, nunca falam com o provider. Derivado de
> [`GOOSE_VS_KATU_LOOP`](../brainstorm/goose-rs/GOOSE_VS_KATU_LOOP.md) §4 (interrupção de 1.ª
> classe) e [`GOOSE_LOOP`](../brainstorm/goose-rs/GOOSE_LOOP.md) §10.9 (interface = stream de
> eventos), regido pelo método do [`OPTIMIZATION_PLAN`](OPTIMIZATION_PLAN.md) §0 e pelos princípios
> do [`.agents/skill/rust/`](../../../.agents/skill/rust/SKILL.md).
>
> **Relação com o [`LOOP_RESILIENCE`](LOOP_RESILIENCE.md).** Aquele plano fechou as invariantes de
> fecho (WL1) e a terminação (WL2). Este plano **substitui a decisão de L-S2** (que rejeitou o
> worker) por uma fronteira explícita de kernel e fecha os pontos de falha que ficaram abertos
> (cancelamento dentro de tools, fuga de I/O, visibilidade dupla, truncagem).

---

## 0. O princípio (inflexível)

> **O kernel vive sozinho.** Uma thread, um dono do `Runtime`, um escritor do log. As superfícies de
> interação **enviam comandos** e **consomem eventos**; não chamam o kernel, não o bloqueiam, não o
> contaminam com terminal, rato ou render.

Três consequências que governam todo o plano:

1. **Determinismo do kernel.** O loop do kernel permanece **single-threaded** e event-sourced; a
   fronteira é um canal, não paralelismo no kernel. (O paralelismo do lote `Shared` continua interno
   e com escopo, como hoje.)
2. **A superfície nunca espera pelo kernel.** O front-end tem o seu próprio loop de eventos e só
   drena o canal do kernel. Um `Esc` a meio de um `bash` de 30 s é visto pelo front-end **nesse
   instante**, mesmo que o kernel esteja ocupado.
3. **A fronteira é serializável.** O protocolo não transporta `Runtime`, `Session` nem `Provider`;
   transporta dados. Assim a thread pode tornar-se **processo** (`katu serve`) sem reescrever o
   protocolo.

---

## 1. Estado actual vs. alvo (evidência)

| Peça | Hoje | Alvo |
|---|---|---|
| Dono do `Runtime` | `AgentHandler` (TUI) e `command::run` (CLI), na thread da superfície | `Kernel`, na thread do kernel |
| Contrato | `Command`/`Update` **síncronos** em `katu-tui::message`, com `Handler::handle` a devolver `Vec<Update>` | `Command`/`Event` **assíncronos** num crate de protocolo |
| Thread do turno | a da UI (`katu-tui::run` → `handler.handle` → `run_turn_with`) | thread do kernel |
| Cancelamento | flag partilhada, mas a UI não a escreve durante tools (não sonda input) | `Command::Cancel` + flag, escrito pela superfície, lido pelo kernel |
| Aprovação | `painter.challenge` na thread da UI, dentro do turno | `Event::ApprovalRequest` / `Command::Approval` (request/response) |
| I/O do provider | thread destacada dentro do turno (vaza no *stall*) | dentro do kernel, com teto total do corpo (nunca vaza) |
| Visibilidade | log não distingue nudge `agent-only` de mensagem do utilizador | `Visibility::{User, Agent}` |
| `katu-tui` | importa `katu_tui` **e** corre o kernel | só protocolo + `App`/render |

**Prova de viabilidade (Send).** `crates/katu/src/agent/turn/batch.rs::in_parallel` já empresta
`&Runtime` a `std::thread::scope`, logo o `Runtime` (e `Session`) são `Send + Sync`. Mover o
`Runtime` para a thread do kernel **não** exige reescrita do kernel.

---

## 2. A fronteira — protocolo `Command`/`Event`

Crate novo `katu-api` (ou módulo `katu-core::api`; decidir em F0 pelo teto de crates): **sem**
`ratatui`, `crossterm` nem `clap`; tipos `Clone + Debug + PartialEq` e `serde` (a serialização é o
que permite o processo futuro).

### 2.1 `Command` (superfície → kernel)

| Comando | Efeito | Aceite com kernel ocupado? |
|---|---|---|
| `Submit { goal, options }` | abre turno, corre o loop | não (recusa `Busy`) |
| `Cancel` | pede cancelamento cooperativo | **sim** |
| `Approval { signature }` | responde a um `ApprovalRequest` pendente | **sim** |
| `SetModel` / `SetThinking` | muda o controlo do **próximo** turno | não |
| `Compact` / `Verify` / `Plan` / `Skill` / `Shell` | operações de kernel não-turno | não |
| `Trash` / `Restore` / `EmptyTrash` / `Transcript` | leitura/escrita de sessão | não |
| `Login` | reconstroi o provider (comando de composição) | não |
| `Shutdown` | fecha a thread do kernel | **sim** |

### 2.2 `Event` (kernel → superfície)

`Activity(Activity)` (efémero, fora do log), `Assistant`, `Tool`, `Info`, `Error`, `Phase`,
`NextAction`, `Usage`, `Models`, `ThinkingOptions`, `Trash`, `Transcript`, `Cancelled`, `Plan`,
`Done`, e os dois novos: `ApprovalRequest { prompt }` e `Busy { command }`.

**Invariante de terminação:** todo `Command` que produz trabalho emite **pelo menos** um evento
terminal (`Done`, `Error` ou `Cancelled`) — a superfície nunca fica pendurada.

### 2.3 `KernelHandle`

```
send(&self, Command) -> Result<(), SendError>   // fila limitada; backpressure explícita
recv(&self, timeout) -> Result<Event, RecvError> // ou EventStream
flag(&self) -> Flag                              // partilhada, para o transporte/tools
join(self) -> Result<(), KernelError>            // fecho determinístico
```

O `Flag` continua a ser o primitivo do [`LOOP_RESILIENCE`](LOOP_RESILIENCE.md) L-P1; a diferença é
que **quem o escreve é a superfície** (o `on_key` do TUI, o handler de sinais do CLI), não o loop do
turno.

---

## 3. O actor do kernel (thread única)

`Kernel` possui `Runtime<'a>`, `Provider`, `Ports`, `Clock` e os canais. Corre **um** loop:

```
loop {
    let command = rx.recv()?;                 // thread do kernel bloqueia aqui, não a superfície
    match command {
        Submit { .. } => run_turn(...),       // run_turn_with com ActivitySink -> canal de eventos
        Cancel        => flag.request(),      // lido no passo, entre calls, na tool e no transporte
        Approval {..} => resolve o canal do pedido pendente,
        Shutdown      => break,
        other         => handle_non_turn(other)?,
    }
}
```

- `ActivitySink` deixa de ser o `Painter`: passa a ser um `ChannelSink` que envia `Event::Activity`.
- A aprovação é um **request/response**: o kernel envia `ApprovalRequest` e bloqueia no canal do
  pedido; a superfície desenha e responde com `Approval`. Sem resposta (canal fechado/cancelado) →
  fail-closed (`None`), como hoje.
- Enquanto um turno corre, o kernel está "ocupado": `Cancel` e `Approval` são processados **durante**
  o turno (o loop do turno sonda um canal de controlo), os restantes comandos são recusados com
  `Busy` (ou enfileirados; decidir em F1).

---

## 4. Invariantes da separação (travadas por teste)

| # | Invariante | Como é travada |
|---|---|---|
| **K1** | O kernel corre numa **única** thread e é o único dono de `Runtime`/`Session`/`Provider` | teste de composição: o handle não expõe o runtime; `join` devolve-o |
| **K2** | As superfícies não nomeiam `Runtime`/`Session`/`Provider` | `katu-tui` e `cli` sem esses imports (gate de dependências) |
| **K3** | `katu-api` não depende de UI (`ratatui`/`crossterm`/`clap`) | `cargo tree` no `check`/`layers.toml` |
| **K4** | Toda a interacção é `Command`/`Event`; cada comando de trabalho tem evento terminal | teste de protocolo |
| **K5** | A thread do kernel **nunca** bloqueia indefinidamente (teto de corpo + idle por passo) | teste de *stall* com provider mudo |
| **K6** | O log tem **um** escritor (a thread do kernel) | `Session` deixa de ser acessível fora do kernel |
| **K7** | O protocolo é serializável (sem empréstimos nem tipos de kernel) | `serde` round-trip no teste |
| **K8** | `Cancel` é honrado em cada fronteira: passo, entre calls, **dentro** da tool, no transporte | teste de `bash` de 30 s cancelado ≤ 250 ms |

---

## 5. Fechar as lacunas observadas (dentro do kernel)

| # | Lacuna (do goose) | Correção neste plano |
|---|---|---|
| **G1** | Cancelamento dentro de tools morto (a UI não sonda input durante a tool) | F1/F2: a superfície sonda input e escreve a flag; o kernel corre a tool. L-P3 passa a funcionar ponta-a-ponta |
| **G2** | Thread de I/O vaza no *stall* (`timeout_recv_body(None)`) | F3: teto **total** do corpo (`timeout_recv_body(Some(budget))`) + *idle* por passo; a thread do kernel fecha sempre |
| **G3** | Nudges `agent-only` aparecem na transcrição como `**utilizador**` | F3: `Visibility::{User, Agent}` no evento de mensagem; a projecção do modelo inclui `Agent`, a transcrição/UI salta-a |
| **G4** | Tool call truncada mas **parseável** é executada em `length` | F3: em `StopReason::Length`, **todas** as calls pendentes viram `Unavailable{length}` (não só as inparseáveis) |
| **G5** | Eco ainda aceite após 1 retry | F3: no orçamento de retries, o eco é **substituído** por nota visível, nunca aceite |
| **G6** | `normalize` não garante adjacência `ToolCall`/`ToolResult` nem fim válido | F3: subconjunto mínimo do `fix_messages` do goose (adjacência, vazios, `fix_lead_trail`) |
| **G7** | Rato/resize/cópia descartados durante o turno | F1/F2: a superfície fica livre; deixa de haver `Ok(_) => {}` a engolir eventos |
| **G8** | Thread destacada pode concorrer com o provider no turno seguinte | F1: um só dono (`K1`/`K6`) — sem thread de I/O destacada a sobreviver ao passo |

---

## 6. Fases

```
F0 (protocolo) → F1 (actor do kernel) → F2 (re-ligar CLI/TUI) → F3 (lacunas goose) → F4 (processo)
```

### F0 — Congelar a fronteira
- Criar o protocolo (`Command`/`Event`/`KernelHandle`/`Flag`) com `serde` e testes de round-trip.
- Mover `Command`/`Update` de `katu-tui::message` para o protocolo; `katu-tui` passa a depender dele.
- **Sem mudança de comportamento.** Gate: `check` verde; `katu-api` sem deps de UI.

### F1 — Actor do kernel
- Extrair `Kernel` de `AgentHandler`: dono do `Runtime`/`Provider`; `run_turn_with` com
  `ChannelSink`; aprovação request/response; `Busy`/`Cancel`/`Shutdown`.
- `AgentHandler` passa a `KernelClient` (só envia/consome).
- Testes: `Submit → eventos`, `Cancel` sem deltas, aprovação ida-e-volta, `Busy`, `join` devolve o
  runtime e o log fecha limpo.

### F2 — Re-ligar as superfícies
- `katu run`: cria o handle, envia `Submit`, bloqueia no evento terminal (o CLI é naturalmente
  sequencial; o kernel é a thread).
- `katu tui`: o loop principal drena o canal do kernel **enquanto** processa teclado/rato/resize e
  redesenha; `handler.handle` deixa de bloquear.
- Remover qualquer acesso directo a `Runtime` dos front-ends (K2).

### F3 — Lacunas goose (G2–G6)
- Teto do corpo; `Visibility`; `length` para todas as calls; eco nunca aceite; `normalize` reforçado.
- Cada uma com teste e, onde há alvo, artefacto.

### F4 — Fronteira de processo (opcional, se o roteiro pedir)
- `katu serve`: o mesmo protocolo sobre Unix socket/stdio; ACP/outros front-ends.
- Só se justifica com mais de um cliente ou isolamento de falhas; o protocolo já está preparado.

---

## 7. Testes e gates

- **Interrupção:** `bash` de 30 s cancelado ≤ 250 ms **a partir da superfície** (não só da porta);
  stream mudo cancelado sem deltas; `orphan.tool_calls == 0` após cancel/kill.
- **Superfície livre:** durante o turno, rato/resize/cópia e *steering* continuam a funcionar
  (teste do loop da UI com kernel em thread).
- **Aprovação:** ida-e-volta; fail-closed sem resposta; cancelamento durante o challenge.
- **Protocolo:** round-trip `serde`; cada `Command` de trabalho tem evento terminal.
- **Determinismo:** o log continua byte-a-byte igual (o kernel é o mesmo; só muda quem o possui).
- **Gates:** `check` verde; `layers.toml` proíbe `katu-api → katu-tui`; `file-length`; diag ≥ 90 %.
- **Bench:** `bench/e18/cancel/` + `gate:cancel` (limite superior ≤ 250 ms), agora com a superfície
  viva; `turn.thread_overhead_us` medido.

---

## 8. Riscos e mitigações

| Risco | Mitigação |
|---|---|
| `Runtime` não é `Send` | já provado `Send + Sync` por `batch::in_parallel` (escopo) |
| Aprovação complica o protocolo | request/response explícito, fail-closed, testado; é o único ponto novo não trivial |
| Corrida no log (dois escritores) | K6: `Session` só dentro do kernel; o handle não a expõe |
| Latência da fronteira | canal *bounded*; o hot path (stream → tool) corre no kernel sem render |
| Regressão de determinismo | o kernel mantém-se single-threaded; o canal preserva ordem |
| Ficheiros > 400 linhas | `kernel/actor.rs`, `kernel/protocol.rs`, `kernel/approval.rs` separados |
| Feature `memory-in-process` | o actor vive atrás da mesma feature que `agent`/`runtime` |

---

## 9. Definition of Done

- [ ] **K1–K8** travadas por teste; `katu-api` sem deps de UI.
- [ ] CLI e TUI são clientes do kernel (zero `Runtime`/`Provider`/`Session` nos front-ends).
- [ ] `Cancel` honrado dentro de tools (L-P3) e no transporte, com `T_cancel.p95 ≤ 250 ms`.
- [ ] G2–G6 corrigidas com teste (e artefacto onde há alvo); G7/G8 caem da separação.
- [ ] `cargo xtask check` e `msrv` verdes; `file-length` verde.
- [ ] `OPTIMIZATION_PLAN.md` §0bis ganha uma linha por item; `LOOP_RESILIENCE.md` regista que L-S2
      foi **substituída** por este plano.
- [ ] Nenhum byte model-visible muda fora do medido (`Model-visible ⟺ logged` intacto).

---

## 10. Não-objetivos

- Tornar o **kernel** multi-threaded. A fronteira é uma thread de superfície; o kernel continua
  single-threaded e determinístico.
- Copiar o loop monolítico ou os dois motores do goose ([`GOOSE_LOOP`](../brainstorm/goose-rs/GOOSE_LOOP.md) §7).
- Mover o loop para `async`/Tokio.
- Processo/socket **antes** de a fronteira in-process estar travada (F4 é condicional).

---

## 11. Mapa sintoma → item

| Sintoma | Item |
|---|---|
| "`Esc` não faz nada durante um `bash`" | F1/F2 · K8 · G1 |
| "UI congela no render/rato durante o turno" | F1/F2 · G7 |
| "sessão vaza threads após um *stall*" | F3 · G2 |
| "o nudge aparece como se eu o tivesse escrito" | F3 · G3 |
| "executou uma tool com argumentos cortados" | F3 · G4 |
| "aceitou o eco como resposta" | F3 · G5 |
| "o CLI e a TUI comportam-se diferente" | F0/F2 · K4 |
| "quero um front-end novo (ACP/serve)" | F4 |
