# 04 — Integração assíncrona: consumir APIs sem travar a UI

Este é o coração arquitetural do `my-redmine`: **todo I/O de rede roda em um executor tokio em background**, comunicando-se com a UI por dois canais `mpsc`. A UI nunca faz `await`; ela **enfileira ações** e **consome resultados** quando chegam.

## O executor em background

Em `src/main.rs:42`, dois canais `unbounded` conectam UI e executor:

```rust
let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel::<ApiAction>();
let (result_tx, mut result_rx) = tokio::sync::mpsc::unbounded_channel::<ApiResult>();

let cw = Arc::clone(&client);
let gc = Arc::new(GerenciaClient::new(...));
tokio::spawn(async move {
    while let Some(action) = action_rx.recv().await {
        let result = dispatch::dispatch(&cw, &gc, action).await;  // I/O aqui
        let _ = result_tx.send(result);
    }
});
```

- `unbounded_channel` para não bloquear o produtor (a UI).
- Um único `tokio::spawn` serializa as chamadas — **uma ação por vez**, sem corrida na API.
- `dispatch::dispatch` é um `match` gigante que roteia `ApiAction` para a função real (`src/dispatch.rs:7`).

## O contrato: `ApiAction` → `ApiResult`

Os dois enums são o "wire protocol" interno (`src/types.rs:307`):

```rust
pub enum ApiAction {
    FetchIssues,
    FetchIssueDetail(RedmineId),
    FetchTimeEntries(RedmineId),
    AddNote { issue_id: RedmineId, notes: String },
    UpdateCustomField { issue_id: RedmineId, field_id: RedmineId, value: String },
    LogTime { issue_id: RedmineId, spent_on: String, hours: f64, activity_id: RedmineId, comments: String },
    UpdateStatus { issue_id: RedmineId, status_id: RedmineId, nota_implementacao: Option<String> },
    FetchActivities,
    FetchIssueStatuses,
    DownloadAttachment { issue_id, attachment_id, filename, url },
    UploadAttachment { issue_id, file_path, description },
    FetchGerenciaRelatorios,
    SendGerenciaRelatorio { descricao, titulo, periodo },
    // ...
}

pub enum ApiResult {
    Issues(Vec<IssueSummary>),
    IssueDetail(IssueDetail),
    TimeEntries(Vec<TimeEntryDisplay>),
    Activities(Vec<IdName>),
    IssueStatuses(Vec<StatusOption>),
    Success(String),
    Error(String),
    GerenciaRelatorios(Vec<GerenciaReport>),
    // ...
}
```

Regras:

- **`ApiAction` é enriquecida** — carrega tudo que a chamada precisa (nada de buscar no estado lá no executor).
- **`ApiResult::Error(String)`** — o executor converte toda falha em mensagem amigável em pt-BR (`format!("Erro ao buscar issues: {e}")`); a UI só exibe.
- O dispatch mapeia erro/sucesso com `map_or_else` (`src/dispatch.rs:9`):

```rust
ApiAction::FetchIssues => api::fetch_issues(client).await
    .map_or_else(|e| ApiResult::Error(e), ApiResult::Issues),
```

## Consumindo no loop: `handler.rs`

`handle_result` aplica o resultado no `App`, grava cache e **encadeia re-sincronizações** (`src/handler.rs:4`):

```rust
pub fn handle_result(app: &mut App, result: ApiResult, atx: &UnboundedSender<ApiAction>) {
    match result {
        ApiResult::Issues(issues) => {
            app.set_issues(issues.clone());
            app.last_sync = Some(chrono::Utc::now());
            if let Some(ref c) = app.cache {
                if let Err(e) = c.save_issues(&issues) { tracing::warn!("cache: {e}"); }
            }
            app.set_status(StatusKind::Success, format!("Sync: {} issues", issues.len()));
        }
        ApiResult::IssueDetail(detail) => {
            let id = detail.id;
            if let Some(ref c) = app.cache { if let Err(e) = c.save_detail(id, &detail) { tracing::warn!("cache: {e}"); } }
            app.open_detail(detail);
            let _ = atx.send(ApiAction::FetchTimeEntries(id)); // encadeia próxima chamada
            app.set_status(StatusKind::Success, format!("#{id} carregado"));
        }
        ApiResult::Error(e) => app.set_status(StatusKind::Error, e),
        // ...
    }
}
```

Padrões que dão estabilidade:

- **Toda escrita em cache é tolerante a erro** (`if let Err(e) = ... { tracing::warn! }`) — cache falhou ≠ app quebrada.
- **Re-sync automático após sucesso**: `ApiResult::Success` enfileira `FetchIssues` + refetch do detalhe selecionado (`src/handler.rs:45`), mantendo a tela fresca pós-escrita.
- Resultados opcionais que dependem de contexto (tempo da issue) são encadeados: detail → fetch time entries.

## Como as chamadas são implementadas (`src/api.rs`)

As funções do executor usam o **wrapper tipado** `redmine-wrapper-rs` (a UI nunca toca em JSON). Exemplos reais:

