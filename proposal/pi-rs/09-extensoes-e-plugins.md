# 09 — Extensões e Plugins

O Pi é "self extensible": extensões são módulos TypeScript carregados **no mesmo processo** via `jiti`, capazes de registrar tools, comandos, atalhos, providers, renderers e handlers de eventos, além de injetar UI e mensagens de contexto.

## 1. O que uma extensão pode fazer

- `registerTool(...)` — tool LLM-callable.
- `registerCommand(...)`, `registerShortcut(...)`, `registerFlag(...)` — CLI/slash/atalhos.
- `registerProvider(...)` — provider de LLM custom.
- `registerEntryRenderer(customType, renderer)` — render de entradas custom no TUI.
- `registerToolRenderer(...)` — render custom de tools.
- `on(event, handler)` — eventos do agente/sessão.
- `ctx.ui.*` — `select/confirm/input/notify/setStatus/setWidget/setFooter/custom/editor`.
- `ctx.sendMessage(...)` — injeta `CustomMessage` no contexto.
- `ctx.bash(...)`, `ctx.exec(...)` — execução.
- Hooks: `cache_warming_decision`, transformação de contexto, etc.
- Persistência própria via entradas `custom`/`custom_message`.

A API completa está em `packages/coding-agent/src/core/extensions/types.ts` (~2000 linhas).

## 2. Carregamento atual

- `jiti-loader.ts` / `jiti-static-loader.ts` transpilam TS em runtime.
- `loader.ts` descobre extensões (builtin, projeto, config, CLI), `wrapper.ts` adapta, `runner.ts` executa lifecycle.
- `virtual-modules.ts` fornece módulos virtuais importáveis.
- `extensions/index.ts` traz as **builtin extensions** (ex.: `llama`).

## 3. Opções para Rust

### Opção A — Traits nativos (MVP, recomendada)
```rust
pub trait Extension: Send + Sync {
    fn name(&self) -> &str { std::any::type_name::<Self>() }
    fn register(&self, reg: &mut Registry) {}
    fn on_event(&self, ev: &AgentEvent, ctx: &ExtensionContext) {}
}
```
- Extensões são crates Rust registrados no binário (feature flags).
- Rápido, seguro, tipado; sem sandbox.
- Bom para extensões de primeira parte e para o núcleo.

### Opção B — WASM (`wasmtime`/`wasmi`)
- Extensões compiladas para `wasm32-wasi`, carregadas em runtime.
- Sandbox de filesystem/rede; linguagem-agnóstico.
- Interface via WIT (component model) espelhando o `ExtensionContext`.
- Custo: ABI, serialização de eventos/UI, tooling de build.

### Opção C — Scripting (`rhai`/`mlua`/`deno_core`)
- `rhai`: puro Rust, fácil de embutir, sem sandbox robusto.
- `mlua` (Lua): maduro, sandbox configurável.
- `deno_core`: JS/TS real (mais próximo do original), mas traz o V8 — pesado e reintroduz um runtime JS.
- Se a meta é **compatibilidade com extensões TS existentes**, `deno_core` é o único caminho realista, porém contradiz parte do ganho de "sem Node".

### Opção D — Processo externo via RPC
- Extensão roda como processo separado falando JSONL/CBOR.
- Isolamento forte; útil para linguagens arbitrárias.
- Latência e complexidade de UI (via `extension_ui_request`).

## 4. Recomendação

1. **Fase 1–2**: traits nativos (Opção A). Portar as builtin extensions e criar o `Registry`.
2. **Fase 4**: WASM (Opção B) para extensões de usuário, com um WIT que espelha `ExtensionContext`. Fornecer SDK em Rust (e talvez TS→WASM) para autores.
3. **Fase 5**: RPC (Opção D) para extensões em outras linguagens.
4. **Não** tentar compatibilidade binária com extensões TypeScript no MVP. Documentar a incompatibilidade como decisão explícita.

## 5. Mapeamento da API de extensão

| TS (`ExtensionContext`) | Rust |
|---|---|
| `ctx.ui.select/confirm/input` | `async fn` em `UiContext` trait |
| `ctx.ui.notify/setStatus/setWidget/setFooter` | métodos síncronos |
| `ctx.ui.custom(factory, opts)` | `Box<dyn Component>` + canal de conclusão |
| `ctx.sessionManager` | `&SessionManager` |
| `ctx.model`, `ctx.getContextUsage()` | getters |
| `ctx.bash/exec` | `ProcessOps` trait |
| `ctx.sendMessage` | `SessionHandle::inject(CustomMessage)` |
| `registerTool/renderer/command` | `Registry` builder |
| eventos | `broadcast::Receiver<AgentEvent>` |
| `EventBus` | `tokio::sync::broadcast` |

## 6. Recursos "declarativos" (sem código)

Grande parte da extensibilidade do Pi é **declarativa** e deve ser portada antes das extensões de código:
- **Skills**: instruções carregadas sob demanda + arquivos de apoio. Formato de diretório/arquivo; descoberta e injeção no contexto. Portar o parser e a injeção.
- **Prompt templates**: arquivos de template que expandem input do editor. Formato textual; portar a expansão.
- **Themes**: JSON de cores; portar o validador (`theme-json.ts`) e o loader.
- **Context files**: AGENTS.md e equivalentes; portar descoberta e montagem.
- **Packages**: manifest + recursos; portar o `package-manager` (npm/git) de forma simplificada (download + layout local).

## 7. Segurança

- O original **não** tem sistema de permissões embutido; roda com as permissões do processo. Recomenda containerização (`docs/containerization.md`).
- Em Rust, a Opção B (WASM) melhora o isolamento, mas a Opção A mantém a mesma postura do original. Documentar e oferecer sandbox via containers.
- Trust de projeto (`ask|always|never`) controla carregar recursos do projeto — portar cedo.

## 8. Migração de extensões existentes

- Fornecer um guia "TS extension → Rust extension" com exemplos equivalentes.
- Manter nomes de eventos e campos idênticos para reduzir atrito.
- Para autores que não migrarem, a Opção D (processo externo) permite reaproveitar a extensão TS rodando sob Node.
