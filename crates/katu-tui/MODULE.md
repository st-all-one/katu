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
  devolve um `Command` só quando há efeito. Testado por modo, sem terminal. O `Command`/`Event`
  vivem no protocolo do kernel (`katu_core::api`, KERNEL_SURFACE F0); `katu-tui` re-exporta-os
  (`Update` é o `Event`).
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
  entra na transcrição (§50.3). O **cancelamento** (**`Esc` e `Ctrl-C`** durante o stream) é
  cooperativo: o loop principal (`run`) escreve a `Flag` partilhada via `Handler::cancel_flag`
  (→ `ActivitySink::cancelled`), o turno fecha limpo e a UI mostra `Update::Cancelled`. A `Flag`
  partilhada interrompe também as tools longas (L-P1/L-P3). O **steering** (E20-T16) usa o mesmo
  loop: o que se escreve aparece na linha de entrada (`App::steering`) e o `Enter` envia
  `Command::Steer` (FIFO) que a borda aplica no passo seguinte (`ActivitySink::steer`). O loop
  principal **nunca** bloqueia no kernel (`Handler::poll` drena sem bloquear), pelo que rato/resize/
  cópia continuam vivos durante o turno (G7); o `Painter` só pinta o `Live` e apresenta o challenge.
  Os **argumentos crus** do modelo (`Live::Tool { name, args }`) aparecem
  no painel ao lado da tool; a tool concluída mostra `✓ <nome>: <resumo>` (`Live::ToolDone`,
  `LIVE_FLOW` LF4); o **raciocínio** (`Live::Thinking`) é desenhado em tom esbatido
  (`LIVE_FLOW` LF2) e o painel **persiste** após o turno até ao próximo `submit` (LF3), pelo que o
  fluxo fica relível.
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
  corre na **thread do kernel** (a borda é só cliente) e a UI nunca bloqueia: drena eventos e
  continua a sondar input durante o stream (L-P1/F2).
- **E20-T10 ☑ / E20-T15 ☑ (TUI v2)** — comandos `/` na linha de mensagem (`src/menu.rs` +
  `src/overlay.rs` + `src/app/menu.rs`): mini-menus de `/model` e `/thinking` (adaptados às
  **capacidades** via `Update::ThinkingOptions`), ajuda `?`, e `Esc`/`Ctrl-C` como cancelamento da
  rodada (`src/run.rs`); os atalhos antigos saíram do keymap. A cópia por seleção de rato usa
  **OSC 52** (`src/copy.rs`, E20-T14). O **steering** e a **citação** vivem na linha de entrada:
  `@<path>` enfileira caminhos (`src/app/menu.rs`, E20-T12) que são prefixados ao próximo turno, e
  escrever durante o turno enfileira prompts aplicados no passo seguinte (E20-T16). `/plan`
  (E20-T11) liga o modo de planeamento (barra mostra `PLANO`; `Update::Plan`) e `!<cmd>` (E20-T12)
  emite `Command::Shell` para a borda executar pela política. `/skill:<nome>` (E20-T13) emite
  `Command::Skill` e a borda força o carregamento do `SKILL.md`.
- **E21 ☑** — **login na TUI**: `/login` abre o menu de provider (opencode Go/Zen **ou** llama.cpp)
  e pede o que falta; `/login opencode <chave>` e `/login llama [url]` resolvem já o pedido; `/logout`
  termina a sessão (E21). A chave é mascarada no render (`App::input_display`). O pedido
  (`Command::Login(LoginRequest)`) é resolvido pela borda, que reconstrói o provider e o modelo;
  o embedding **não** é tocado.

## Fronteira

- **Cliente do kernel**: consome o protocolo `katu_core::api` (emite `Command`, injeta `Event` no
  `App`); não possui `Runtime`/`Provider`/`Session` (K2). O transporte é em memória (KERNEL_SURFACE
  F0), não um serviço: sem servidor, sem rede (G7).
- Não depende de `katu-tools`/`katu-providers`/`knudge` (firewall `layers.toml`): a UI só vê o
  estado puro e emite `Command`s que a borda executa.