```rust
// Lista com includes (custom_fields, children) e filtros tipados
pub async fn fetch_issues(client: &RedmineClient) -> Result<Vec<IssueSummary>, String> {
    let filter = IssueFilter {
        assigned_to_id: Some("me".into()),
        status_id: Some("*".into()),
        sort: Some("id:desc".into()),
        ..Default::default()
    };
    let issues = client.issues.list_with_includes(Some(&filter), &["custom_fields", "children"])
        .await.map_err(|e| format!("Erro ao buscar issues: {e}"))?;
    Ok(issues.into_iter().map(|i| {
        let cf = build_cf_map(i.custom_fields.as_deref());
        IssueSummary {
            id: i.id, subject: i.subject.unwrap_or_default(),
            project_name: name_or(&i.project),
            status_name: name_or(&i.status),
            // ... converte o tipo da API para o Display type
        }
    }).collect())
}
```

Notas:

- **`unwrap_or_default()` / `name_or()`** para campos opcionais — nunca assuma que o JSON tem tudo.
- A conversão **API type → Display type** acontece no executor; a UI só lê `IssueSummary`.
- `update_custom_field`, `log_time`, `update_status` usam payloads tipados (`CreateTimeEntryPayload`, `UpdateIssuePayload`).

## Integração HTTP crua (segunda API) — `src/gerencia.rs`

Quando não há wrapper, o padrão é um `struct Client` com `reqwest::Client` **compartilhado** + credenciais:

```rust
pub struct GerenciaClient {
    http: reqwest::Client,  // reutilizado em todas as chamadas (connection pool)
    url: String, key: String, dev: String,
}

pub async fn fetch_relatorios(&self) -> Result<Vec<GerenciaReport>, String> {
    let resp = self.http
        .get(format!("{}/relatorio-devs", self.url))
        .header("X-API-Key", &self.key)
        .query(&[("dev", &self.dev)])
        .send().await
        .map_err(|e| format!("Erro ao buscar relatórios: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("API Gerencia erro {status}: {body}"));
    }

    let json: serde_json::Value = resp.json().await
        .map_err(|e| format!("Erro ao parsear resposta: {e}"))?;

    let data = json.get("data").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let reports: Vec<GerenciaReport> = data.into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())  // item inválido não derruba o resto
        .collect();
    Ok(reports)
}
```

Boas práticas demonstradas:

- **Um `reqwest::Client` por serviço**, clonado por referência — pooling e reuse de conexão.
- Verifique `status().is_success()` e inclua o **corpo da resposta no erro** — diagnóstico real.
- **`filter_map(serde_json::from_value)`**: um item malformado na lista não derruba o fetch inteiro.
- Headers de auth (`X-API-Key`) centralizados no método, não espalhados.

## Download / upload de arquivos

Download sob demanda com auth (`src/api.rs:186`):

```rust
pub async fn download_attachment(client: &RedmineClient, issue_id, _attachment_id, url, filename, open_after) -> Result<String, String> {
    let dir = Config::attachments_dir(issue_id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Erro ao criar dir: {e}"))?;
    let path = dir.join(filename);
    if path.exists() && !open_after { return Ok(format!("Já existe: {}", path.display())); } // cache local
    if !path.exists() {
        let api_key = client.config.token.as_deref().unwrap_or("");
        let bytes = reqwest::Client::new().get(url)
            .header("X-Redmine-API-Key", api_key).send().await
            .map_err(|e| format!("Erro download: {e}"))?.bytes().await
            .map_err(|e| format!("Erro bytes: {e}"))?;
        std::fs::write(&path, &bytes).map_err(|e| format!("Erro salvar: {e}"))?;
    }
    if open_after { let _ = opener::open(&path); }   // abre no visualizador do SO
    Ok(format!("Anexo{}: {}", if open_after {" aberto"} else {""}, abs))
}
```

Upload em duas etapas (tokenização + attach à issue):

```rust
let token = client.attachments.upload(filename, &data).await.map_err(...)?;
client.issues.update(issue_id, &UpdateIssuePayload {
    uploads: Some(vec![UploadPayload { token, filename: Some(...), content_type: None, description }]),
    ..Default::default()
}).await.map_err(...)?;
```

Padrão de arquivos: **diretório por entidade** (`attachments/{issue_id}/`) e **skipped se já existe** — reaproveita downloads e reduz I/O.

## Resumo do fluxo de uma escrita (ex.: lançar tempo)

```
Usuário digita + Alt+Enter → InputResult::Confirmed
  → valida (data/horas/conflito)
  → app.request_confirm("Lançar Xh em #Y?", ConfirmCallback::LogTime{...})
  → usuário 'y' → handle_confirm
  → app.pending_log_time = probe
  → app.queue_action(ApiAction::LogTime{...})
Loop drena → atx.send(ApiAction::LogTime)
Executor: dispatch → api::log_time → reqwest POST → ApiResult::Success("Tempo lançado!")
Loop: result_rx.try_recv → handle_result
  → set_status(Success, ...) + checa conflito diário (probe)
  → enfileira FetchIssues + FetchIssueDetail (re-sync)
```

## Armadilhas comuns

| Pitfall | Solução |
|---|---|
| `await` direto na UI | Nunca; sempre enfileirar + consumir `try_recv` |
| Bloquear o executor com call sequencial lento | Um spawn serial é ok para tarefas de poucos usuários; para paralelismo real, `join!`/vários workers |
| Erro de API sem contexto | `format!("Erro ...: {e}")` + corpo da resposta no erro |
| Item JSON malformado derruba lista | `filter_map(from_value.ok())` |
| Credenciais hardcoded | Config via `config.toml`/env (ver `config.rs`), nunca no código |

## Próximo passo

Veja [05-cache-persistencia.md](./05-cache-persistencia.md) para o cache offline com SQLite.
