# `katu-tui` — Interface de Terminal

**Épico:** E10 · **Crate:** `crates/katu-tui`

A **interface de terminal** do katu (uma das duas superfícies, com a CLI — G7). UX de codificação
sobre `ratatui` 0.30 + `crossterm` 0.29, com estado central e render puro.

---

## 1. Visão Geral

O crate `katu-tui` é a **UI de terminal** do katu. Responsável por:

- **Estado central** (`App`): todo o estado da UI vive num único struct
- **Render puro** (`ui::render`): desenha a partir do `App`; nunca faz I/O
- **Keymap puro** (`action::map_key`): traduz teclas em `Action`s puras
- **Painel de atividade efémero**: stream do modelo e tools em curso, fora do log
- **Challenge-and-response**: aprovações humanas com checklist + justificação
- **Mini-menus** (`/model`, `/thinking`): seleção de modelo e grau de pensamento
- **Transcrição durável**: vista read-only do log (`.katu/transcript.md`)
- **Lixeira**: listar, restaurar e esvaziar (com challenge)

**Fronteira:** Sem servidor, sem rede (G7). Não depende de `katu-tools`/`katu-providers`/`knudge`
(firewall `layers.toml`): a UI só vê o estado puro e emite `Command`s que a borda executa.

---

## 2. Arquitetura

### 2.1 Composição

