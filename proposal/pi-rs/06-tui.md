# 06 — TUI (`pi-tui`)

`packages/tui` (19k LOC) é uma biblioteca de terminal própria, com rendering diferencial e dois renderers. Portá-la é uma das partes mais delicadas porque envolve terminfo, largura de caracteres e protocolos de imagem.

## 1. Dois renderers

| Renderer | Comportamento |
|---|---|
| `TuiMainScreen` | Renderiza no buffer principal; preserva o scrollback do terminal. Terminal controla o scroll. |
| `TuiAltScreen` | Viewport de altura fixa no buffer alternativo; scroll é da aplicação; ao parar, restaura o buffer principal e imprime o documento final. |

Ambos implementam a interface `TUI` (componentes, foco, overlays, input, lifecycle, queries, render).

## 2. Rendering diferencial

- Componentes implementam `render(width: u16) -> Vec<Line>`.
- O renderer compara linhas e emite apenas as mudanças (diff por linha/viewport).
- **Synchronized output** (CSI 2026) para updates atômicos sem flicker.
- **Bracketed paste**: pastes grandes viram marcadores (>10 linhas).
- Reset de estilo/hyperlink ao fim de cada linha (é preciso reaplicar estilo por linha).
- `requestRender()` é coalescido — múltiplas requests viram um frame.

Em Rust: a base natural é `crossterm` (raw mode, eventos, ANSI) ou `ratatui`. **Recomendação**: `crossterm` para I/O de terminal + implementação própria do diff (o Pi tem semântica específica de main/alt screen, scrollback preservado, CSI 2026, selection e OSC 8). Adotar `ratatui` inteiro tende a brigar com o modelo de "documento" e o layout de stack. Usar `ratatui` como referência de layout, não como runtime.

## 3. Componentes (a portar)

`Text`, `TruncatedText`, `Markdown`, `Image`, `Input`, `Editor`, `SelectList`, `SettingsList`, `ScrollView`, `Loader`, `CancellableLoader`, `Box`, `Container`, `VStack`, `HStack`, `Spacer`, `MouseRegion`.

Interfaces:
- `Component::render(width, height?) -> Vec<Line>`
- `Focusable` + `CURSOR_MARKER` (posiciona cursor físico para IME CJK)
- Handlers de teclado/mouse; `matchesKey`/`Key`
- `invalidate()` limpa caches dependentes de tema/largura

Em Rust:
```rust
pub trait Component {
    fn render(&mut self, width: u16) -> Vec<Line>;
    fn invalidate(&mut self) {}
    fn handle_key(&mut self, key: &KeyEvent) -> Option<KeyResult> { None }
    fn handle_mouse(&mut self, ev: &MouseEvent) -> Option<MouseResult> { None }
    fn focusable(&self) -> bool { false }
}
```

## 4. Cores

- Modelo de cor `Color` = indexed ANSI | sRGB | OKLCH (e OKHSL aceito no parse).
- `mixColors`, `colorToRgb`, `foregroundAnsi`, `styleText`.
- Conversão para truecolor ou 256 cores conforme capacidades do terminal.
- A cor indexada 0–15 segue a paleta do usuário (sRGB aproximado).
- OKLCH → sRGB fora de gamut; conversões não são cacheadas — converter uma vez e reusar.

Em Rust: implementar OKLab/OKLCH/OKHSL (fórmulas conhecidas, baratas) + `owo-colors`/`anstyle` para emissão ANSI. O `palette` crate pode ajudar em conversões, mas o modelo de gamut do Pi é específico.

## 5. Largura de caracteres e quebra

- Usar `unicode-width` no lugar de `get-east-asian-width`.
- Utilitários: `visibleWidth()`, `truncateToWidth()`, `sliceByColumn()`, `wrapTextWithAnsi()`.
- ANSI escapes, emoji, wide chars e combining precisam ser contados por colunas visuais, não bytes/chars.
- A preservação de ANSI ao truncar é um ponto sensível — portar com testes golden.

## 6. Layout alt-screen (`tui-plan.md`)

Sistema de layout estilo flex:
- `StackEntry { basis, grow, shrink, min_size, max_size, visible }`
- `VStack` / `HStack` alocam regiões; `ScrollView` é dono do scroll de uma região (com `follow: "end"`, `overscroll: "chain"`).
- Geometria é reconstruída a cada frame, mas os caches de linhas dos componentes são reutilizados.
- `render(width)` direto produz documento ilimitado (usado ao restaurar a main screen).
- ScrollView primário: busca com `Ctrl+Shift+F`, navegação por marcadores OSC 133, indicador "scroll to end".

Este é o subsistema que justifica o `tui-plan.md` de 36KB. No port, criar um módulo `layout/` com testes de alocação (stack, overflow, visibilidade).

## 7. Input

- Parser de teclado que cobre protocolos suportados + modificadores; Kitty keyboard protocol (`isKittyProtocolActive`, key release/repeat).
- `KeybindingsManager` com defaults configuráveis (`DEFAULT_EDITOR_KEYBINDINGS`, `DEFAULT_APP_KEYBINDINGS`) e detecção de conflito.
- Mouse normalizado (fullscreen): wheel rola o `ScrollView` mais próximo; drag primário disponível para seleção de transcript; OSC 8 tem precedência.
- `StdinBuffer` para batching/parsing de sequências.
- Kill ring, undo stack, word navigation, fuzzy match, autocomplete (paths + slash commands).

Em Rust: `crossterm::event` cobre a maior parte; adicionar parsing Kitty e bracketed paste; `unicode-segmentation` para navegação por palavra/grafema.

## 8. Imagens inline

- Protocolos Kitty e iTerm2.
- `Photon` (WASM/Rust) usada para resize/processamento no `coding-agent`; em Rust usar o crate `image` e emitir os protocolos diretamente.
- Elimina `@silvia-odwyer/photon-node`, o `.wasm` e o `image-resize-worker`.

## 9. Componentes nativos (removidos no Rust)

| Native TS | Substituição Rust |
|---|---|
| Darwin `darwin-platform.m` | `arboard` (clipboard) + detecção de modificadores via `crossterm`/`kitty` |
| Linux `linux-platform-x11.c` (libxcb) | `arboard` (X11/Wayland) ou `wl-clipboard-rs` |
| Win32 `win32-platform.c` | `arboard` + `windows` crate se necessário |

Isso remove `build:native:{darwin,linux,win32}`, prebuilds e a lógica de fallback para `wl-paste`/`xclip`.

## 10. Testes

O original usa `@xterm/headless` para validar o stream ANSI. Em Rust:
- `vt100` ou `avt` (emulador de terminal headless) para comparar frames.
- Golden files do stdout; testes de resize, wide chars, tema, foco, main/alt.
- `PI_TUI_WRITE_LOG` → utilitário de captura de ANSI.

## 11. Ordem de port

1. `Terminal` + raw mode + tamanho + input básico (`crossterm`).
2. `Text`, `Box`, `Container`, `VStack`, `HStack`, `Spacer`.
3. Renderer main-screen com diff + synchronized output.
4. `Editor`/`Input` + keybindings + cursor marker.
5. `Markdown` (pulldown-cmark) + syntax highlight (`syntect`).
6. `SelectList`, `SettingsList`, `Loader`.
7. `ScrollView` + `TuiAltScreen` + overlays + mouse.
8. Imagens inline + clipboard.
