# 05 — Cache e persistência local (SQLite)

O `my-redmine` usa SQLite (`rusqlite`) como cache offline e como **fonte de dados de trabalho local** (indicações). O padrão: startup instantâneo a partir do cache, sync manual (`r`), e cache tolerante a falhas.

## Modelo do cache (`src/cache.rs`)

Três tabelas + uma chave-valor genérica:

| Tabela | Conteúdo |
|---|---|
| `cache` | Chave-valor: `issues:list` (JSON) e `detail:{id}` (JSON) |
| `cache_activity` | Catálogo de atividades (id + nome) |
| `cache_indicacoes` | Dados locais das indicações (1 linha, JSON) — **dados de trabalho, não cache** |

O conteúdo é **JSON serializado com serde** — o schema dos tipos é o contrato. Por isso há versionamento.

### Abertura com migração automática

```rust
// src/cache.rs:26
pub fn open(path: &Path) -> Result<Self, CacheError> {
    let conn = rusqlite::Connection::open(path)
        .map_err(|e| CacheError::Open(e.to_string()))?;

    let version: u32 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|e| CacheError::Sql(e.to_string()))?;

    if version > 0 && version < CACHE_VERSION {
        // Schema desatualizado → limpa tudo e reconstrói
        conn.execute_batch(
            "DELETE FROM cache;
             DELETE FROM cache_activity;
             DELETE FROM cache_indicacoes;")
        .map_err(|e| CacheError::Sql(e.to_string()))?;
        tracing::warn!("Cache schema desatualizado (v{version}), dados limpos");
    } else if version > CACHE_VERSION {
        // Cache de versão FUTURA → não toca (evita corromper um app mais novo)
        return Err(CacheError::VersionMismatch { found: version, expected: CACHE_VERSION });
    }

    conn.execute_batch(&format!(
        "PRAGMA user_version = {CACHE_VERSION};
         PRAGMA journal_mode = WAL;
         PRAGMA foreign_keys = ON;
         CREATE TABLE IF NOT EXISTS cache (...);
         CREATE TABLE IF NOT EXISTS cache_activity (...);
         CREATE TABLE IF NOT EXISTS cache_indicacoes (...);"
    )).map_err(|e| CacheError::Sql(e.to_string()))?;

    Ok(Self { conn })
}
```

Pontos de robustez:

- **`PRAGMA user_version`** = versão do schema. Bump em `CACHE_VERSION` quando os tipos serializados mudarem.
- Versão **mais antiga** → limpa dados e atualiza (dados velhos podem não deserializar).
- Versão **mais nova** → recusa abrir (`VersionMismatch`) em vez de corromper; o app segue sem cache (`Option<CacheDb>`).
- **`journal_mode = WAL`** para leituras não bloquearem escritas e vice-versa.
- O `main` abre o cache com fallback tolerante: `CacheDb::open(&path).inspect_err(|e| tracing::warn!(...)).ok()` → `app.cache: Option<CacheDb>`.

## Uso no app: cache-first, API-fallback

A integração com a UI segue cache-first:

- **Startup** (`src/main.rs:64`): carrega issues e indicações do cache → tela instantânea; depois enfileira sync automático.
- **Abrir detalhe** (`src/event/list.rs:14`): tenta `cache.load_detail(id)`; se não houver, enfileira `FetchIssueDetail`.
- **Sync** (`r`): baixa da API e **sobrescreve** o cache (`save_issues`).

Escrever sempre é seguro contra falha:

```rust
if let Some(ref c) = app.cache {
    if let Err(e) = c.save_issues(&issues) { tracing::warn!("cache: {e}"); }
}
```

> Cache com falha de escrita ≠ app quebrada. Logue e siga em frente.

## Merge de dados locais + API (padrão valioso)

As indicações têm campos locais (nota_impl, notas, tempo, status, anexos) que a API da Gerência **não devolve**. O handler faz um merge quando o fetch chega (`src/handler.rs:65`):

