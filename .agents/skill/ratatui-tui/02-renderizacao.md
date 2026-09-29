# 02 — Renderização: Layout, List, scroll e popups

A renderização do `my-redmine` (`src/ui.rs`) demonstra os padrões essenciais: dividir a tela com `Layout`, listas com `ListState`, scroll **clampado**, popups centrais e tema. Tudo é reconstruído a cada frame a partir do `App`.

## Estrutura base: header / status bar / conteúdo / rodapé

Cada tela começa dividindo a área total em faixas verticais (`src/ui.rs:13`):

```rust
let [header, sbar, content, footer] = Layout::vertical([
    Constraint::Length(1),      // título
    Constraint::Length(1),      // barra de status
    Constraint::Fill(1),        // conteúdo (resto da tela)
    Constraint::Length(1),      // rodapé com atalhos
]).areas(frame.area());

frame.render_widget(Paragraph::new(" Redmine TUI ").style(...).centered(), header);
```

Dicas:

- `Constraint::Fill(1)` para a área que sobra; `Length` para faixas fixas.
- A **barra de status** (dados dinâmicos) e o **rodapé** (atalhos conforme o modo) são `Paragraph`s simples re-renderizados a cada frame.
- O rodapé muda com o contexto — um `match (mode, screen, focus)` produz a hint certa (`src/ui.rs:59`).

## Telas de conteúdo: `match app.screen`

O conteúdo muda por tela; o resto (header/status/footer) permanece igual:

```rust
match app.screen {
    Screen::IssueList => render_list(frame, content, app),
    Screen::IssueDetail => {
        let [la, ma, ra] = Layout::horizontal([
            Constraint::Percentage(28), Constraint::Percentage(37), Constraint::Percentage(35),
        ]).areas(content);
        render_list(frame, la, app);   // mesma lista reaproveitada
        render_mid(frame, ma, app);
        render_right(frame, ra, app);
    }
    Screen::GerenciaRelatorios => render_gerencia_relatorios(frame, content, app),
    Screen::GerenciaIndicacoes => render_gerencia_indicacoes(frame, content, app),
}
```

Note o **reuso**: a lista de issues é a mesma função em tela cheia ou em 28% da largura no detalhe.

## Lista: `ListState` + índices filtrados

```rust
let items: Vec<ListItem> = app.filtered_indices.iter()
    .map(|&i| &app.issues[i])          // índice filtrado → item real
    .map(|iss| ListItem::new(Line::styled(
        format!(" #{} {} [{}]", iss.id, iss.subject, iss.status_name),
        Style::new().fg(t.val).bg(bg),       // zebra: normal_row / alt_row
    )))
    .collect();

frame.render_stateful_widget(
    List::new(items)
        .highlight_style(Style::new().bg(t.sel_bg).add_modifier(Modifier::BOLD))
        .highlight_symbol(" ▸"),
    inner, &mut app.list_state,
);
```

- `ListState` é mantido no `App`, não recriado — a seleção sobrevive a frames.
- As linhas com zebra usam `i % 2`; a seleção usa `highlight_style` + `highlight_symbol`.
- Estados vazios têm mensagem própria (`" Cache vazio. r p/ sync."`) antes de renderizar.

## Scroll seguro: sempre clamp com `saturating_sub`

**Nunca** deixe o scroll ultrapassar o conteúdo — senão a tela "pula" ou mostra lixo. O padrão do projeto:

```rust
// Roda o cursor para a faixa visível: clamp ao conteúdo real
let max_scroll = lines.len().saturating_sub(inner.height as usize);
app.right_scroll = app.right_scroll.min(max_scroll);
let visible: Vec<Line> = lines.iter()
    .skip(app.right_scroll)
    .take(inner.height as usize)
    .cloned()
    .collect();
frame.render_widget(Paragraph::new(Text::from(visible)).wrap(Wrap { trim: false }), inner);
```

E o handler de scroll também faz clamp na origem (`src/app.rs:323`):

```rust
pub fn scroll_detail(&mut self, delta: isize) {
    let max = ...; // altura estimada do conteúdo
    let new = (self.detail_scroll as isize + delta).max(0).min(max as isize - 10).max(0) as usize;
    self.detail_scroll = new;
}
```

> Use `.saturating_sub()` para larguras/alturas (`inner.width.saturating_sub(3)`) e evite subtrações que estouram em terminais minúsculos.

## Popups centrais

Padrão reutilizável: `Clear` para apagar o que estava embaixo + `Block` com borda redonda, centralizado por porcentagem (`src/ui.rs:287`):

