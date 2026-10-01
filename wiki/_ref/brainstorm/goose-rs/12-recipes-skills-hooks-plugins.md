# 12 — Recipes, skills, hooks, plugins e segurança

O goose tem uma camada de **automação declarativa** e uma camada de **política/segurança** — ambas sem exigir código Rust do usuário.

## 1. Recipes — agentes como YAML

`crates/goose/src/recipe/mod.rs`. Uma recipe descreve um agente/configuração reutilizável:

```rust
pub struct Recipe {
    pub version: String,
    pub title: String,
    pub description: String,
    pub instructions: Option<String>,   // system prompt
    pub prompt: Option<String>,         // prompt inicial
    pub extensions: Option<Vec<ExtensionConfig>>,
    pub settings: Option<Settings>,     // provider, model, temperature, max_turns
    pub activities: Option<Vec<String>>,
    pub author: Option<Author>,
    pub parameters: Option<Vec<RecipeParameter>>,
    pub response: Option<Response>,     // response.json_schema
    pub sub_recipes: Option<Vec<SubRecipe>>,
    pub retry: Option<RetryConfig>,
}
```

- **Parâmetros** (`RecipeParameter`): `key`, `input_type`, `requirement` (`required`/`optional`/`user_prompt`), `default`, `description`. Substituídos no template.
- **Sub-recipes** (`SubRecipe`): `name`, `path`, `values`, `sequential_when_repeated`, `description`. Permitem composição.
- **Resposta estruturada**: `response.json_schema` injeta o `final_output_tool`.
- **Retry** por recipe.

Subsistemas: `build_recipe/`, `template_recipe.rs` (renderização), `validate_recipe.rs`, `value_deserializer.rs`, `manifest.rs`, `local_recipes.rs`, `recipe_extension_adapter.rs`.

CLI: `goose recipe validate|deeplink|open|list`, `--recipe`, `--params`, `--sub-recipe`, `--explain`, `--render-recipe`. Há suporte a recipes via GitHub (`GOOSE_RECIPE_GITHUB_REPO`) e deeplinks para o desktop.

O próprio repositório tem `goose-self-test.yaml` — o goose testa a si mesmo rodando uma recipe.

## 2. Skills

Skills ensinam o goose a executar um fluxo. Modelo:

- **`SKILL.md`** com frontmatter (`SkillFrontmatter`).
- Locais: globais (`~/.agents/skills/`, ou configurável) e por projeto (`<project>/.agents/skills/`).
- `discover_skills(working_dir)`, `list_installed_skills`, `all_skill_dirs`.
- **Argumentos**: `skill_argument_names`, `skill_argument_hint`, `loaded_skill_context_with_args`.
- Skills embutidas (`skills/builtins/`).
- `supporting_files.rs` — arquivos anexos a uma skill.

O conceito mais amplo é **Sources** (`sources.rs`, 2.164 linhas): três tipos de fonte editáveis — `Skill`, `Project`, `Agent` — mais `BuiltinSkill`:

- Skills: `~/.agents/skills/` ou `<project>/.agents/skills/`.
- Projects: `<dataDir>/projects/<slug>.md` (com working dirs).
- Agents: arquivos de definição de agente.

Frontmatter parseado com `serde_yaml` (`parse_frontmatter`), com validação de nomes e slugs.

## 3. Hooks — política executável no ciclo de vida

`crates/goose/src/hooks/mod.rs` (2.330 linhas). Eventos:

```rust
pub enum HookEvent {
    PreToolUse, PreToolUseResult, PostToolUse, PostToolUseFailure,
    SessionStart, SessionEnd, UserPromptSubmit,
    BeforeReadFile, AfterFileEdit,
    BeforeShellExecution, AfterShellExecution,
    Stop,
}
```

- `HookManager::load(project_root, use_login_shell_path)` escaneia **plugins habilitados** por `hooks/hooks.json`.
- Regras têm **matchers** (ex.: por nome de tool).
- `emit_blocking` devolve `HookDecision`:

