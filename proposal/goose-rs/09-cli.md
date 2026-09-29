# 09 — CLI

O binário `goose` (crate `goose-cli`, 28.616 linhas) é a superfície principal para terminal e o "laboratório" onde recursos nascem antes de ir para o desktop.

- Entry point: `crates/goose-cli/src/main.rs`.
- Definição de comandos: `crates/goose-cli/src/cli.rs` (3.326 linhas, `clap` derive).
- Sessão interativa: `session/mod.rs` (3.312), `session/output.rs` (2.050), `session/builder.rs` (1.662), `session/streaming_buffer.rs` (1.033).

## 1. Comandos de topo

| Comando | Função |
|---|---|
| *(sem subcomando)* | Abre sessão interativa padrão |
| `configure` | Configura providers/extensões/settings |
| `info [-v] [--check]` | Mostra configuração; `--check` testa conexão |
| `doctor` | Diagnóstico do setup |
| `mcp <server>` | Roda um servidor MCP embutido |
| `acp` | goose como **servidor ACP** em stdio |
| `serve` | ACP sobre HTTP/WebSocket |
| `roam` | Compartilhamento P2P (iroh) |
| `session` | Inicia/retoma/gerencia sessões |
| `run` | Executa input/recipe sem interação |
| `recipe` | validate, deeplink, open, list |
| `skills` | Lista skills |
| `plugin` | Install/update plugins |
| `schedule` | Jobs agendados (cron) |
| `gateway` | Integrações externas (ex.: Telegram) |
| `update` | Atualiza o CLI |
| `term` | Sessão integrada ao terminal |
| `local-models` | Modelos de inferência local |
| `completion` | Gera completions (bash/zsh/fish/nu/pwsh) |
| `review` | Code review local com checks |
| `validate-extensions`, `mcp-probe` | Validação/teste de extensões |

## 2. `session`

```rust
Session {
    command: Option<SessionCommand>,   // list/remove/export/import/diagnostics/rename
    identifier: Option<Identifier>,    // --name | --session-id | --path
    resume: bool,
    fork: bool,      // requer resume: copia o histórico p/ nova sessão
    edit: bool,      // abre a conversa no $EDITOR antes de continuar
    history: bool,   // mostra histórico ao retomar
    system: Option<String>,
    session_opts, extension_opts, model_opts,
}
```

Destaques: **fork** (criar nova sessão copiando o histórico), **edit** (editar a conversa no editor e opcionalmente fork), **history**.

`SessionCommand`: `List` (com `--format text|json`, filtro por `--working_dir`, `--limit`, `--ascending`), `Remove`, `Export`, `Import`, `Diagnostics`, `Rename`.

## 3. `run` — modo não interativo

```rust
Run {
    input_opts:      // --instructions <file> | --text <str> | --recipe <name|path>
    identifier:      // --name/--session-id/--path
    run_behavior:    // --interactive, --no-session, --resume, --stats
    session_opts:    // --debug, --max-tool-repetitions, --max-turns, --container
    extension_opts:  // --with-extension, --with-streamable-http-extension, --with-builtin, --no-profile
    output_opts:     // -q/--quiet, --output-format text|json|stream-json
    model_opts:      // --provider, --model, ...
}
```

**Formatos de saída** são cruciais para automação:

| Formato | Uso |
|---|---|
| `text` (default) | interativo/humano |
| `json` | agente único, saída estruturada |
| `stream-json` | streaming consumível por máquina |

`--no-session` executa sem persistir sessão (automação). `--stats` imprime estatísticas de geração.

## 4. Extensões na CLI

```
--with-extension "ENV=val cmd args..."     # stdio
--with-streamable-http-extension "url timeout=100"
--with-builtin developer,github            # builtin (comma-separated)
--no-profile                               # ignora perfil de extensões salvo
```

`--with-extension` aceita nome opcional (`name:ENV=val cmd...`); sem nome, a extensão é nomeada pelo launcher (`npx`, `python`, `uvx`…), desambiguando quando necessário. `--container <id>` roda extensões dentro de um container Docker.

## 5. `term` — integração com o shell

Comando pensado para tornar o goose parte do terminal:

```bash
eval "$(goose term init zsh)"          # zsh/bash
goose term init nu | save ...          # nushell
```

- `term init [--default]` imprime script de inicialização; cada terminal mantém uma **sessão persistente** que é retomada automaticamente.
- `term run <prompt>` envia um prompt à sessão do terminal.
- `term log <command>` (oculto) registra comandos via hook do shell.
- `term info` imprime info compacta (tokens, modelo) para o prompt: `●○○○○ sonnet`.
- Aliases `@goose` e `@g`; com `--default`, comandos desconhecidos vão ao goose.

## 6. `review` — code review local

Descobre revisores-subagente em `**/.agents/checks/*.md` e overrides em `**/.agents/REVIEW.md`, monta uma requisição de review a partir do working tree (ou de um range de diff) e roda via goose. Suporta `--model`, `--provider`, `--override-model`, `--prompt` (prompt base customizado) e orquestração.

## 7. Estrutura interna da sessão

| Arquivo | Papel |
|---|---|
| `session/mod.rs` | Loop interativo |
| `session/output.rs` | Renderização (Markdown, cores, tool calls) |
| `session/builder.rs` | Monta a sessão a partir das opções |
| `session/streaming_buffer.rs` | Buffer de streaming (evita flicker) |
| `session/input.rs` | Leitura/edição de input |
| `session/elicitation.rs` | Respostas a elicitation |
| `session/editor.rs` | Integração com `$EDITOR` |
| `session/completion.rs` | Autocomplete de comandos |
| `session/paste.rs` | Tratamento de colagens grandes |
| `signal.rs` | Tratamento de sinais (Ctrl-C) |

Há também `scenario_tests/` — testes de cenário end-to-end do CLI.

## 8. Lições

1. **Um binário, muitas portas de entrada** (interativo, run, acp, serve, term, mcp).
2. **`run` com `--output-format`** torna o agente scriptável (`json`/`stream-json`).
3. **`--no-session`** para execuções efêmeras.
4. **Integração com shell (`term`)** como diferencial de UX.
5. **Subcomandos de sessão ricos** (fork, edit, import/export) para inspeção e reprodutibilidade.
6. **Features** (`bundled-mcp`, `scheduler`, `update`, `roaming`, `acp-http`, `local-inference`) tornam o binário moldável por distribuição.
