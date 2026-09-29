# 07 — Ferramentas (Tools)

O `coding-agent` define 8 tools nativas. Cada tool tem: nome, descrição, schema de parâmetros, `execute`, opções plugáveis (`operations`) para delegar a FS/SSH/sandbox, renderer de TUI e metadados de prompt.

## 1. Catálogo e contratos

| Tool | Input | Comportamento |
|---|---|---|
| `read` | `{ path, offset?, limit? }` | Lê arquivo; auto-resize de imagens; detecção de MIME; truncation |
| `bash` | `{ command, timeout? }` | Executa shell; output acumulado com throttle; truncation; full output em arquivo |
| `powershell` | (análogo a bash) | Execução no PowerShell (Windows) |
| `edit` | `{ path, edits: [{ oldText, newText }] }` | Substituição exata **disjunta**; match no arquivo original; diff unificado |
| `write` | `{ path, content }` | Cria/sobrescreve; cria diretórios-pai |
| `grep` | `{ pattern, path?, glob?, ignoreCase?, literal?, context?, limit? }` | Busca regex/literal; respeita `.gitignore`; usa `rg` |
| `find` | `{ pattern, ... }` | Descoberta de arquivos por glob |
| `ls` | `{ path?, ... }` | Listagem |

Configuração padrão (`defaultTools`): `read`, `bash`, `edit`, `write`.
Read-only: `read`, `grep`, `find`, `ls`.

## 2. Contrato `AgentTool` / `ToolDefinition`

TS (`coding-agent`):
```ts
interface ToolDefinition<TSchema, TDetails> {
  name: string;
  label: string;
  description: string;
  promptSnippet: string;
  promptGuidelines: string[];
  parameters: TSchema;                 // TypeBox -> JSON Schema
  constrainedSampling?: { type: "json_schema", strict: "prefer" | ... };
  execute(toolCallId, args, ctx, update): Promise<AgentToolResult<TDetails>>;
  renderers?: ...;
}
```

Rust proposto:
```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;   // JSON Schema
    fn prompt_snippet(&self) -> &str { "" }
    fn prompt_guidelines(&self) -> &[&str] { &[] }
    fn execution_mode(&self) -> ExecutionMode { ExecutionMode::Parallel }
    async fn execute(&self, ctx: &ToolContext, args: serde_json::Value,
                     updates: &mut dyn FnMut(ToolUpdate)) -> Result<ToolResult>;
    fn render(&self, ...) -> Option<Box<dyn Component>> { None }
}
```

Validação de argumentos: `schema` → validar com `jsonschema` (ou `schemars` + validador). O original valida com TypeBox e também usa `partial-json` para argumentos em streaming.

## 3. Detailhes de implementação a preservar

### `read`
- Trunca por `DEFAULT_MAX_LINES`/`DEFAULT_MAX_BYTES` (head).
- Detecta imagem suportada; se o modelo não tem input `image`, adiciona nota.
- `autoResizeImages` + `resizeOptions` por modelo.
- `access` antes de ler (erro claro).

### `bash`
- Timeout em segundos; máx `2_147_483_647 ms`.
- `OutputAccumulator` com throttle (`BASH_UPDATE_THROTTLE_MS`) para updates parciais.
- Truncation head/tail + `fullOutputPath`.
- Kill de process tree; tracking de PIDs detached.
- Config de shell por plataforma (`getShellConfig`, `getShellEnv`).

### `edit`
- `edits[].oldText` deve ser **único** e não sobrepor outros edits no mesmo call.
- Cada edit é casado contra o **arquivo original**, não incrementalmente.
- Normalização BOM e line endings (`detectLineEnding`, `normalizeToLF`, `restoreLineEndings`).
- Gera diff string + patch unificado.
- `withFileMutationQueue` serializa mutações por arquivo.

### `write`
- `mkdir -p` do diretório pai; escrita UTF-8.
- Também passa pela fila de mutação de arquivo.

### `grep`
- Usa `rg` se disponível (`ensureTool`); fallback? O original assume ripgrep.
- `GREP_MAX_LINE_LENGTH` para truncar linhas longas.
- Respeita ignore files (`ignore` npm → crate `ignore`).
- `limit` default 100; `context` (linhas antes/depois).

Em Rust: a crate `ignore` (do ripgrep) + `grep-searcher`/`grep-regex` dão busca nativa sem depender de `rg` externo, mas o Pi usa `rg` — para paridade de output, considerar invocar `rg` quando presente e ter fallback nativo.

## 4. Pluggable operations

Cada tool expõe `Operations` para redirecionar I/O (ex.: SSH, containers, sandbox). Ex.:
```ts
interface ReadOperations { readFile; access; detectImageMimeType? }
interface BashOperations { spawn; ... }
```
Em Rust: traits `ReadOperations`, `WriteOperations`, `BashOperations`, etc., com implementação default de filesystem local. Isso é o ponto de extensão para o Gondolin/sandbox.

## 5. Renderers de TUI

Cada tool tem renderers (`tools/renderers/{read,bash,edit,grep,...}.ts`) que produzem componentes para o transcript (diferenças, output colorido, estado de progresso). No port, implementar como `impl ToolRenderer` que devolve `Box<dyn Component>` e reaproveita componente anterior quando possível.

## 6. Prompt contributions

Cada tool contribui com `snippet` + `guidelines` para o system prompt (seção `tools`). Ex.:
- `read`: "Read file contents"; guideline "Use read instead of cat or sed."
- `edit`: várias guidelines sobre match exato e edits disjuntos.
- `bash`: "You can inspect PI_* environment variables..."

Essas strings entram no system prompt e afetam o comportamento do modelo — **manter o texto idêntico** para paridade comportamental (e para evals).

## 7. Variáveis de ambiente do agente

- `PI_*` expostas às tools: modelo, sessão, etc. (o `bash.ts` menciona inspecionar `PI_*`).
- Mapear quais são setadas (ver `docs/environment-variables.md`) e reproduzir.

## 8. Fila de mutação e concorrência

`file-mutation-queue.ts` garante que `edit`/`write` no mesmo arquivo não intercalem. Em Rust: um `Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>` ou um ator por path.

## 9. Ordem de port

1. `read`, `write`, `bash` (com truncation e output accumulator).
2. `edit` (com edit-diff e fila).
3. `grep`, `find`, `ls`.
4. Renderers de TUI.
5. `powershell`.
6. Operations plugáveis + sandbox.