```
┌─────────────────────────────────────────────────────────────────┐
│                      katu-tui (UI pura)                         │
├─────────────────────────────────────────────────────────────────┤
│  App (estado central)  │  render (puro)  │  map_key (puro)     │
├─────────────────────────────────────────────────────────────────┤
│  Command (UI → borda)  │  Update (borda → UI)                   │
├─────────────────────────────────────────────────────────────────┤
│  Handler (trait)  │  Painter (streaming efémero)                │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      katu (borda)                               │
│  AgentHandler implementa Handler + ActivitySink                 │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2 Módulos

| Módulo | Responsabilidade |
|--------|------------------|
| `lib.rs` | Reexportações públicas |
| `run.rs` | Loop de eventos, restauro do terminal, `Painter` |
| `app.rs` | Estado central (`App`) |
| `app/update.rs` | Injeção de `Update`s no `App` |
| `app/menu.rs` | Comandos `/` e mini-menus |
| `app/panel.rs` | Painel de atividade efémero |
| `app/viewer.rs` | Accessores da vista da transcrição |
| `action.rs` | Keymap puro (`Mode`, `Action`, `map_key`) |
| `ui.rs` | Render puro |
| `live.rs` | Eventos efémeros do painel de atividade |
| `approval.rs` | Challenge-and-response (checklist + justificação) |
| `approval/view.rs` | Render do challenge |
| `controls.rs` | Controlos do core (modelo e grau de pensamento) |
| `menu.rs` | Mini-menus de comandos `/` |
| `overlay.rs` | Sobreposições de ajuda e mini-menu |
| `transcript.rs` | Vista da transcrição durável (read-only) |
| `trash.rs` | Vista da lixeira |
| `entry.rs` | Tipos de apresentação (`Role`, `Entry`, `Status`) |
| `message.rs` | `Command` (UI → borda) e `Update` (borda → UI) |
| `throttle.rs` | Orçamento de render (~60 fps) |
| `copy.rs` | Cópia por seleção de rato via OSC 52 |
| `layout.rs` | Utilitários de layout (painel centrado) |

---

## 3. Módulos em Detalhe

### 3.1 Estado Central (`src/app.rs`)

**`App`** detém todo o estado da UI:

```rust
pub struct App {
    input: String,              // Linha de mensagem
    citations: Vec<String>,     // Caminhos citados com `@<path>`
    plan: bool,                 // Modo de planeamento
    steering: String,           // Buffer de steering em curso
    transcript: Vec<Entry>,     // Conversa (durável na sessão)
    mode: Mode,                 // Modo de entrada
    status: Status,             // Estado da barra
    phase: String,              // Fase do kernel
    streaming: String,          // Texto do modelo em curso (efémero)
    thinking: String,           // Raciocínio em curso (efémero)
    live: Vec<String>,          // Eventos discretos do painel (efémero)
    controls: Controls,         // Modelo e grau de pensamento
    menu: Option<Menu>,         // Mini-menu aberto
    pending_menu: Option<MenuKind>,
    thinking_options: Vec<Thinking>,
    trash: Trash,               // Vista da lixeira
    viewer: TranscriptView,     // Vista da transcrição
    next_action: Option<String>,// Próxima ação do checkpoint
    usage: Option<String>,      // Uso/costo do último turno
    scroll: u16,                // Deslocamento da conversa
    quit: bool,                 // Sair
}
```

**Princípio:** Nenhuma variável de UI paralela. Fase e pendências vêm do `App`, que a borda
alimenta a partir do `State` do kernel.

### 3.2 Keymap Puro (`src/action.rs`)

**`map_key(KeyEvent, Mode) -> Option<Action>`** é uma função pura:

- Traduz teclas em `Action`s puras
- Nenhuma lógica de UI no handler de eventos
- Testável por modo, sem terminal

**Modos:**

| Modo | Descrição |
|------|-----------|
| `Normal` | Navegação: `q` sai, `i`/Enter escreve, `/` inicia comando, `?` abre ajuda |
| `Insert` | Edição da linha de mensagem |
| `Confirm` | Confirmação de ação destrutiva (`y`/`n`) |
| `Trash` | Navegação na vista da lixeira |
| `Transcript` | Leitura da transcrição durável |
| `Help` | Sobreposição de ajuda |
| `Menu` | Mini-menu de seleção |

**Ações (TUI v2):**

- `EnterInsert`, `StartCommand`, `LeaveInsert`
- `Insert(char)`, `Backspace`, `Submit`, `Cancel`, `Confirm`
- `ScrollUp`, `ScrollDown`
- `OpenHelp`, `MenuUp`, `MenuDown`, `MenuConfirm`
- `OpenTrash`, `OpenTranscript`, `CloseOverlay`
- `TrashUp`, `TrashDown`, `Restore`, `EmptyTrash`
- `Compact`, `Verify`, `Quit`

**Nota:** Os antigos atalhos de um toque (`m`, `t`, `l`, `T`, `c`, `v`) foram **removidos** na
TUI v2. Os comandos vivem na linha de mensagem (`/model`, `/thinking`, …).

### 3.3 Render Puro (`src/ui.rs`)

**`render(frame: &mut Frame<'_>, app: &App)`** desenha um quadro completo:

```
┌─────────────────────────────────────────────────────────────────┐
│ header (fase + pendência + modelo + pensamento)                 │
├─────────────────────────────────────────────────────────────────┤
│ conversa (scroll)  │ atividade (efémero)                       │
├─────────────────────────────────────────────────────────────────┤
│ input (mensagem/steering)                                       │
├─────────────────────────────────────────────────────────────────┤
│ footer (status)                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Tetos (E10-T03):**
- `MAX_TRANSCRIPT_ENTRIES = 200` entradas por quadro
- `MAX_ACTIVITY_LINES = 100` linhas por quadro
- `STREAM_TAIL_BYTES = 8 KiB` do stream efémero

### 3.4 Painel de Atividade Efémero (`src/live.rs`)

**Eventos efémeros** (nunca entram no log nem no transcript):

| Evento | Descrição |
|--------|-----------|
| `Text(String)` | Delta de texto do modelo |
| `Thinking(String)` | Delta de raciocínio |
| `Tool { name, args }` | Tool pedida (com argumentos crus) |
| `ToolDone(String)` | Tool concluída |
| `Refused { rule, evidence }` | Recusa de política |
| `Unavailable { control }` | Tool indisponível |
| `Clear` | Limpa o painel (fim de turno) |

**Contraste com o transcript:**
- **Live**: efémero, só o painel de atividade, cauda de 8 KiB
- **Transcript**: durável, `.katu/transcript.md`, projeção do log

### 3.5 Challenge-and-Response (`src/approval.rs`)

**Aprovação nunca é um *rubber-stamp***:

1. **Checklist** (3 perguntas positivas, todas obrigatórias):
   - "Li a regra e a evidência."
   - "Quero que esta operação corra."
   - "A autorização é só para este alvo."

2. **Justificação** escrita (obrigatória)

3. **Assinatura** (`granted_by`)

**Estado puro do challenge:**

```rust
pub struct Challenge {
    prompt: ChallengePrompt,
    checked: Vec<bool>,
    reason: String,
    focus: Focus,
    cursor: usize,
}
```

**Teclas:**
- `↑/↓`: navegar no checklist
- `Tab`: alternar foco (checklist ↔ justificação)
- `Espaço`: marcar pergunta
- `Enter`: submeter (só se completo)
- `Esc`/`Ctrl-C`: cancelar (fail-closed)

### 3.6 Controlos (`src/controls.rs`)

**Só o utilizador** muda modelo/pensamento (o agente nunca se auto-escala):

```rust
pub(crate) struct Controls {
    reasoning: Thinking,
    models: Vec<String>,
    index: usize,
}
```

**Métodos:**
- `set_models(models)`: publica a lista de modelos
- `set_model(model)`: seleciona um modelo
- `set_thinking(thinking)`: define o grau de pensamento

### 3.7 Mini-menus (`src/menu.rs`)

**Comandos `/`:**

| Comando | Descrição |
|---------|-----------|
| `/model` | Escolhe o modelo (menu) |
| `/thinking` | Escolhe o grau de pensamento (menu) |
| `/compact` | Liga/desliga a compactação do histórico |
| `/plan` | Liga/desliga o modo de planeamento |
| `/verify` | Corre o gate de verificação |
| `/trash` | Abre a lixeira |
| `/transcript` | Abre a transcrição durável |
| `/help` | Ajuda |
| `/quit` | Sai da UI |

**Padrões da linha de mensagem:**

| Padrão | Descrição |
|--------|-----------|
| `!<cmd>` | Shell pela política (bloqueado em `/plan`) |
| `@<path>` | Cita um caminho (só o caminho) |
| `/skill:<nome>` | Força o carregamento de uma skill |

### 3.8 Transcrição Durável (`src/transcript.rs`)

**Vista read-only** da transcrição:

- A transcrição é a projeção do **log** (durável)
- A borda lê o ficheiro `.katu/transcript.md` e injeta-a via `Update::Transcript`
- Esta vista só a apresenta — o painel de atividade continua efémero e separado (§50.3)

**Estado:**

```rust
pub(crate) struct TranscriptView {
    lines: Vec<String>,
    scroll: u16,
    open: bool,
}
```

### 3.9 Lixeira (`src/trash.rs`)

**Vista da lixeira** (recuperável):

- Lista `.katu/trash`
- Restaurar nunca apaga nada e não passa pela política (E06-T09)
- Esvaziar é **destrutivo**: exige challenge

**Estado:**

```rust
pub(crate) struct Trash {
    items: Vec<TrashEntry>,
    index: usize,
    open: bool,
}
```

### 3.10 Throttle (`src/throttle.rs`)

**Orçamento de render** (~60 fps):

```rust
pub(crate) const FRAME_INTERVAL_MS: u64 = 16;

pub(crate) struct Throttle<'c> {
    clock: &'c dyn Clock,
    interval_ms: u64,
    last_ms: AtomicU64,
    pending: AtomicBool,
}
```

**Métodos:**
- `due()`: `true` se já passou o intervalo (ou há um quadro forçado)
- `request()`: força o próximo quadro (mudança estrutural)

**Determinismo:** Usa a porta `Clock` (nada de `Instant::now`).

### 3.11 Cópia (`src/copy.rs`)

**Cópia por seleção de rato via OSC 52:**

- A seleção é acompanhada em coordenadas de ecrã
- Ao soltar o botão, o texto é lido do buffer renderizado
- Escrito no clipboard via OSC 52 (`ESC ] 52 ; c ; base64 BEL`)
- **Sem dependência nova** (terminal sem suporte simplesmente ignora a sequência)

### 3.12 Loop de Eventos (`src/run.rs`)

**`try_init`/`restore`:**
- Liga o modo cru, o ecrã alternativo
- **Panic hook** que restaura o terminal
- **Guarda RAII** (`Drop`) restaura em qualquer saída normal

**Loop de eventos:**
- `poll(50 ms)` mantém o CPU baixo sem parecer travado
- Traduz teclas em `Action`s via `map_key`
- Delega em `App::apply_action`
- Executa `Command`s via `Handler`

**`Painter`:**
- Recebe eventos **efémeros** e redesenha no terminal
- Nunca escreve no log nem no transcript
- **Cancelamento**: `Esc` durante o turno
- **Steering**: teclas alimentam o buffer; `Enter` enfileira

---

## 4. Abordagens de Engenharia

### 4.1 Estado Central + Render Puro

**Princípio:** Tudo vive em `App`; o render só lê dele.

**Benefício:** Testabilidade (sem terminal) e previsibilidade (sem efeitos colaterais).

### 4.2 Keymap Puro

**Princípio:** `map_key(KeyEvent, Mode) -> Option<Action>` é uma função pura.

**Benefício:** Testável por modo, sem terminal. Nenhuma lógica de UI no handler de eventos.

### 4.3 Painel de Atividade Efémero

**Princípio:** O stream do modelo e as tools em curso aparecem ao vivo, **fora** do log e do
transcript.

**Benefício:** O durável é o próprio log de sessão, reconstruível; o live nunca entra na
transcrição (§50.3).

### 4.4 Challenge-and-Response

**Princípio:** Uma aprovação **nunca** é um *rubber-stamp*: exige responder a perguntas positivas
(checklist) **e** escrever uma justificação.

**Benefício:** Evita acidentes por *default*; só um humano assina.

### 4.5 Throttle por Clock

**Princípio:** O `ratatui` já faz render diferencial; aqui governa-se **quando** desenhar.

**Benefício:** Um fluxo de deltas não provoca um quadro por delta. Determinismo (usa a porta
`Clock`).

### 4.6 Cancelamento Cooperativo

**Princípio:** `Esc` durante o turno pede cancelamento; o turno fecha limpo.

**Benefício:** O utilizador pode interromper um turno sem deixar o log inconsistente.

### 4.7 Steering

**Princípio:** O que se escreve aparece na linha de entrada e o `Enter` enfileira um prompt (FIFO)
que a borda aplica no passo seguinte.

**Benefício:** O utilizador pode redirecionar o turno sem interrompê-lo.

### 4.8 Cópia via OSC 52

**Princípio:** A seleção é acompanhada em coordenadas de ecrã; ao soltar, o texto é lido do buffer
renderizado e escrito no clipboard via OSC 52.

**Benefício:** Sem dependência nova; um terminal sem suporte simplesmente ignora a sequência.

### 4.9 Panic-Safe

**Princípio:** Restaura o terminal em qualquer saída (guarda RAII + panic hook do `try_init`).

**Benefício:** O terminal nunca fica em modo cru após um pânico.

### 4.10 Comandos `/` na Linha de Mensagem

**Princípio:** Os comandos vivem na linha de mensagem (`/model`, `/thinking`, …); os antigos
atalhos de um toque foram **removidos**.

**Benefício:** Superfície mais limpa; descobribilidade via ajuda (`?`).

---

## 5. Gaps, Flags e Pendências

### 5.1 Gaps Conhecidos

| Gap | Descrição | Estado |
|-----|-----------|--------|
| Executor em background | Turno síncrono | Trabalho futuro |
| Benchmark por frame | `xtask bench-render`/`gate:render` (E15-T01) | Pendente |
| Verificação de zero alocações | Hot path (E18-T10) | Pendente |

### 5.2 Limitações Conhecidas

| Limitação | Descrição |
|-----------|-----------|
| `MAX_TRANSCRIPT_ENTRIES = 200` | Teto de entradas por quadro |
| `MAX_ACTIVITY_LINES = 100` | Teto de linhas por quadro |
| `STREAM_TAIL_BYTES = 8 KiB` | Cauda do stream efémero |
| `FRAME_INTERVAL_MS = 16` | ~60 fps |
| `POLL_MILLIS = 50` | Período de sondagem de eventos |

### 5.3 Dívida Técnica

| Item | Descrição |
|------|-----------|
| `Controls` | Só o utilizador muda; a borda aplica ao **próximo** turno |
| `pending_menu` | Menu a abrir quando a borda publicar as capacidades do novo modelo |

---

## 6. Testes

### 6.1 Suíte de Testes

| Módulo | Testes |
|--------|--------|
| `action` | Keymap puro por modo |
| `approval` | Challenge (checklist, justificação, cancelamento) |
| `run` | Mapa de teclas do turno (só `Esc` cancela) |
| `throttle` | Intervalo, forçado |
| `live` | `trim_tail` (fronteira de caractere) |
| `app` | Estado central (typing, submitting, updates, live, usage) |
| `render` | Render num backend de teste (header, menus, overlays) |

### 6.2 Exemplos de Teste

```rust
#[test]
fn typing_then_submitting_emits_a_command_and_echoes_the_user() {
    let mut app = App::new();
    app.apply_action(Action::EnterInsert);
    for character in "olá".chars() {
        app.apply_action(Action::Insert(character));
    }
    assert_eq!(app.input(), "olá");
    let command = app.apply_action(Action::Submit);
    assert_eq!(command, Some(Command::Submit("olá".to_string())));
    assert!(app.input().is_empty());
    assert!(app.pending());
    assert_eq!(app.status(), &Status::Working);
    assert_eq!(app.transcript().first().map(|e| e.role), Some(Role::User));
}
```

---

## 7. Referências

- **MODULE.md:** [`crates/katu-tui/MODULE.md`](../../../crates/katu-tui/MODULE.md)
- **katu (borda):** [`wiki/crates/katu/CRATE.md`](../katu/CRATE.md)
