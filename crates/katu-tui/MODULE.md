# `katu-tui`

**Épico:** E10 · **Fase:** 6.

A **interface de terminal** do katu (uma das duas superfícies, com a CLI — G7).

## Responsabilidade

- UX de codificação sobre `ratatui` 0.30 + `crossterm` 0.29 (estado central, render puro).
- Panic-safe: restaura o terminal em qualquer saída (guarda RAII + panic hook do `try_init`).
- Controlos do core: modelo/grau de pensamento, compactar, ver lixeira.

## Estado (E10)

- **E10-T01 ☑** — esqueleto (`src/run.rs`): `try_init`/`restore`, guarda RAII, loop de eventos com
  `poll(50 ms)`. O panic hook reencaminha para o hook anterior depois de restaurar.
- **E10-T02 ☑** — keymap **puro** (`src/action.rs`): `Mode`, `Action`, `map_key`; `App::apply_action`
  devolve um `Command` só quando há efeito. Testado por modo, sem terminal.
- **E10-T03 ◐** — render diferencial delegado ao `Terminal` (diff de células); **throttle** por
  `Clock` (`src/throttle.rs`; ~60 fps, forçado em cada tecla/fim de turno) e **teto de trabalho**
  por quadro (200 entradas de conversa, 100 linhas de painel, cauda de 8 KiB do stream). **Falta:**
  benchmark por frame feito (`xtask bench-render`/`gate:render`, E15-T01); falta a verificação de
  zero alocações no hot path (E18-T10).
- **E10-T05 ☑** — **painel de atividade efémero** (`App::live`/`streaming`/`thinking` + `Update::Live`
  + `Painter`): o stream do modelo e as tools em curso aparecem ao vivo, **fora** do log e do
  transcript (`katu_core::event!` `tui.live`). O durável é o próprio log de sessão, reconstruível; a
  **transcrição durável** vive em `<root>/.katu/transcript.md` (a borda projeta o log e escreve-a
  atomicamente após cada turno) e o **viewer read-only** (`T` → `Action::OpenTranscript`,
  `Command::Transcript`, `src/transcript.rs`) lê o ficheiro com scroll e sem edição — o live nunca
  entra na transcrição (§50.3). O **cancelamento** (**só `Esc`** durante o stream) é cooperativo
  (`Painter::cancelled`/`poll_input` → `ActivitySink::cancelled`): o turno fecha limpo e a UI
  mostra `Update::Cancelled`. O **steering** (E20-T16) usa a mesma sondagem não bloqueante: o que se
  escreve aparece na linha de entrada e o `Enter` enfileira um prompt (FIFO) que a borda aplica no
  passo seguinte (`ActivitySink::steer`). Os **argumentos crus** do modelo (`Live::Tool { name, args }`) aparecem
  no painel ao lado da tool.
- **E10-T06 ☑** — cabeçalho mostra fase + pendência + **próxima ação** do checkpoint
  (`Update::NextAction`) a partir do `App` (alimentado pelo binário com `Runtime::phase`), e o
  **uso/custo** do turno (`Update::Usage`, formatado pela borda).
- **E10-T04 ☑** — **superfície de política**: as recusas do kernel (`Denied`/`Unavailable`) chegam
  ao painel e ao transcript com **regra + evidência** (`Live::Refused`/`Live::Unavailable`, evento
  `tui.live` com `kind=refused`) e o **override** por **challenge-and-response** (`approval::Challenge`:
  perguntas positivas + justificação, `Esc`/`Ctrl-C` cancelam) é apresentado numa sobreposição
  centrada (`approval::render`); o `Painter::challenge` corre o sub-loop de teclado durante o turno
  (E07-T05).
- **E10-T07 ☑** — **modelo** (`m` → `Action::CycleModel`) e **grau de pensamento**
  (`t` → `Action::CycleThinking`) como `Action`s puras, com o estado em `src/controls.rs`
  (`Controls`) refletido no cabeçalho; **vista da lixeira** (`l` → lista `.katu/trash`, `r` restaura
  e `x` esvazia com challenge, `src/trash.rs`), **compactação** (`c` → liga/desliga o contexto
  efetivo, E09-T07) e **gate de verificação** (`v`, E09-T03). A borda aplica o modelo ao **próximo**
  turno e publica a lista via `Update::Models` (do **catálogo** do provider, E12-T02/T10). O turno
  corre de forma **síncrona** no handler da borda (executor em background é trabalho futuro).
- **E20-T10 ☑ / E20-T15 ☑ (TUI v2)** — comandos `/` na linha de mensagem (`src/menu.rs` +
  `src/overlay.rs` + `src/app/menu.rs`): mini-menus de `/model` e `/thinking` (adaptados às
  **capacidades** via `Update::ThinkingOptions`), ajuda `?`, e `Esc` como **único** cancelamento da
  rodada (`src/run.rs`); os atalhos antigos saíram do keymap. A cópia por seleção de rato usa
  **OSC 52** (`src/copy.rs`, E20-T14). O **steering** e a **citação** vivem na linha de entrada:
  `@<path>` enfileira caminhos (`src/app/menu.rs`, E20-T12) que são prefixados ao próximo turno, e
  escrever durante o turno enfileira prompts aplicados no passo seguinte (E20-T16).

## Fronteira

- Sem servidor, sem rede (G7). Não é um protocolo.
- Não depende de `katu-tools`/`katu-providers`/`knudge` (firewall `layers.toml`): a UI só vê o
  estado puro e emite `Command`s que a borda executa.
