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
  benchmark por frame e verificação de zero alocações no hot path (E15-T01/E18-T10).
- **E10-T05 ◐** — **painel de atividade efémero** (`App::live`/`streaming`/`thinking` + `Update::Live`
  + `Painter`): o stream do modelo e as tools em curso aparecem ao vivo, **fora** do log e do
  transcript (`katu_core::event!` `tui.live`). O durável é o próprio log de sessão. **Falta:**
  transcrição em ficheiro com viewer read-only.
- **E10-T06 ◐** — cabeçalho mostra fase + pendência a partir do `App` (alimentado pelo binário com
  `Runtime::phase`).
- **E10-T04 ☑** — **superfície de política**: as recusas do kernel (`Denied`/`Unavailable`) chegam
  ao painel e ao transcript com **regra + evidência** (`Live::Refused`/`Live::Unavailable`, evento
  `tui.live` com `kind=refused`) e o **override** por **challenge-and-response** (`approval::Challenge`:
  perguntas positivas + justificação, `Esc`/`Ctrl-C` cancelam) é apresentado numa sobreposição
  centrada (`approval::render`); o `Painter::challenge` corre o sub-loop de teclado durante o turno
  (E07-T05).
- **E10-T07 ◐** — **modelo** (`m` → `Action::CycleModel`) e **grau de pensamento**
  (`t` → `Action::CycleThinking`) como `Action`s puras, com o estado em `src/controls.rs`
  (`Controls`) refletido no cabeçalho; a borda aplica ao **próximo** turno e publica a lista via
  `Update::Models`. **Falta:** compactar (E09-T07), ver lixeira (E06-T09) e a lista de modelos vir
  do catálogo (`dynamic_models`, E12-T02). O turno corre de forma **síncrona** no handler da borda
  (executor em background é trabalho futuro).

## Fronteira

- Sem servidor, sem rede (G7). Não é um protocolo.
- Não depende de `katu-tools`/`katu-providers`/`knudge` (firewall `layers.toml`): a UI só vê o
  estado puro e emite `Command`s que a borda executa.
