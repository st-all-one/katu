# 07 — Tarefas, planos e fluxo

> **Tarefa é uma view derivada de notas**, não um banco paralelo. `scope` marca
> o **nível**; `type` marca a **espécie**. Papel, modo, progresso, impacto e
> fluxo são **derivados**.

## 1. Hierarquia e modelo

```
epic ⊃ { issue ⊃ task | task }
```

- **`epic`** é a raiz (pode viver sozinho, **não tem pai**, sem `type` — o tipo
  efetivo `epic` é derivado, D149).
- **`issue`** é **opcional** (D134).
- **`task`** é a folha.
- O pai pode ser **qualquer ancestral de rank estritamente menor**
  (`epic < issue < task`); `epic → task` direto é válido.
- **Pai único**; `blocks` é **1-based** e exige pai.
- Profundidade ≤ 4.

**Duas camadas da hierarquia:**

1. **Relação durável** — marcador no corpo:
   `<!-- knudge:parent <id> blocks <n> -->` (texto; sobrevive a qualquer
   rebuild).
2. **Projeção derivada** — aresta `results_in` do pai para o filho, recriada a
   partir do marcador.

## 2. `scope`, `type` e `--kind`

| `scope` | `type` default | `--kind` aceita | papel (depth 0/1/≥2) |
|---|---|---|---|
| `epic` | *(omitido → epic)* | — | **Epic** |
| `issue` | `task` | `task/error/question/risk/decision` | Feature (com filhos) / Story (folha) |
| `task` | `task` | idem | Sub-task |

`validate_kind(scope, kind)` recusa `--kind` em `epic` e aceita só `WORK_KINDS`
(5) em `issue`/`task`.

## 3. O que é derivado (nunca armazenado)

| Derivação | Regra |
|---|---|
| **Papel** (`Role`) | espécie manda (`error`→Bug, `question`→Spike, `risk`→Risk, `decision`→Decision); senão `0`→Epic, `1`+filhos→Feature, `1`→Story, `≥2`→SubTask |
| **Modo** (`Mode`) | `incremental`→Magentic; cadeia de irmãos→Sequential; senão Concurrent |
| **Progresso** | itens de trabalho **folha** na subárvore e quantos `closed` |
| **Épico** (`epic_of`) | sobe pelos pais (≤ 3 níveis) até `scope=epic` |
| **Impacto** | nº de itens **abertos** que dependem transitivamente |
| **Contexto** | pai/bloqueadores/bloqueados/filhos + épico |
| **Fluxo** | derivado do log de eventos (cycle/lead/throughput + caminho crítico) |

**Por que não armazenar:** papel/modo/progresso como campos ficam desatualizados
(a árvore muda) e criam uma segunda verdade.

## 4. Máquina de status (`validate_transition`)

| de \ para | `active` | `in_progress` | `blocked` | `closed` |
|---|---|---|---|---|
| `active` | ✓ | ✓ | | ✓ |
| `in_progress` | ✓ | ✓ | | ✓ |
| `blocked` | ✓ | ✓ | | |
| `closed` | | | | ✓ |

- Transições em `kd task update --status`; **fechamento** é `kd task close` (com
  evidência).
- `superseded`/`forgotten` são atribuídos por `update`/`forget`.
- **Aberto** = `active`/`in_progress`/`blocked`; `is_actionable` exclui
  `closed`/`superseded`/`forgotten` (usado por `--sort impact` e `next:`).

## 5. Comandos

### `new` — criar

```bash
kd task new --summary "Migração para o schema V2" --scope epic --anchor plan/v2.md
kd task new --summary "Converter ids históricos" --scope task --parent epic_01abc
kd task new --summary "Story do cache" --scope issue --parent epic_01abc
kd task new --summary "Corrigir off-by-one" --scope task --kind error --parent issue_01def
kd task new --summary "Escrever testes do parser" --scope task \
  --checks test --checks lint --tag parser --anchor src/toon/parse.rs <<'EOF'
Cobrir: whitespace Unicode, aspas, listas aninhadas.
EOF
```

Flags: `[BODY]`, `--summary`, `--scope`, `--kind`, `--parent`, `--checks`,
`--anchor`, `--tag`, `--params`, `--batch`, `--dry-run`.

### `new --batch` — lote (D141)

```bash
kd task new --batch - --dry-run <<'EOF'
{"key":"p","statement":"Pai em lote","scope":"issue"}
{"key":"c","statement":"Filho em lote","scope":"task","parent":"p"}
{"statement":"Depende do filho","scope":"task","depends_on":["c"]}
EOF
```

- `key` local referencia itens entre linhas (`parent`/`depends_on` por `key`).
- `id` presente = **atualiza**; ausente = **cria**.
- Processa em ordem (pai antes do filho), **best-effort** com `warnings[]`;
  teto `task.batch_max`; `--dry-run` só avalia.
- Carrega as 7 arestas explícitas (ids ou `key`s), permitindo re-parentar.

