# 02 — Ciclo, convenções da CLI e contrato de saída

## 1. O ciclo central

```
kd ask → kd write → kd task → kd sync
(buscar)  (gravar)   (executar) (commit)
```

| Passo | Pergunta | Comando |
|---|---|---|
| **Buscar** | "o que já se sabe?" | `kd ask` |
| **Gravar** | "o que aprendi que vale guardar?" | `kd write` |
| **Executar** | "o que falta e como está?" | `kd task` |
| **Versionar** | "como preservo/compartilho?" | `kd sync` |

Antes de gravar, **sempre** busque. O `write` faz dedup contra o existente.

## 2. Os 14 verbos (superfície v3, D209)

Top-level: `init prime rewind ask write task map maintenance doctor drain config
forget sync self` (+ `help`).

| Verbo | Absorve / papel |
|---|---|
| `kd` (sem args) | = `kd help` (exit 0) |
| `init` | funda `.knudge/`, escreve `AGENTS.md` + skill |
| `prime` | protocolo de uso estático (byte-idêntico por versão) |
| `ask` | toda pesquisa: recall, `--id` (get), `--around` (expand), `--rank`, `--tags`, `--suggest` |
| `write` | toda escrita: criar, `--update`, `--link`, `--outcome`, `--claim`, `--batch` |
| `task` | trabalho: `new list show update close graph plan flow` |
| `rewind` | estado/handoff ponto-no-tempo |
| `map` | mapa agregado do corpus (`--axis`, `--semantic`, `--communities`, `--write`) |
| `doctor` | saúde: 13 checks + auditoria (`--fix`, `--explain`) |
| `maintenance` | `compact`/`learn`/`prune` (só propõem) |
| `drain` | fila de embeddings; `service` = worker de auto-drain |
| `config` | `get/set/unset/list/promote` |
| `forget` | soft-delete (`--restore`, `--purge`) |
| `sync` | commit de `notas/` + `eventos/` |
| `self` | `version/setup/completions/upgrade` |

O verbo `knowledge` **não existe** (D209): `--rank`/`--tags`/`--suggest` são
modos de `ask`, o mapa é `map`, a promoção é `config promote`.

## 3. Contrato de saída (D71/R20–R23)

- **stdout = dados** (pipe/`--json`); **stderr = logs**. Nunca `println!`/
  `eprintln!`; a saída passa pela borda.
- Envelope `--json`:
  `{success, command, data?, error?, warnings?}`; em erro,
  `error{code, message, retryable}`. **Nenhum log vaza no stdout.**
- Pipe enxuto: `id|statement|score|why`. Busca vazia → `[no_results]` (exit 0).
- **EPIPE** (pipe fechado) → **exit 0** (D73).
- `strict` (config de projeto, D94) promove `warnings[]` a erro.
- `kd` sozinho = `kd help` (exit 0); `--json` sem verbo = `invalid_input` (2).

Teste de sanidade: `kd ... --json 2>/dev/null` deve ser sempre JSON válido.

## 4. Convenções de entrada

- **O posicional é conteúdo, nunca metadado**: em `write`/`task new` é o corpo;
  em `ask` é a consulta. Ids vão em `--id`.
- `-` lê de stdin (corpo/consulta/params); pipe/heredoc sem posicional também.
- `--params '<json>'` envia um item/consulta de uma vez; `--params -` lê stdin.
- `--params` e `-` são **vias exclusivas**.

### 4.1 Listas: repetição ≡ vírgula (D210)

Flags de **seleção** aceitam as duas formas, equivalentes:

```bash
kd ask "cache" --tag a --tag b     # repetindo
kd ask "cache" --tag a,b           # vírgula
```

Vale para `--id --type --class --tag --anchor --edge --claim --checks --files`.
O formato por **espaço** (`--id a b`) **não existe** (exit 2). Em `ask`,
`--id`/`--around` **conflitam** com uma query textual. **Texto livre** (query,
corpo, `--step`, `--summary`, `--note`, `--message`) **não** é dividido; arrays
vão por `--params '<json>'`.

