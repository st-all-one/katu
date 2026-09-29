# 10 — Configuração, git e integração com o projeto

## 1. Configuração em dois níveis (D61–D64)

| Nível | Caminho | Papel |
|---|---|---|
| **Global** | Linux `~/.config/local/knudge/config.toml`; macOS `~/Library/Application Support/knudge/config.toml`; Windows `%APPDATA%\knudge\config.toml` | template/default curado |
| **Projeto** | `.knudge/config.toml` | efetivo, **tem precedência** |

- O projeto **clona** o global na instanciação (cópia literal, D62); merge por
  chave é evolução futura.
- `Config::effective(global, project)` mescla e **remove segredos do projeto**
  (D91): o projeto nunca carrega credenciais — `api_key_env` e afins vivem **só
  no global**.
- `config set/unset` **valida contra o schema** e reescreve preservando ordem.
- `strict` é config de **projeto** (`[behavior] strict`), não flag (D94).

## 2. `kd config`

```
kd config get   --key <CHAVE> [--global]
kd config set   --key <CHAVE> --value <VALOR> [--global]
kd config unset --key <CHAVE> [--global]
kd config list  [--global]
kd config promote <recommend|approve|edit|remove|list>
```

```bash
kd config get --key recall.default_limit
kd config set --key recall.default_limit --value 3
kd config set --key recall.semantic --value false
kd config set --key behavior.strict --value true
kd config set --key mcp.hints_cap --value 3 --global
kd init --force                       # recopia o global para o projeto
```

Chave ausente → exit 3; valor inválido → exit 7. Chave desconhecida → sugestão
(D212).

### Chaves mais usadas

| Chave | Default | Para quê |
|---|---|---|
| `recall.default_limit` | `5` | resultados padrão do `ask` |
| `recall.semantic` | `true` | liga/desliga o canal semântico |
| `recall.semantic_weight` | `30.0` | peso do canal semântico |
| `recall.anchor_weight` | `2.0` | peso do canal de âncoras |
| `recall.rrf_k` | `60` | constante do RRF |
| `dedup.create_below` / `dedup.merge_below` | `0.75` / `0.92` | limiares do `write` |
| `write.batch_max` / `task.batch_max` | `100` | teto de lote |
| `retention.*` | — | validade por classificação |
| `decay.anchor_threshold` / `decay.grace_days` | `0.5` / `30` | detecção de âncora quebrada |
| `behavior.strict` | `false` | promove avisos a erro |
| `embeddings.provider` | `"http"` | `http`/`lightweight`/`none` |
| `embeddings.model` / `dimensions` | granite / `384` | identidade do modelo |
| `embeddings.mode` | `"lazy"` | `lazy`/`manual` |
| `embeddings.endpoint` | `:8889` | servidor HTTP |
| `mcp.observation_mode` / `mcp.hints_cap` | `true` / `3` | servidor MCP |
| `proposals.enforce` | `false` | portão bloqueia o `pre-record` |
| `rules.enabled` | `false` | regras governadas |
| `programs.glob` | `plan/*.md` | o que é um programa |

A lista completa sai de `kd config list`. Grupos: `recall.*`, `retrieval.*`,
`embeddings.*`, `mcp.*`, `rules.*`, `proposals.*`, `clusters.*`,
`suggestions.*`, `retention.*`, `write.*`, `task.*`, `programs.*`, `behavior.*`,
`[secrets]`.

## 3. Codec TOML próprio (D97)

Subset implementado em `config/toml/` — **sem o crate `toml`**:

- **Aceita:** comentários, `[seção]`/`[seção.sub]`, chaves bare/citadas/
  pontilhadas, strings de **uma linha**, inteiros com `_`, floats, booleanos e
  listas (inclusive multilinha).
- **Rejeita:** `[[array-of-tables]]`, strings multilinha e `null` — erro `config`
  (exit 7).
- **Leitura preserva a ordem** (diff mínimo); a **escrita é canônica** — `config
  set` gera diff de **uma linha**.

O mesmo codec serve o catálogo de validators (`.knudge/validators.toml`) e os
templates de plano (`.knudge/templates.toml`).

