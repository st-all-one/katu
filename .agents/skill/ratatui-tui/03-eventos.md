# 03 — Eventos: teclado, modos e confirmações

A interação no `my-redmine` (`src/event/`) segue um padrão de **máquina de estados com 3 modos** (Normal / Insert / Confirm) e **handlers por tela**. Toda operação de escrita passa por uma confirmação explícita (`y`/`n`) antes de virar ação de API.

## Polling com timeout — nunca bloqueie o loop

O loop usa `crossterm::event::poll` com timeout curto. Isso deixa o loop "vivo" para processar resultados assíncronos mesmo sem tecla pressionada:

```rust
// src/event/mod.rs:17
const POLL_MS: u64 = 100;

pub fn poll_events(app: &mut App) -> Result<bool> {
    if !event::poll(std::time::Duration::from_millis(POLL_MS))? { return Ok(false); }
    let Event::Key(key) = event::read()? else { return Ok(false); };
    if key.kind != KeyEventKind::Press { return Ok(false); }  // ignora Release/Repeat
    match app.mode {
        Mode::Insert => handle_insert(app, key),
        Mode::Confirm => handle_confirm(app, key.code),
        Mode::Normal => handle_normal(app, key.code),
    }
    Ok(true)
}
```

Pontos importantes:

- **Filtre `KeyEventKind::Press`** — sem isso, segurar uma tecla dispara eventos de `Repeat` que quebram a navegação.
- Timeout de 100ms ≈ 10 polls/seg; baixo o suficiente para parecer instantâneo e alto o suficiente para não comer CPU.
- `Mode` é o primeiro discriminante; depois `Screen` delega ao handler específico (`handle_normal` → `list::handle_list` / `detail::handle_detail` / `gerencia::handle_gerencia`).

## Modos de entrada

| Modo | O que faz | Entra quando | Sai quando |
|---|---|---|---|
| `Normal` | Navegação, atalhos de tela | padrão | `enter_insert` / `request_confirm` |
| `Insert` | Campo de texto ativo (`ratatui-textarea`) | `app.enter_insert(target)` | `InputResult::Cancel` / `Confirmed` |
| `Confirm` | Popup y/n antes de ação de escrita | `app.request_confirm(...)` | `y`/`Enter` ou `n`/`Esc` |

O `InputTarget` diz **o que** o campo de texto significa agora (nota, data, filtro, etc.) — é ele que determina o fluxo de confirmação no `handle_insert`.

## O campo de texto: `InputField`

`src/input.rs` embrulha o `ratatui-textarea` e devolve um resultado tri-state:

```rust
pub enum InputResult { Confirmed, Cancel, Editing }

pub fn handle_key(&mut self, key: KeyCode, modifiers: KeyModifiers) -> InputResult {
    // Confirmar: Ctrl+Enter ou Alt+Enter (compatibilidade cross-terminal)
    let is_confirm = key == KeyCode::Enter
        && (modifiers.contains(KeyModifiers::CONTROL) || modifiers.contains(KeyModifiers::ALT));
    if is_confirm {
        let val = self.value();
        if val.trim().is_empty() { return InputResult::Cancel; }
        return InputResult::Confirmed;
    }
    if key == KeyCode::Enter {
        // Enter sem modifier = quebra de linha (campos multi-linha)
        self.textarea.input(...);
        return InputResult::Editing;
    }
    if key == KeyCode::Esc { return InputResult::Cancel; }
    self.textarea.input(...);
    InputResult::Editing
}
```

Regras ergonômicas que o projeto adota:

- **Enter** = quebra de linha (descrições/notas multi-linha).
- **Ctrl+Enter** / **Alt+Enter** = confirmar (campos longos onde Alt+Enter evita conflito com emuladores que engolem Ctrl+Enter).
- **Esc** = cancelar e voltar a `Normal`.
- Campo vazio + confirmar = **cancelar** (sem popup desnecessário).

O `handle_insert` por target decide o que fazer com cada `InputResult` — por exemplo, para LogTime o fluxo é **data → horas → comentário → confirmação** (`src/event/mod.rs:35`):

```rust
InputTarget::LogTimeDate => {
    if code == KeyCode::Enter { app.enter_insert(InputTarget::LogTimeHours); return; }
    match app.input_date.handle_key(code, modifiers) {
        InputResult::Confirmed => app.enter_insert(InputTarget::LogTimeHours),
        InputResult::Cancel => { reset(app); app.mode = Mode::Normal; ... }
        InputResult::Editing => {}
    }
}
```

## Validação antes de confirmar

O handler valida e dá **feedback no status bar** antes de abrir a confirmação — o usuário descobre o problema cedo, não depois do y:

```rust
// src/event/mod.rs:67 (LogTimeComments confirmado)
if let Ok(hours) = hs.parse::<f64>() {
    if hours > 0.0 && !date.is_empty() {
        if let Some(warning) = app.has_issue_time_conflict(id, &date, hours) {
            app.set_status(StatusKind::Error, warning);
            app.mode = Mode::Normal; ...; return;  // bloqueia
        }
        app.request_confirm(format!("Lançar {hours}h em #{id} em {date}?"),
            ConfirmCallback::LogTime { issue_id: id, spent_on: date, hours, activity_id: act, comments },
            None);
    } else {
        app.set_status(StatusKind::Error, "Horas > 0 e data obrigatórios".into());
    }
}
```

