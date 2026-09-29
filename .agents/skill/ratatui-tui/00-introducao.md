# 00 — Introdução: modelo de loop de uma TUI Ratatui

Este guia documenta como construir uma TUI **prática e estável** com `ratatui` + `crossterm` + `tokio`, usando como referência o projeto real `redmine-tui` (gerenciador de tasks do Redmine).

## O que é uma TUI Ratatui

Ratatui desenha **todo o terminal a cada frame** (bufferização por diffs). Não existe árvore de widgets persistente: você reconstrói a tela a cada iteração do loop a partir do **estado** da aplicação. Isso simplifica a lógica (a renderização é uma função pura do estado) e evita bugs de "widget desatualizado".

```
            ┌──────────────────────────────────────────────────┐
            │           App (estado centralizado)             │
            └──────────────────────────────────────────────────┘
  draw() ◄──────────────────────────────────────────► eventos
   │                                                     │
   └── ratatui::run(loop) ──► ui::render(&mut app)        │
          │                     │                          │
          │                     ▼                          │
          │               Frame / Buffer                  │
          └──────────────► poll_events(&mut app) ◄─────────┘
```

## O loop principal (não bloqueie a UI)

O coração de qualquer TUI ratatui é um loop que roda ~10x/segundo. No `my-redmine` (`src/main.rs:82`):

```rust
let atx = action_tx.clone();
ratatui::run(|terminal| {
    loop {
        // 1. Renderiza um frame a partir do estado atual
        terminal.draw(|frame| ui::render(&mut app, frame))?;

        // 2. Consome resultados assíncronos que chegaram (sem esperar)
        if let Ok(r) = result_rx.try_recv() {
            handler::handle_result(&mut app, r, &atx);
        }

        // 3. Envia ações que a UI enfileirou durante o frame anterior
        for a in app.dequeue_actions() { let _ = atx.send(a); }

        // 4. Lê teclado (com timeout curto — nunca bloqueia o loop)
        event::poll_events(&mut app)?;

        if app.should_quit { break; }
    }
    Ok::<_, color_eyre::Report>(())
})?;
```

Regras de ouro do loop:

1. **Nunca faça I/O (rede/arquivo) dentro do `draw`**. Renderização deve ser rápida e determinística.
2. **Não bloqueie o loop esperando rede**. Use `try_recv` para ver se chegou resultado, e `event::poll(timeout)` para teclado.
3. **Enfileire ações** em vez de executá-las: a UI seta `pending_actions`; o loop drena e envia para o executor (detalhes no [04-integracao-async.md](./04-integracao-async.md)).
4. **Centralize o estado** num único `App`; render e handlers só mutam esse struct.

## Stack recomendada (usada no caso real)

| Dependência | Versão | Papel |
|---|---|---|
| `ratatui` | 0.30 | Widgets, Layout, render |
| `crossterm` | 0.29 | Terminal raw mode, eventos de teclado |
| `tokio` | 1 (full) | Runtime async (executor da API) |
| `reqwest` | 0.12 (json) | Cliente HTTP |
| `serde` / `serde_json` | 1 | Tipagem e wire format |
| `rusqlite` | 0.32 (bundled) | Cache local offline |
| `color-eyre` | 0.6 | Erros amigáveis + `?` no `main` |
| `thiserror` | 2 | Erros tipados em libs |
| `ratatui-textarea` | 0.9 | Campo de texto multi-linha |
| `pulldown-cmark` | 0.13 | Render markdown na descrição |
| `tracing` / `tracing-subscriber` | 0.1 | Logs (warnings de cache) |

Para um projeto pequeno, adicione apenas `ratatui`, `crossterm`, `color-eyre` e uma estratégia de I/O. Tokio/reqwest/sqlite só entram quando há integração remota.

## Onde a UI não deve ir

No `my-redmine`, arquivos utilitários ficam fora do caminho de render:

- `src/theme.rs` — cores/estilos (light/dark)
- `src/input.rs` — wrapper do `ratatui-textarea` com confirm/cancel
- `src/md.rs` — converte markdown em `Vec<Line>` estilizados
- `src/types.rs` — structs, enums, constantes, helpers de formatação

A renderização (`ui.rs`) e os handlers (`event/`) **só consomem** esses módulos; eles não sabem de API/cache.

## Próximo passo

Veja [01-arquitetura.md](./01-arquitetura.md) para a organização de módulos e o estado central `App`.