### `list` — listar

```bash
kd task list --ready
kd task list --blocked --explain
kd task list --ready --sort impact
kd task list --scope epic_01abc
kd task list --tag parser --anchor src/toon/parse.rs
kd task list --universe
```

**Exige filtro** (`--scope`/`--status`/`--kind`/`--parent`/`--ready`/`--blocked`/
`--tag`/`--anchor`) ou `--universe`. `--sort`/`--explain` **não** contam.
`--sort impact` ordena por `(impacto desc, created asc, id asc)`; `--explain`
acrescenta o motivo / `unblocks=N`. `--full-content` renderiza o bloco completo
(não pipe-safe).

### `show` — detalhe

```bash
kd task show --id task_01abc
kd task show --id task_01abc,task_01def
kd task show --id task_01abc --history
```

Mostra corpo, checks, âncoras, tags, evidências e o **contexto** (pai, o que
bloqueia, o que destrava, filhos, épico e progresso).

### `update` — editar

```bash
kd task update --id task_01abc --status in_progress
kd task update --id task_01abc --statement "Novo texto" --parent issue_01def
kd task update --id task_01abc --checks test --checks lint
kd task update --id task_01abc --anchor src/cache.rs      # substitui o conjunto
kd task update --id task_01abc --clear-anchors
```

### `close` — fechar com evidência (D55)

```bash
kd task close --id task_01abc --outcome success --note "testes verdes"
kd task close --id task_01abc --outcome partial --note "2 de 3 casos"
kd task close --id task_01abc --outcome abandoned --note "requisito mudou"
```

Fechar **exige evidência**: roda os `checks` configurados e grava
`outcomes[]`/`evidence`. A saída mostra o épico mais próximo e o progresso
(`done/total`).

### `graph` — árvore

```bash
kd task graph --root epic_01abc
kd task graph --program plan/v2.md
kd --json task graph --root epic_01abc | jq '.data.nodes'
```

### `flow` — fluxo e caminho crítico (D205)

```bash
kd task flow
kd task flow --window-days 1
kd --json task flow | jq '.data.critical_path'
```

- **Cycle time** = `review − primeiro evento`; **Lead time** =
  `close (ou agora) − create`.
- **Throughput** = fechamentos por janela.
- **Caminho crítico** (PERT/CPM): `L(id) = dur(id) + max L(deps)`; desempate por
  maior total, depois caminho mais longo, depois menor caminho lexicográfico.

### `plan` — prompt e submit (D105/D138)

```bash
kd task plan epic_01abc --prompt --template feature
kd task plan epic_01abc --submit --from plano.toon
kd task plan epic_01abc --submit --from -
```

`--prompt` imprime o prompt TOON do template (`feature`/`bug`/`refactor`,
`.knudge/templates.toml`). `--submit` valida **tudo** antes de gravar
(atomicidade lógica) e cria os passos como folhas `task`, ligando dependências
via `write::link`.

## 6. Programas externos (D119/D139)

Um **Programa** é um arquivo `plan/*.md` real (o "porquê", git-tracked). O elo é
a **âncora** (nenhum `scope` novo).

- `roots_for_path` resolve **todos** os épicos-raiz ancorados a um path — um
  `plan.md` pode ancorar vários épicos (floresta).
- `task graph --program` renderiza a floresta; o check `program-anchor` do
  `doctor` avisa quando falta a âncora (warn).

## 7. Relação com conhecimento

- `kd ask` responde **o que se sabe** e, por padrão, só conhecimento; trabalho
  entra com `--with-task`.
- `kd task list` responde **o que fazer** e exige escopo/filtro.
- A ponte é a **âncora** e a confirmação **tarefa→conhecimento** (D108): tarefa
  com outcomes de sucesso que compartilha âncoras **confirma** a nota —
  `recall.confirmation_from_tasks` (default 0,1).
- **Arestas têm via única**: `kd task new` não cria aresta; use
  `kd write --link <FROM:ARESTA:TO>` (D126).

## 8. Anti-padrões e invariantes

- **Não** criar tarefa por `kd write` — use `kd task` (rejeita `task`/`epic`).
- **Não** guardar papel/modo/progresso/dono/prioridade — todos derivados.
- **Não** usar `scope=plan` — saiu (D134); `doctor --fix` migra para `epic`.
- **Não** criar dependência por flag em `task new` — use `write --link`.
- **Não** fechar sem evidência — `task close` roda validators.
- **Não** misturar `ask` e `task list` — superfícies separadas (D146).

## 9. Erros comuns

| Sintoma | Causa | Ação |
|---|---|---|
| `task list` sem filtro | exige escopo | passe filtro ou `--universe` |
| `close` recusa | falta evidência | `--outcome success` (+ `--note`) |
| `graph --program` vazio | épico sem âncora | ancore o épico em `plan/*.md` |
| hierarquia inválida | pai de nível errado | o pai precisa ser nível acima |
