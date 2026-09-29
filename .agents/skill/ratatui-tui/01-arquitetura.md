# 01 — Arquitetura: como organizar o projeto

A organização dos módulos é o que separa uma TUI frágil de uma extensível. O padrão usado no `my-redmine` (e recomendado) é **uma linha por responsabilidade**, com o estado centralizado em `App`.

## Mapa de módulos (caso real)

```
src/
├── main.rs          — runtime tokio, canais mpsc, loop ratatui
├── app.rs           — struct App (todo o estado) + métodos de navegação
├── ui.rs            — renderização pura do estado (widgets, popups)
├── event/
│   ├── mod.rs       — polling, roteamento por modo, handlers de input/confirm
│   ├── list.rs      — teclado da tela de lista
│   ├── detail.rs    — teclado do detalhe
│   └── gerencia.rs  — teclado das telas de gerência
├── dispatch.rs      — rota ApiAction → chamada async real
├── handler.rs       — aplica ApiResult no estado + grava cache
├── api.rs           — chamadas à API Redmine (via wrapper tipado)
├── gerencia.rs      — cliente HTTP da API Gerência (reqwest)
├── cache.rs         — SQLite (WAL, versionado)
├── config.rs        — config.toml + env vars
├── input.rs         — campo de texto (ratatui-textarea) com confirm/cancel
├── md.rs            — markdown → Linhas estilizadas
├── theme.rs         — paleta de cores (light/dark)
└── types.rs         — structs, enums, constantes, formatação
```

## O estado central: `App`

Tudo que a UI precisa mora numa única struct (`src/app.rs:13`). Renderização e eventos apenas leem/mutam ela — nunca há estado espalhado entre widgets.

```rust
pub struct App {
    pub should_quit: bool,
    pub screen: Screen,          // qual tela está ativa
    pub mode: Mode,              // Normal | Insert | Confirm
    pub input_target: InputTarget, // o que o input atual significa
    pub issues: Vec<IssueSummary>,
    pub filtered_indices: Vec<usize>, // índice da lista = posição no filtro
    pub list_state: ListState,   // seleção do ratatui
    pub detail: Option<IssueDetail>,
    pub detail_scroll: usize,
    pub status_message: Option<String>,
    pub status_kind: StatusKind,
    pub pending_actions: Vec<ApiAction>, // ações enfileiradas p/ executor
    pub confirm_action: Option<String>,
    pub confirm_callback: Option<ConfirmCallback>,
    pub theme: Theme,
    pub theme_is_dark: bool,
    // ... telas de gerência, inputs, seletores, etc.
}
```

Pontos-chave:

- **Enums de navegação tipam o comportamento** (abaixo).
- **`filtered_indices` separa a lista exibida da lista completa** — `ListState` navega sobre os índices filtrados; o resto da UI consulta `self.issues[i]`. Filtros nunca mutam os dados.
- **`pending_actions` é o "buffer de saída"** — handlers de teclado enfileiram aqui; o loop drena e envia ao executor (ver [04](./04-integracao-async.md)).
- **`confirm_*`** guardam a confirmação pendente (popup y/n) antes de qualquer escrita.

### Enums de estado: `Screen`, `Mode`, `InputTarget`

Três enums pequenos evitam um inferno de `if`s soltos:

```rust
// src/types.rs:63
pub enum Screen { IssueList, IssueDetail, GerenciaRelatorios, GerenciaIndicacoes }

// src/types.rs:69
pub enum Mode { Normal, Insert, Confirm }

// src/types.rs:72 — o que o campo de texto está editando agora
pub enum InputTarget {
    None, AddNote, EditNotaImplementacao,
    LogTimeDate, LogTimeHours, LogTimeComments,
    AttachFilePreview, AttachFileNote,
    FilterIssues, FilterLinkIssues, SelectStatus,
    GerenciaNewRelatorio, GerenciaNewIndicacao,
    GerenciaLinkIssues,
    EditIndicacaoHorasPopup, EditIndicacaoStatusSel,
    EditIndicacaoNotaImpl, EditIndicacaoNotas,
}
```

O `match app.mode` no `event/mod.rs:24` roteia todo o teclado em três grandes blocos (Insert/Confirm/Normal). O `match app.screen` dentro de Normal delega para o handler da tela. **Adicionar uma tela nova = 1 variante de `Screen` + 1 função de render + 1 handler de teclado.**

## Regras de organização que evitam bugs

| Regra | Por quê |
|---|---|
| `ui.rs` **não** chama API/cache | Render vira função pura do estado — rápido e determinístico |
| `event/` **não** faz I/O | Só enfileira `ApiAction`; o resultado volta pelo canal |
| Métodos de estado ficam em `App` (ex.: `next()`, `apply_filter`) | Lógica de navegação testável sem TUI |
| Tipos "feios" da API viram **Display types** em `types.rs` | A UI nunca toca no JSON cru; datas viram `Option<NaiveDate>` |
| Formatação (`format_hours`, `fmt_date`) isolada | Testes unitários sem depender de render |

## Tipos tipados (Rust 2024) — `types.rs`

O projeto investe em **type safety** para o estado:

- `RedmineId` — newtype com `Display`, `FromStr`, `Hash`, `Eq`, `Ord` (evita misturar id de issue com id de status).
- Datas `Option<NaiveDate>` / `Option<DateTime<Utc>>` — nunca strings soltas no estado.
- Deserialização defensiva com `#[serde(default, deserialize_with = ...)]` para campos ausentes/inválidos não quebrarem o cache (detalhes no [06](./06-estabilidade-seguranca.md)).
- Enums com serde (`Periodo` com `#[serde(rename_all = "snake_case")]`) para wire format controlado.

## Padrão de navegação: `ListState` + índices filtrados

Navegação circular e à prova de lista vazia, concentrada em métodos de `App` (`src/app.rs:297`):

```rust
pub fn next(&mut self) {
    let len = self.filtered_indices.len();
    if len == 0 { return; }                    // nunca panica em lista vazia
    let i = self.list_state.selected()
        .map_or(0, |i| if i >= len - 1 { 0 } else { i + 1 }); // circular
    self.list_state.select(Some(i));
}

pub fn selected_issue(&self) -> Option<&IssueSummary> {
    self.selected_issue_idx().and_then(|i| self.issues.get(i))
}
```

Todo acesso ao item selecionado passa por `filtered_indices → issues`, sempre com `.get()` — nunca por índice direto desprotegido.

## Próximo passo

Veja [02-renderizacao.md](./02-renderizacao.md) para Layout, List, scroll e popups.
