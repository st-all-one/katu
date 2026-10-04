# LIVE_FLOW — plano de implementação: o fluxo do turno visível nas superfícies

> **Plano de visibilidade do turno**, derivado do goose
> ([`GOOSE_LOOP.md`](../brainstorm/goose-rs/GOOSE_LOOP.md) §8) e alinhado com a fronteira
> [`KERNEL_SURFACE.md`](KERNEL_SURFACE.md).
>
> **Tese.** O kernel **já** produz o fluxo efémero (`Live`: raciocínio, tool calls, execução) e
> entrega-o a qualquer superfície. O sintoma "só vejo o output final, a tela fica travada" **não**
> é falta de produção — é **consumo**: o CLI descarta o stream e a TUI esconde o raciocínio e limpa
> o painel no fim do turno. A correcção é fazer as superfícies **consumirem o stream de eventos**,
> como o goose (`render_message_streaming`, `stream-json`) — sem abrir caminho novo no kernel (G7).
>
> **Evidência de partida:** `crates/katu/src/agent/command.rs::drive` (ignora tudo excepto
> `Turn`/`Failure`); `crates/katu-tui/src/ui.rs::activity_lines` (não desenha `thinking`);
> `crates/katu-tui/src/app/update.rs` (`Done` limpa o painel).

---

> **Estado (execução).** **LF1–LF5** implementados e travados por teste: o CLI mostra o fluxo
> (raciocínio, tools com resumo, texto) em `stderr` e o `--output stream-json` emite um evento por
> linha; a TUI desenha o raciocínio e o resumo das tools e o painel persiste até ao próximo turno.

## 0. Hierarquia e método

**Hierarquia (o de cima vence):**

1. **Q — qualidade/transparência.** O utilizador vê o que o modelo faz, quando o faz.
2. **P — performance.** Nada aqui adiciona I/O ao turno: os eventos já existem no canal.
3. **S — simplificação.** As superfícies são **consumidores** do mesmo stream; não há caminho novo.

**Restrições inflexíveis:** zero `unwrap/expect/panic` em `src/`; ficheiros ≤ 400 linhas;
`cargo xtask check` verde; o fluxo **nunca** entra no log nem no contexto do modelo
(`Model-visible ⟺ logged`, E04).

## 1. Contrato da visibilidade (invariantes)

| # | Invariante | Onde é imposta | Como é travada |
|---|---|---|---|
| **V1** | O CLI mostra o fluxo (raciocínio/tools/texto) **enquanto** corre, em `stderr`; `stdout` fica só com os dados | `agent/command/progress.rs` | teste do formatter |
| **V1b** | `--output stream-json` emite um objecto JSON por evento em `stdout` | `agent/command/progress.rs` | teste do `stream_line` |
| **V2** | A TUI desenha o raciocínio em curso (efémero, com teto de bytes) | `katu-tui/src/ui.rs::activity_lines` | teste de render |
| **V3** | O fluxo do último turno fica visível até ao próximo `submit` | `katu-tui/src/app/update.rs` | teste de estado |
| **V4** | Nada do fluxo entra no log, no transcript durável ou no contexto do modelo | kernel | `Model-visible ⟺ logged` (existe) |
| **V5** | Sem TTY, o CLI não escreve progresso em `stderr` (não polui *pipes*) | `progress.rs` | teste de activação |

## 2. Itens

### LF1 — CLI: progresso ao vivo (adotado)

- **Sintoma:** `katu run "…"` fica minutos sem escrever nada e só imprime o texto final.
- **Causa:** `drive` (`agent/command.rs`) consome `Turn`/`Failure` e descarta `Live`/`Info`/`Phase`.
- **Mecanismo goose:** `process_agent_response` consome o stream e `render_message_streaming`
  escreve cada item; a interface **é** um consumidor do stream (`GOOSE_LOOP` §8).
- **Mudança:** novo `Progress` (puro `render` + escritor) que traduz `Event` em linhas de
  progresso e as escreve em `stderr` **só quando `stderr` é TTY**. `stdout` permanece só com os
  dados (regra `report.rs`). O texto do assistente é mostrado ao vivo e, no fim, o envelope
  repete-o em `stdout` (progresso ≠ resultado).
- **Teste:** `render` devolve raciocínio/tools/recusa; `with_enabled(false)` não escreve.

### LF2 — TUI: raciocínio visível (adotado)

- **Sintoma:** durante o raciocínio (a fase mais longa) o ecrã parece travado.
- **Causa:** `apply_live` guarda `Live::Thinking` em `App::thinking`, mas `activity_lines` só
  desenha `live()` e `streaming()`.
- **Mudança:** `activity_lines` passa a desenhar o raciocínio (tom esbatido, itálico) **antes** das
  tools e do texto; `panel.rs` aplica `trim_tail` ao raciocínio (faltava o teto de bytes).
- **Teste:** render inclui o texto de raciocínio; o buffer é limitado.

### LF3 — TUI: fluxo persistente (adotado)

- **Sintoma:** no fim do turno o painel desaparece (`Done`/`Cancelled` limpam), pelo que o fluxo
  não é relível.
- **Mudança:** `Done`/`Cancelled` deixam de limpar; o painel persiste até ao próximo `submit`
  (`menu.rs::submit` já limpa no início do turno seguinte).
- **Teste:** `Done` mantém o painel; um `submit` novo limpa-o.

### LF4 — resultado da tool (adotado)

- **Sintoma:** `Live::ToolDone` mostrava só `✓ <nome>`, sem o resultado da execução.
- **Mudança:** `Activity::ToolDone`/`Live::ToolDone` ganham um **resumo** de uma linha do delta
  (`CallOutcome::delta` → `summarize`, teto de 200 caracteres); a TUI e o CLI mostram
  `✓ <nome>: <resumo>`. O resumo é **efémero** (nunca entra no log nem no contexto).
- **Teste:** `a_tool_done_carries_a_one_line_summary`; painel/render.

### LF5 — CLI: `--output stream-json` (adotado)

- **Mudança:** `katu run --output <text|json|stream-json>` (o `--json` mantém-se). `stream-json`
  emite **um objecto JSON por linha** (JSONL) em `stdout` durante o turno, seguido do envelope
  final; `text` (default) mantém o progresso humano em `stderr`; `json` só o envelope.
- **Teste:** `stream_json_emits_one_object_per_event`; resolução de `Output` em `run_params`.

## 3. O que **não** copiar do goose

- O `tokio::select!` e o `CancellationToken` — o katu tem o `Flag` partilhado (L-P1/L-P3).
- O buffer anti-flicker de terminal — o `ratatui` faz render diferencial e o `Throttle` coalesce.
- `AgentEvent` como tipo próprio — o protocolo `Command`/`Event` já é esse contrato.

## 4. Referências

- **katu:** `crates/katu/src/agent/command.rs` (`drive`), `crates/katu/src/agent/command/progress.rs`
  (novo), `crates/katu-tui/src/ui.rs` (`activity_lines`), `crates/katu-tui/src/app/panel.rs`
  (`apply_live`), `crates/katu-tui/src/app/update.rs` (`Done`), `crates/katu-core/src/api/live.rs`.
- **goose:** [`GOOSE_LOOP.md`](../brainstorm/goose-rs/GOOSE_LOOP.md) §8 (CLI/TUI) e §3.4 (consumo do
  stream); [`GOOSE_VS_KATU_LOOP.md`](../brainstorm/goose-rs/GOOSE_VS_KATU_LOOP.md) §2 (UI durante o
  turno).
