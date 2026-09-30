# E10 — TUI e UX de codificação

> **Fase 6.** A interface desenhada para o fluxo de codificação (tese, §0). Mostra a política em
> ação: `blocked`/`needs_human` com evidência e `override_reason`.
>
> **Decisões:** DF3, DF4. **Depende de:** E09.
> **Gate do épico:** panic-safe + evidência visível + keymap testável.

---

## Decisões de UX herdadas

- **Panic hook que restaura o terminal** (§14, taskdeck; §16.7).
- **Mapa puro de teclas:** `map_key(KeyEvent, Mode) -> Action` + `apply_action(&mut App, Action)`,
  testável por modo (§14).
- **Split live ↔ durable** (§50.3): widget ao vivo que **não entra no chat**; transcrição completa
  sempre em ficheiro; viewer read-only.
- **Resultado de subagente = status + resposta final**, nunca a transcrição (§50.4).
- **Thinking fora do contexto durável e do ecrã**, com toggle em runtime (§50.17).

---

## Padrões de referência (skill `ratatui-tui`)

O guia [`ratatui-tui`](../.agents/skill/ratatui-tui/SKILL.md) (Ratatui 0.30 + Crossterm 0.29)
fecha o ponto em aberto **OA3** com um desenho testado. Padrões a herdar:

- **Estado central, render puro.** Um `App` único detém o estado; a camada de render só desenha a
  partir dele (**nunca** chama I/O); os handlers de evento só produzem `Action`. `Screen`/`Mode`/
  `InputTarget` como enums pequenos substituem `if`s soltos — o análogo direto de
  `map_key`/`apply_action` (E10-T02).
- **I/O fora do caminho de render.** Um executor em background (tokio) comunica por dois canais
  `mpsc` (`Action` → `Result`); a UI nunca faz `await`. Combina com E01-T09 e com o
  `spawn_blocking` do knudge (E03-T04).
- **Confirmação em toda a escrita.** Nenhuma mutação acontece sem `y` explícito — é o "undo
  barato" e a materialização de `RequireApproval` (DF2/DF4).
- **Barra de estado com pendências.** `Info|Success|Error` + indicador de trabalho em voo, para o
  utilizador não achar que a TUI travou.
- **Restauro do terminal em todo o caminho de saída** (RAII + panic hook) — já em E10-T01.
- **Parsing defensivo** do que vem de fora (aplica-se também a `--json`/eventos): campo ausente →
  default; item malformado não derruba o lote.

> **A confirmar:** o skill recomenda **Ratatui** sobre `crossterm` cru; OA3 passa a ter base
> empírica. O cache SQLite/WAL do skill **não** se aplica — o katu não persiste estado de UI.

---

## Tarefas

### E10-T01 ☑ Esqueleto TUI (`crossterm` cru)
- **Entregáveis:** loop de eventos, alt-screen, panic hook, restauro do terminal.
- **Aceite:** matar o processo durante o render deixa o terminal utilizável; teste de panic hook.
- **Estado:** `katu-tui/src/run.rs` usa `try_init` (modo cru + ecrã alternativo + panic hook que
  restaura) e uma guarda RAII (`Drop` → `restore`) em qualquer saída; loop com `poll(50 ms)`. O
  hook é o do `ratatui` (restaura antes de chamar o hook anterior).

### E10-T02 ☑ Keymap puro e modos
- **Entregáveis:** `Action`, `AppMode`, `map_key`, `apply_action`.
- **Aceite:** o keymap é uma função pura testada por modo; nenhuma lógica de UI no handler de
  eventos.
- **Estado:** `katu-tui/src/action.rs` (`Mode`, `Action`, `map_key`) + `App::apply_action` em
  `app.rs` (devolve `Command` só quando há efeito). Testado por modo (Normal/Insert/Confirm) e para
  `Ctrl-C`/caracteres de controlo; o loop de eventos só traduz a tecla em `Action`.

### E10-T03 ◐ Render diferencial e orçamento de render
- **Entregáveis:** diff de frames; throttle; teto de trabalho por frame.
- **Estado:** o `diff` de células é delegado ao `Terminal` do `ratatui`; o **throttle** vive em
  `katu-tui/src/throttle.rs` (`Throttle`, guiado pela porta `Clock` — sem `Instant::now`, testado
  com relógio determinístico) e coalesce os quadros a ~60 fps, forçando um quadro em cada tecla e
  fim de turno. O **teto de trabalho** limita entradas da conversa (200), linhas do painel (100) e
  a cauda do buffer de streaming (8 KiB, em fronteira de caractere).
- **Falta:** o **benchmark** por frame e a verificação de **zero alocações** no hot path
  (E15-T01/E18-T10).
