---
name: ratatui-tui
description: >
  Ratatui 0.30 + Crossterm 0.29 TUIs in Rust: render loop, state architecture,
  keyboard events, async integration with Tokio, offline cache in SQLite/WAL,
  and REST API consumption. Load when building interactive terminal UIs.
category: libraries
version: "0.30"
tags: [rust, ratatui, crossterm, tui, terminal, tokio, sqlite, cli]
license: MIT
---

# Ratatui 0.30 TUI (Rust)

## Use When
- Interactive CLIs, dashboards, terminal managers
- Lists, forms, popups, keyboard navigation
- TUIs that consume REST APIs and cache offline
- UIs that must not block on network I/O

## Core Rules
- Single event loop: draw → poll events → update state → repeat.
- Keep one central `App` state; derive screens/renders from it.
- Never do blocking I/O in the render path; run network on Tokio tasks and send results over `mpsc`.
- Restore the terminal on every exit path (RAII guard; `disable_raw_mode`, leave alternate screen).
- Handle `Ctrl-C`/`Esc` explicitly; support resize (`Event::Resize`).
- Use `ListState`/`TableState` for selection and scroll; clamp indices.
- Bound memory: stream/paginate API responses; cache with TTL.
- SQLite cache: WAL mode, busy timeout, schema versioning.
- Debounce rapid key repeats; use `poll(Duration)` to keep CPU low.

## Core Patterns
```rust
loop {
  terminal.draw(|f| ui(f, &app))?;           // render from state
  if event::poll(Duration::from_millis(50))? {
    if let Event::Key(key) = event::read()? {
      match key.code {
        KeyCode::Char('q') => break,
        KeyCode::Down => app.next(),
        KeyCode::Up => app.prev(),
        _ => {}
      }
    }
  }
  while let Ok(msg) = rx.try_recv() { app.apply(msg); }  // async results
}
```

```rust
// Async: spawn request, send result
tokio::spawn(async move {
  let items = client.list().await?;
  tx.send(ApiResult::Items(items)).await.ok();
});
```

## File Map
| File | Content |
|---|---|
| `00-introducao.md` | Loop model, recommended stack |
| `01-arquitetura.md` | Modules, central `App`, Screen/Mode/InputTarget enums |
| `02-renderizacao.md` | Layout/Constraint, List+ListState, popups, theme |
| `03-eventos.md` | Key polling, modes, per-screen handlers, confirmation |
| `04-integracao-async.md` | Tokio executor, mpsc, ApiAction/ApiResult |
| `05-cache-persistencia.md` | SQLite WAL, versioning, offline cache, merge |
| `06-estabilidade-seguranca.md` | Terminal restore, panics, errors, secrets |

## Read Order
`00`→`01`→`02`→`03`; async `04`; cache `05`; hardening `06`.

## Prereqs
Rust 1.85+ (edition 2024); basic terminal/async knowledge.