```rust
fn popup(frame: &mut Frame, area: Rect, title: &str, pct_x: u16, pct_y: u16, theme: &Theme) -> (Rect, Rect) {
    let pa = center(pct_x, pct_y, area);
    let block = Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::new().bg(theme.popup_bg))
        .border_style(Style::new().fg(theme.border));
    let inner = block.inner(pa);
    frame.render_widget(Clear, pa);      // limpa atrás do popup
    frame.render_widget(block, pa);
    (pa, inner)
}

fn center(pct_x: u16, pct_y: u16, area: Rect) -> Rect {
    let p = Layout::vertical([Constraint::Fill(1), Constraint::Percentage(pct_y), Constraint::Fill(1)]).split(area)[1];
    Layout::horizontal([Constraint::Fill(1), Constraint::Percentage(pct_x), Constraint::Fill(1)]).split(p)[1]
}
```

`Clear` é **obrigatório** — sem ele o popup "vaza" sobre o conteúdo anterior. O render principal desenha popups **depois** da tela base (`src/ui.rs:84`), sobrepondo por cima.

### Formulários dentro do popup

Campos empilhados com `Layout::vertical` de `Length(3)` + rótulo fixo e campo `Fill(1)`:

```rust
let ch = Layout::vertical([
    Constraint::Length(3), Constraint::Length(3),
    Constraint::Length(3), Constraint::Length(3), Constraint::Fill(1),
]).split(inner);

let row0 = Layout::horizontal([Constraint::Length(8), Constraint::Fill(1)]).split(ch[0]);
frame.render_widget(Paragraph::new(" Data:").style(Style::new().fg(t.label)), row0[0]);
frame.render_widget(&app.input_date, row0[1]);
```

- Foco visual via "▶" no rótulo (`"▶ Título:"` vs `"  Título:"`) — mais barato e claro que mudar cor.
- Seletores de lista (projeto, prioridade, atividade) são `Paragraph` com nome + hint `(←/→)` — evitam um widget `List` extra para um único valor.

## Markdown na descrição

`src/md.rs` converte markdown (`pulldown-cmark`) em `Vec<Line<'static>>` estilizado: **negrito/italico/heading/código** viram `Modifier`s e cores do tema; blocos de código e quebras viram linhas. O render só pega o recorte visível:

```rust
let scroll = app.detail_scroll.min(desc.len().saturating_sub(1));
let visible: Vec<Line> = desc.iter().skip(scroll).take(rest.height as usize).cloned().collect();
frame.render_widget(Paragraph::new(Text::from(visible)).wrap(Wrap { trim: false }), rest);
```

> `Text::from(Vec<Line>)` aceita linhas estilizadas — ideal para conteúdo markdown/rich.

## Tema claro/escuro

`theme.rs` define um `struct Theme` com todas as cores. A alternância (`l`) troca a instância no `App` (`src/app.rs:478`):

```rust
pub fn toggle_theme(&mut self) {
    self.theme_is_dark = !self.theme_is_dark;
    self.theme = if self.theme_is_dark { Theme::dark() } else { Theme::light() };
    self.input.set_placeholder_color(self.theme.placeholder);
    // ...
}
```

- **Nenhum widget usa `Color` hardcoded** — todos leem `t.val`, `t.muted`, `t.error`, etc.
- O tema é `Clone` e clonado no início do `render` (`let t = app.theme.clone();`) para não emprestar mutavelmente do `App` durante o draw.
- Cores `Rgb` para 24-bit; caia com `Color::DarkGray`/`Green` básicas se o terminal for limitado.

## Half-blocks: preview de imagem (bônus)

`src/event/mod.rs:594` (`image_to_halfblocks`) transforma uma imagem em arte ASCII colorida usando o caractere `▀` com `fg`/`bg` = dois pixels: uma célula = 2 linhas verticais de imagem. Útil para preview de anexos sem lib gráfica.

## Armadilhas comuns

| Pitfall | Solução |
|---|---|
| Popup sem `Clear` | Sempre `frame.render_widget(Clear, area)` antes do block |
| Scroll sem limite | Clamp com `.min()`/`.saturating_sub()` em **todos** os caminhos de scroll |
| Área negativa em terminais pequenos | `width.saturating_sub(n)` e `Layout` com `Fill` no fim |
| Estado da lista perdido | Manter `ListState` no `App`, nunca recriar a cada frame |
| Cor hardcoded | Tudo via `Theme`; sem `Color::Red` espalhado |

## Próximo passo

Veja [03-eventos.md](./03-eventos.md) para o roteamento de teclado e o padrão de confirmação.