```rust
pub enum HookDecision {
    Allow,
    Deny { reason: String, plugin: String },
}
```

- Duas causas de negação (`HookOutcomeCause`): `PolicyDenial` ("Do not retry; this is a policy denial") e `HookFailure` ("could not complete ... configured to block on failure").
- **Fail-open por padrão**; um hook pode ser configurado para **bloquear em falha**. Há teste explícito: `subprocess_failures_fail_open_by_default_and_block_when_configured`.
- Hooks que nunca receberam o payload **não podem permitir**, mas **podem negar** (teste dedicado).
- Hooks podem produzir **banners** (`emit_collecting_banners`).
- Subprocesso nunca via shell implícito; substituição de `plugin_root`.

Princípio de design: hooks são **poderosos mas com política de falha explícita**, e a decisão é observável (`policy_evaluated`, `cause`).

## 4. Plugins — empacotamento de skills + hooks

Um plugin é um **diretório**:

```
my-plugin/
├── plugin.json
├── skills/
│   └── review/SKILL.md
├── hooks/
│   └── hooks.json
└── scripts/notify.sh
```

- `PluginFormat`, `PluginInstall`, `PluginInstallOptions`, `PluginAutoUpdateResult`, `ImportedSkill`.
- `install_plugin(source)` / `update_plugin(name)` / `auto_update_plugins()` — instalação a partir de repositório git.
- Formatos plugáveis: `plugins/formats/open_plugins.rs` (848), `formats/gemini.rs` (232) — o goose importa formatos de plugins de terceiros.
- Descoberta: `discovery.rs` (558), `discover_enabled_plugins`.
- `mcp_servers.rs` — plugins podem trazer servidores MCP.

CLI: `goose plugin install|update|list`.

**Aviso de segurança** (documentação): plugins executam comandos locais; instalar só de fontes confiáveis.

## 5. Permissões

Modelo em três partes:

1. **`GooseMode`** (`auto` | `approve` | `smart_approve` | `chat`) — política global da sessão.
2. **`ToolInspector`** (`tool_inspection.rs`):

```rust
pub trait ToolInspector: Send + Sync { /* ... */ }
pub struct InspectionResult;
pub enum InspectionAction { /* allow/deny/alert */ }
pub struct ToolInspectionManager;
pub fn apply_inspection_results_to_permissions(...);
```

3. **`PermissionInspector` + `PermissionJudge` + `ToolPermissionStore`**: registra e reavalia decisões de permissão por tool.

A confirmação flui como dado na conversa (`ToolConfirmationRequest` / `ActionRequired`), conectando permissão, streaming e UI.

## 6. Segurança

`crates/goose/src/security/` (3.352 linhas):

- `SecurityManager` / `SecurityResult`.
- `AdversaryInspector`, `EgressInspector` — detectors (exfiltração de rede etc.).
- `ClassificationClient` + `ModelMappingConfig` — classificação por modelo.
- `patterns.rs` — `ThreatPattern`, `RiskLevel`, `ThreatCategory`.
- `scanner.rs`.

E `agents/extension_malware_check.rs` (35 KB) para inspecionar extensões.

## 7. Como as peças se relacionam

```
recipe (YAML) ──▶ parâmetros ──▶ extensions + settings
                                   │
skills (SKILL.md) ──┐              ▼
plugins (plugin.json) ── hooks ──▶ Agent (pipeline)
                       skills       │
                                    ▼
                          permissões (GooseMode + inspectors)
                                    │
                                    ▼
                          execução de tool / MCP
```

## 8. Lições

1. **Recipes** tornam o agente declarativo, componível (sub-recipes) e reproduzível.
2. **Skills/hooks/plugins** são o "sistema operacional" de customização do usuário.
3. **Hooks com política de falha explícita** (fail-open vs. block) e decisão auditável.
4. **Permissões como pipeline de inspectors**, com confirmação representada na conversa.
5. **Segurança como subsistema** (adversary/egress/classificação), não como afterthought.
6. **Importação de formatos de plugins de terceiros** (Gemini, Open Plugins) amplia o ecossistema.