Padrões extraídos:

- Input inválido → `set_status(StatusKind::Error, msg)` + volta a `Normal` (o usuário relê o erro no status bar).
- Regra de negócio quebrada (ex.: >8h no dia) → bloqueia com aviso.
- Só depois de tudo válido → `request_confirm` com descrição humana.

## Confirmações: `request_confirm` + `ConfirmCallback`

`App` guarda a confirmação pendente como enum tipado (`src/app.rs:104`):

```rust
pub enum ConfirmCallback {
    SyncIssues,
    AddNote { issue_id: RedmineId, notes: String },
    EditNotaImplementacao { issue_id: RedmineId, value: String },
    LogTime { issue_id, spent_on, hours, activity_id, comments },
    UpdateStatus { issue_id, status_id, nota_implementacao: Option<String> },
    UploadAttachment { issue_id, file_path, description },
    GerenciaSendRelatorio { descricao, titulo, periodo },
    GerenciaSendIndicacao { titulo, projeto, descricao, prioridade },
    GerenciaLinkToIssue { issue_id: RedmineId, indicacao: GerenciaIndicacao },
}
```

E o fluxo (`src/event/mod.rs:450`):

```rust
fn handle_confirm(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Char('y') | KeyCode::Enter => {
            let cb = app.confirm_callback.take();
            app.confirm_action = None; app.confirm_detail = None;
            app.mode = Mode::Normal; ...; reset(app);
            if let Some(cb) = cb {
                match cb {
                    ConfirmCallback::SyncIssues => {
                        app.queue_action(ApiAction::FetchIssues);
                        app.queue_action(ApiAction::FetchActivities);
                    }
                    ConfirmCallback::LogTime { issue_id, spent_on, hours, activity_id, comments } => {
                        app.pending_log_time = Some(LogTimeProbe { issue_id, date: spent_on.clone(), hours });
                        app.queue_action(ApiAction::LogTime { issue_id, spent_on, hours, activity_id, comments });
                    }
                    // ... cada callback vira uma ou mais ApiAction na fila
                }
            }
        }
        KeyCode::Char('n') | KeyCode::Esc => { /* descarta tudo, volta a Normal */ }
        _ => {}
    }
}
```

Por que esse padrão é estável:

- **`take()` remove** o callback — não dá para confirmar a mesma ação duas vezes.
- Confirmar **só enfileira** (`queue_action`); o I/O acontece no executor (ver [04](./04-integracao-async.md)). A UI nunca espera.
- Callbacks compostos (como `GerenciaLinkToIssue`) podem enfileirar **várias** ações em ordem — nota → tempo → status → anexos.
- O popup de confirmação pode mostrar um resumo detalhado via `confirm_detail` (`build_link_confirm_detail`, `src/event/mod.rs:563`).

## Handlers por tela — exemplo

`src/event/list.rs` (tela de lista):

```rust
pub(super) fn handle_list(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Char('j') | KeyCode::Down => app.next(),
        KeyCode::Char('k') | KeyCode::Up => app.previous(),
        KeyCode::Enter => {
            if let Some(issue) = app.selected_issue() {
                if let Some(ref cache) = app.cache {
                    if let Some(detail) = cache.load_detail(issue.id) {
                        app.open_detail(detail); return;   // cache primeiro
                    }
                }
                app.queue_action(ApiAction::FetchIssueDetail(issue.id));
            }
        }
        KeyCode::Char('f') => app.enter_insert(InputTarget::FilterIssues),
        KeyCode::Char('r') => app.request_confirm("Sincronizar?".into(), ConfirmCallback::SyncIssues, None),
        KeyCode::Char('q') => app.should_quit = true,
        _ => {}
    }
}
```

Padrões:

- **Ambiguidade de tecla por contexto resolvida por `Screen`/`focus`** — ex.: `Enter` abre issue na lista, baixa anexo no foco de anexos, seleciona issue no popup de vínculo.
- **Cache-first**: abre do cache se existir; senão enfileira fetch (nunca bloqueia).
- Atalhos sempre com fallback silencioso (`_ => {}`) — tecla desconhecida não quebra o estado.

## Armadilhas comuns

| Pitfall | Solução |
|---|---|
| Não filtrar `KeyEventKind::Press` | Repetições de tecla quebram navegação/formulário |
| `event::read()` bloqueante | Sempre `poll(timeout)` antes do `read()` |
| Confirmar duas vezes | `callback.take()` no início do `y` |
| I/O dentro do handler | Handler só enfileira `ApiAction` |
| Input inválido sem feedback | `set_status(Error, ...)` + voltar a `Normal` |

## Próximo passo

Veja [04-integracao-async.md](./04-integracao-async.md) — como consumir APIs sem travar a UI.