## 4. `kd config promote` — regras governadas (D157)

```bash
kd config promote recommend --universe
kd config promote approve decision_01abc --universe
kd config promote edit decision_01abc --summary "Regra revisada"
kd config promote remove decision_01abc --universe
kd config promote list
```

Transforma conhecimento maduro em **regras governadas** gravadas no bloco
`knudge:rules` do `AGENTS.md`. Desligado por padrão; respeita
`rules.max_promoted`/`min_confidence`.

## 5. Git, worktree e onboarding (D29–D34)

### Resolução do projeto (D29/D91)

- `.knudge/` resolve no **worktree principal** (`git rev-parse
  --git-common-dir`); submódulo não conta.
- O projeto é identificado por **nome lógico**: worktrees do mesmo repo
  **compartilham** o mesmo `.knudge/`.

### Exclusão via `info/exclude` (D30/D34)

- A exclusão vive em `.git/info/exclude` — **nunca** `.gitignore`.
- `persist_in_project=true` (default): versiona `notas/` + `eventos/`, exclui
  derivados (`.idx/`, `cache/`, `*.lock`).
- `persist_in_project=false`: exclui o `.knudge/` inteiro (local-only).

### `.gitattributes` (D31)

`merge=union` para `events*.jsonl` (e cache vetorial — D148), para que eventos
concorrentes não se percam no merge.

### `AGENTS.md` e blocos idempotentes (D60)

`git/block.rs::upsert` insere/substitui blocos delimitados por
`<!-- knudge:start -->`/`<!-- knudge:end -->` sem duplicar. O **protocolo** e o
bloco irmão `knudge:rules` são separados: `init`/`onboard` reescrevem só o
protocolo, preservando as regras.

### Skill do projeto (D162)

`kd init`/`onboard` cria/atualiza `.agents/skill/kd/SKILL.md` — idempotente, com
*version marker*, referenciada no `AGENTS.md`, corpo em inglês e token-optimized.
**Nunca sobrescreve** um arquivo do usuário (sem o marcador).

## 6. `kd init` e `kd onboard`

```
kd init [--force] [--no-prompt]
```

Cria a estrutura canônica (`.knudge/{config.toml,notas/,eventos/,.idx/,cache/}`),
clona o config global, aplica exclusões, escreve o protocolo no `AGENTS.md`,
instala a skill e ajusta `.gitattributes`. Também cria/atualiza a estrutura
`notas/<tipo>/`.

- `--force` recopia o config global (não mexe nas notas).
- `--no-prompt` não imprime o prompt inicial (CI/scripts).
- Fora de um repositório git, funciona, mas as integrações viram no-op.

`--json`: `{project, root, knowledge_dir, in_repo, config_written,
exclude_changed, attributes_changed, agents_changed, skill_changed}`.

## 7. `kd sync`

```
kd sync [--message <TXT>]
```

Comita `notas/` + `eventos/` no worktree principal, com guard de worktree e
mensagem derivada do último evento. Executa `git -C <raiz>` — nunca um shell.

| Versionado | Derivado (fora do git) |
|---|---|
| `.knudge/notas/**` | `.knudge/.idx/` |
| `.knudge/eventos/events*.jsonl` | `.knudge/cache/` |
| `.knudge/config.toml` etc. | `.knudge/.locks/` |
| `.knudge/emb_cache.jsonl` (opt-in) | `.idx/contexts/` |

Fora de um repositório git, degrada com aviso (não é fatal). Retomar num clone:
`git pull` → `kd drain --digest` → `kd rewind --budget 2000`.

## 8. Multi-dev

Clone com o **mesmo modelo** de embeddings: o índice se reconstrói de notas +
cache, sem reinferência. Se dois devs editarem a mesma nota, o git deixa
marcadores; o `doctor` aponta e o agente resolve — o knudge **nunca** mescla um
derivado sozinho.

## 9. Remover a memória

Para aposentar conhecimento, use `kd forget` (soft, reversível). Para remover o
`.knudge/` inteiro de um projeto, com `persist_in_project=false` ele já fica
fora do git.