- **Aceite:** benchmark de render por frame dentro do orçamento (E15); sem alocações no hot path
  de render (verificado por lint/bench).

### E10-T04 ☑ Superfície de política: `blocked`/`needs_human`
- **Entregáveis:** apresentação do veredicto com `rule_id`, evidência e o caminho de override
  (challenge-and-response, não rubber-stamp, §33).
- **Aceite:** um `Deny` mostra a regra e a evidência; um override exige resposta a perguntas
  positivas e regista `override_reason` + `overridden_by`.
- **Estado:** as recusas do kernel chegam à UI com **regra + evidência** (`Activity::Refused`/
  `Unavailable` → `Live::Refused`/`Unavailable`, evento `tui.live` `kind=refused`); a recusa fica no
  painel e no transcript (teste e2e: `read .env` → `Denied{contain-sensitive-read}`). O **override**
  interativo está feito: quando a política devolve `RequireApproval`, o `LivePainter` abre um
  **challenge-and-response** (`katu-tui::Challenge`, perguntas positivas + justificação obrigatória;
  `Esc`/`Ctrl-C` cancelam) e, com a assinatura, o kernel regista `ApprovalGranted` e re-executa a
  chamada (E07-T05). Um `Deny` critical e um `NeedsHuman` **não** são sobreponíveis (muro).

### E10-T05 ◐ Split live/durable
- **Entregáveis:** painel de observação efémero; transcrições em ficheiro; viewer read-only.
- **Aceite:** o live não entra no transcript do modelo; o durable é reconstruível do log.
- **Estado:** painel de atividade efémero feito (`App::live`/`streaming`/`thinking`, `Update::Live`,
  `Painter` em `katu-tui/src/run.rs`): o stream do modelo e as tools em curso aparecem ao vivo,
  com o evento estruturado `tui.live`, **fora** do log e do transcript (testado: deltas ao vivo não
  entram no contexto do modelo). O durável é o próprio log de sessão (`Session`), reconstruível.
  **Falta:** transcrição em ficheiro dedicada e viewer read-only.

### E10-T06 ◐ Checkpoint e estado visíveis
- **Entregáveis:** indicador de fase, checkpoint atual, pendências, próxima ação.
- **Aceite:** o estado mostrado deriva do `State` (fonte única), nunca de variável de UI paralela.
- **Estado:** o cabeçalho mostra a fase (via `Runtime::phase()` → `Update::Phase`) e a pendência
  (derivada do `Status`); a barra mostra o estado. **Falta:** checkpoint atual/próxima ação quando
  o checkpoint tipado (E09-T02) for ligado à UI.

### E10-T07 ◐ Controlos do core na TUI
- **Entregáveis:** selector de **modelo** e de **grau de pensamento** (`set_model`/`set_thinking`,
  E12-T10); comando de **compactar conversa** (E09-T07) com pré-visualização antes/depois; vista
  da **lixeira** (`.katu/trash`) com `restore` (E06-T09); confirmação explícita em toda a ação
  destrutiva.
- **Estado:** o **seletor de modelo** (`m`) e o **grau de pensamento** (`t`) estão feitos como
  `Action`s puras (`CycleModel`/`CycleThinking`) + `Command::{SetModel, SetThinking}`, com o estado
  em `katu-tui/src/controls.rs` (`Controls`) e o modelo/pensamento no cabeçalho. A **vista da
  lixeira** (`l`) lista `.katu/trash` (`trash::list`) e restaura com `r` (`trash::restore`; E06-T09,
  recuperável). A **compactação** (`c`) mostra a pré-visualização antes/depois
  (`Runtime::compaction_preview`, E09-T07) e o **gate de verificação** (`v` → `Runtime::verify`;
  se bloquear, pede override humano por challenge, E09-T03). A borda aplica a escolha de modelo ao
  **próximo** turno (o agente **nunca** se auto-escala) e publica a lista via `Update::Models`.
  **Falta:** a lista de modelos vir do catálogo (`dynamic_models`, E12-T02) e o **esvaziamento** da
  lixeira com challenge-and-response.
- **Aceite:** cada controlo mapeia para uma `Action` pura (E10-T02) e é testável por modo; o
  agente **não** altera modelo/pensamento sem o utilizador; esvaziar a lixeira exige
  challenge-and-response.

---

## Definition of Done

- [ ] E10-T01…T07 concluídas.
- [ ] Terminal panic-safe; keymap testado; evidência visível.
- [ ] Benchmark de render no orçamento.
- [ ] `cargo xtask check` e job `msrv` verdes.

## Não-objetivos

- Temas, imagens inline, plugins de UI: adiados.
- Desktop/Electron: fora de escopo (§0).