```rust
// Preserva campos locais das indicações em cache ao receber dados da API
let cached = std::mem::take(&mut app.gerencia_indicacoes);
let cached_map: HashMap<u64, GerenciaIndicacao> = cached.into_iter()
    .map(|ind| (ind.id, ind)).collect();

let merged: Vec<GerenciaIndicacao> = items.into_iter().map(|ind| {
    if let Some(local) = cached_map.get(&ind.id) {
        GerenciaIndicacao {
            nota_implementacao: local.nota_implementacao.clone(),
            notas: local.notas.clone(),
            status_local: local.status_local.clone(),
            status_local_id: local.status_local_id,
            horas: local.horas.clone(),
            ..ind              // campos da API ficam por cima
        }
    } else { ind }
}).collect();

// Re-adiciona indicações locais que a API não devolveu
for (id, local) in cached_map {
    if !merged.iter().any(|ind| ind.id == id) { merged.push(local); }
}
```

Isso permite **trabalho offline**: cria/edita indicação local, e o vínculo só envia ao Redmine quando confirmado. O cache é a fonte da verdade dos campos locais.

## Escrevendo múltiplas linhas: transação

Catálogos (atividades) são reescritos com transação — tudo-ou-nada:

```rust
pub fn save_activities(&mut self, activities: &[IdName]) -> Result<(), String> {
    let tx = self.conn.transaction().map_err(...)?;
    tx.execute("DELETE FROM cache_activity", [])?;
    for activity in activities {
        tx.execute("INSERT INTO cache_activity (id, name) VALUES (?1, ?2)",
            rusqlite::params![activity.id.raw(), &activity.name])?;
    }
    tx.commit().map_err(...)?;
    Ok(())
}
```

## Carregar JSON defensivamente

Todo `load_*` devolve `Option`/`Vec` e **nunca panica**:

```rust
pub fn load_issues(&self) -> Option<Vec<IssueSummary>> {
    let mut stmt = self.conn.prepare("SELECT value FROM cache WHERE key = 'issues:list'").ok()?;
    let json: String = stmt.query_row([], |row| row.get(0)).ok()?;
    serde_json::from_str(&json).ok()
}
```

- `prepare(...).ok()?` e `query_row(...).ok()?` — tabela ausente/erro SQL vira `None`.
- `serde_json::from_str(...).ok()` — JSON corrompido vira `None`, não panica.
- Esses métodos retornam `Option` no caso da `CacheDb` estar com `#[allow(dead_code)]`; o resto usa `Result<_, String>` com mensagem.

## Migração de dados legados

`load_indicacoes` migra o formato antigo de horas (campos soltos → vetor) ao carregar:

```rust
for indicacao in &mut indicacoes {
    if indicacao.horas.is_empty() && !indicacao.horas_hours.is_empty() {
        indicacao.horas.push(IndicacaoTimeEntry {
            spent_on: indicacao.horas_spent_on.clone(),
            hours: indicacao.horas_hours.clone(),
            activity_id: indicacao.horas_activity_id,
            comments: indicacao.horas_comments.clone(),
        });
    }
}
```

## Testes do cache

O `cache.rs` é o módulo mais testado do projeto (round-trips com `tempfile`, migração de versões). Padrões:

- Round-trip: salvar → carregar → **comparar pelo JSON** (`assert_same_json`), que é deterministicamente estável.
- Banco novo define `user_version`; versão antiga limpa; versão futura retorna `VersionMismatch`.
- Testes usam `tempfile::TempDir` para não poluir o diretório real.

> Teste o cache isoladamente: ele é a parte mais sensível a mudanças de schema.

## Armadilhas comuns

| Pitfall | Solução |
|---|---|
| Mudar o JSON dos tipos e esquecer o version | Bump `CACHE_VERSION` + estratégia de migração |
| Cache corrompido derruba o app | `open` tolerante + `Option<CacheDb>` + `load_*` nunca panic |
| Sobrescrever campos locais com dados da API | Merge explícito preservando os locais |
| Escrita falha interrompe o fluxo | `if let Err(e) = ... { tracing::warn! }` |
| Várias escritas sem transação | `transaction()` para reescritas de catálogo |

## Próximo passo

Veja [06-estabilidade-seguranca.md](./06-estabilidade-seguranca.md) para lints e boas práticas de robustez.