### 4.2 Conjuntos fechados (D212)

Flag com lista fixa **rejeita valor inválido** com a lista completa + sugestão
("did you mean…", Levenshtein determinística). Flag **ausente** não valida nada.

| Flag/campo | Valores |
|---|---|
| `--type` | `fact decision question task def error snippet link meta risk` |
| `--class` | `foundational tactical observational` |
| `--status` | `active in_progress blocked closed superseded forgotten` |
| `--scope` | `epic issue task` |
| `--kind` | `task error question risk decision` |
| `--outcome` | `success partial failure abandoned` |
| arestas (`--link`/`--edge`/`--via`) | `references depends_on contradicts supports extends replaces rejects results_in same_as broader narrower related` |
| `--relation` (`ask --suggest`) | `duplicate contradiction link` |
| `--axis` (`map`) | `anchor type classification scope` |
| `--sort` (`task list`) | `impact` |
| `--template` (`task plan`) | `feature bug refactor` |
| `self setup` | `claude cursor codex pi` |
| `self completions` | `bash zsh fish` |
| `--log-level` | `error warn info debug trace off` |

### 4.3 Escopo obrigatório (D143/D144/D130)

Operações que varrem o corpus exigem escopo
(`--tag/--anchor/--type/--class/--scope/--around`) **ou** `--universe`; sem
escopo → exit 2: `map`, `ask --rank`, `maintenance learn/compact/prune`,
`task list`. Em `task list`, `--sort`/`--explain` **não** contam como escopo.

## 5. Help e protocolo

```bash
kd                 # == kd help
kd --help          # visão geral
kd help <verbo>    # ajuda detalhada e exemplos
kd prime           # protocolo ("help da IA"), compacto por padrão
kd prime --long    # protocolo + gramática TOON + schema
```

`prime` é **estático** (não conhece o corpus) e serve para colar no início da
sessão. O estado dinâmico é `kd rewind`.

## 6. Códigos de saída

| Código | `ErrorKind` | Significado |
|---|---|---|
| `0` | — | sucesso (inclui busca vazia e pipe fechado) |
| `2` | `InvalidInput` | uso/argumento inválido |
| `3` | `NotFound` | id/chave não encontrado |
| `4` | `Conflict` | conflito |
| `5` | `Io` | erro de arquivo |
| `6` | `Timeout` | provedor/hook excedeu o tempo |
| `7` | `Config` | configuração inválida |
| `8` | `Schema` | nota/schema inválido |
| `9` | `UnsafeBlocked` | bloqueado por segurança |
| `70` | `Internal` | erro interno |
| `101` | — | **reservado a panic** |

`ErrorKind::code()` é o contrato de máquina (`not_found`, `invalid_input`, …);
`exit_code()` é a tradução para o processo. `retryable()` só para `Timeout`.
Todo erro de I/O carrega o `path`. `warnings[]` indica **degradação graciosa**
(recurso opcional falhou; resultado parcial); `strict` promove a erro.

## 7. Um dia típico

```bash
# Chegando
kd rewind --budget 2000
kd task list --ready --sort impact

# Durante
kd ask --anchor src/cache.rs --brief
kd write --summary "Cache usa LRU" --type decision --anchor src/cache.rs
kd task update --id task_01abc --status in_progress

# Encerrando
kd task close --id task_01abc --outcome success --note "testes verdes"
kd doctor
kd sync --message "notas: sessão de hoje"
```

## 8. Anti-padrões

- Gravar sem `ask` antes → duplicatas.
- Inventar `id` — ids são derivados; copie-os.
- `kd write --type task` — rejeitado; use `kd task`.
- Criar aresta por flag de tarefa — só via `kd write --link`.
- Esperar que `learn`/`compact`/`prune` mutem o corpus — só propõem.
- Logar no stdout — em `--json`, stdout é só o envelope.
- Declarar tarefa concluída sem `--outcome`.
- Commitar `.idx/`/`cache/`/`contexts/` — derivados; `kd init` cuida do exclude.
- Guardar segredos no corpo (logs redigem; notas não).
