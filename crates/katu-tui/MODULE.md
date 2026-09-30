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
- **E10-T03 ◐** — render diferencial delegado ao `Terminal`; sem alocações no hot path e
  orçamento/benchmark por frame (E15) por medir. O `Painter` redesenha a cada evento (throttle por
  tempo exige o port `Clock`, adiado para T03).
- **E10-T05 ◐** — **painel de atividade efémero** (`App::live`/`streaming`/`thinking` + `Update::Live`
  + `Painter`): o stream do modelo e as tools em curso aparecem ao vivo, **fora** do log e do
  transcript (`katu_core::event!` `tui.live`). O durável é o próprio log de sessão. **Falta:**
  transcrição em ficheiro com viewer read-only.
- **E10-T06 ◐** — cabeçalho mostra fase + pendência a partir do `App` (alimentado pelo binário com
  `Runtime::phase`).
- **E10-T04/T07 ☐** — superfície de política (`blocked`/`needs_human` + override) e controlos do
  core (modelo/pensamento, compactar, lixeira) por fazer. O turno corre de forma **síncrona** no
  handler da borda (executor em background é trabalho futuro).

## Fronteira

- Sem servidor, sem rede (G7). Não é um protocolo.
- Não depende de `katu-tools`/`katu-providers`/`knudge` (firewall `layers.toml`): a UI só vê o
  estado puro e emite `Command`s que a borda executa.
