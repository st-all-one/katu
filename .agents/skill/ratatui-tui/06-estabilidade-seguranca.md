# 06 — Estabilidade e segurança: lints, parsing e UX de erro

Este arquivo consolida as práticas que tornam a TUI do `my-redmine` **difícil de quebrar**: política de lints no `Cargo.toml`, parsing defensivo de dados externos, confirmações de escrita, e feedback contínuo ao usuário.

## Lints no Cargo.toml — prevenção por construção

```toml
[lints.rust]
unsafe_code = "forbid"        # zero unsafe no workspace

[lints.clippy]
unwrap_used = "deny"          # proíbe unwrap(); exige expect("mensagem")
```

Efeitos:

- `cargo clippy` **falha** se houver `unwrap()` ou `unsafe`. Isso força revisão de cada acesso opcional.
- Combinado com `unsafe_code = "forbid"`, o risco de UB/crash em produção cai drasticamente.
- Convenção do projeto: **`expect()` SEMPRE com mensagem** explicando a premissa (`parse_datetime("...").expect("datetime RFC 3339 deve ser válido")`).

No código real isso aparece assim (`src/types.rs`):

```rust
fn deserialize_notes<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]                 // aceita "nota" OU ["n1","n2"]
    enum NotesValue { One(String), Many(Vec<String>) }
    Ok(match NotesValue::deserialize(deserializer)? {
        NotesValue::One(note) => if note.is_empty() { Vec::new() } else { vec![note] },
        NotesValue::Many(notes) => notes,
    })
}
```

## Parsing defensivo de dados externos

Tudo que vem de API/JSON assume que **pode estar ausente, nulo ou malformado**. Padrões:

| Situação | Padrão usado |
|---|---|
| Campo ausente em JSON do cache | `#[serde(default)]` |
| Data inválida no JSON | `deserialize_with = "deserialize_optional_date"` → `None` |
| Campo `null` | deserializadores que mapeiam `None`/`Default` |
| Enum desconhecido | `Periodo` não tem catch-all — **falha com erro claro** (proposital, evita valor errado silencioso) |
| Item malformado em lista | `filter_map(from_value.ok())` — o item é descartado, o resto permanece |
| String vazia que vira lista | `deserialize_notes` (untagged One/Many) |

`deserialize_optional_date` (`src/types.rs:20`):

```rust
fn deserialize_optional_date<'de, D>(deserializer: D) -> Result<Option<NaiveDate>, D::Error> {
    let s: Option<String> = Option::deserialize(deserializer)?;
    Ok(s.and_then(parse_naive_date))   // data inválida → None, não erro
}
```

Princípio: **o cache nunca deve derrubar a UI por um valor estranho**. O contrário (enum com variante desconhecida) falha de propósito — melhor um erro claro que um valor silenciosamente errado.

## Confirmação em toda operação de escrita

Nenhuma mutação remota acontece sem `y`. O padrão (detalhado no [03](./03-eventos.md)):

1. Usuário dispara ação (`t`, `s`, `n`, `a`...).
2. Validação local (dados obrigatórios, conflitos) **antes** de qualquer popup.
3. `request_confirm(descrição, callback, resumo?)` → `Mode::Confirm`.
4. `y`/`Enter` → enfileira `ApiAction`; `n`/`Esc` → descarta.
5. `callback.take()` no `y` garante execução única.

Efeito colateral desejado: o usuário **nunca faz uma escrita por acidente** — a confirmação é o "undo barato" de uma TUI.

## Feedback contínuo: a barra de status

Toda ação visível tem reação (`app.set_status`). Três níveis:

```rust
pub enum StatusKind { Info, Success, Error }
```

- **Info** — `"Sincronizando..."`, `"Ordenação: título"`.
- **Success** — `"Sync: 23 issues"`, `"Nota adicionada!"`.
- **Error** — `"Erro ao adicionar nota: [parse-error] resposta 204 sem corpo"` (com o detalhe real).

A barra também mostra o **estado de operações pendentes** (`src/ui.rs:38`):

```rust
let sync_text = if app.pending_actions.is_empty() {
    format!(" Sync: {sync} ")
} else {
    format!(" Sync: {sync} ⏳ ")   // há ações em voo
};
```

> Se a API demorar, o usuário vê `⏳` e sabe que há trabalho em andamento — sem parecer que travou.

## Erros amigáveis e enriquecedores

O executor converte erros crus em mensagens com contexto (`src/dispatch.rs`):

```rust
ApiAction::AddNote { issue_id, notes } =>
    api::add_note(client, issue_id, &notes).await
        .map_or_else(|e| ApiResult::Error(e), ApiResult::Success),
```

e as funções de API adicionam contexto:

```rust
.map_err(|e| format!("Erro ao buscar issues: {e}"))?
.map_err(|e| format!("API Gerencia erro {status}: {body}"))?  // inclui corpo HTTP
```

- `color-eyre::install()` no `main` dá backtraces legíveis para erros não tratados.
- Erros em pt-BR, com o motivo técnico no final (ex.: status HTTP + body).
- `tracing::warn!` para problemas não fatais (cache, clipboard) — não sobem à UI.

## Regras defensivas em operações de estado

- **Nunca índices diretos**: `list_state.selected().and_then(|i| filtered_indices.get(i))` → `.get()` sempre.
- **Listas vazias**: toda navegação checa `len == 0` antes (`next()`/`previous()` retornam cedo).
- **Scroll clamp**: `min`/`saturating_sub` em todos os caminhos (ver [02](./02-renderizacao.md)).
- **Take para liberar**: `std::mem::take(&mut app.pending_actions)` e `confirm_callback.take()` evitam duplicidade.

## Formatação tipada (menos bugs de display)

Helpers isolados e testados:

```rust
pub fn format_hours(hours: f64) -> String {
    let h = hours.floor() as u64;
    let m = ((hours - h as f64) * 60.0).round() as u64;
    format!("{h}:{m:02}")
}
```

- `format_hours(1.5)` → `"1:30"`, `format_hours(2.25)` → `"2:15"` — com teste unitário.
- Datas `Option<NaiveDate>` formatadas por helper `fmt_date` (`"-"` quando `None`).
- Enum de enumeração (`resolve_cf_value`) com fallback `[ID {id}]` para valores não mapeados — nunca quebra por id novo.

## Checklist final de estabilidade

- [ ] `cargo clippy` limpo com `unwrap_used = "deny"` / `unsafe_code = "forbid"`
- [ ] Nenhum `unwrap()`; `expect()` sempre com mensagem
- [ ] Todo `load_*` de cache retorna `Option` e nunca panica
- [ ] Toda escrita remota passa por confirmação y/n
- [ ] Scroll/índices sempre com `.get()`/`min`/`saturating_sub`
- [ ] Status bar reflete sucesso, erro e trabalho pendente (`⏳`)
- [ ] Erros de API com contexto (status HTTP + body) em pt-BR
- [ ] Testes nos módulos de borda (cache, parsing, formatação)

## Fim do guia

Reveja o [00-introducao.md](./00-introducao.md) para o quadro geral, ou navegue pelo `SKILL.md` para os arquivos por tópico. O projeto de referência completo é o `redmine-tui`/`my-redmine`.
