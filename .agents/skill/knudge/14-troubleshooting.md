# 14 — Troubleshooting

Se nada aqui ajudar: `kd doctor`, `kd prime` e o catálogo `DIVERGENCES.md`.

## 1. Instalação e PATH

| Sintoma | Causa | Ação |
|---|---|---|
| `kd: command not found` | PATH não atualizado | `export PATH="$HOME/.local/bin:$PATH"`; novo shell |
| `permission denied` no install | destino sem permissão | `INSTALL_DIR=~/.local/bin` |
| `checksum mismatch` | download corrompido | rode de novo; confira `VERSION`/rede |
| `rustc` antigo | MSRV | Rust **1.97+** (`rustup update`) |
| Completions não funcionam | shell não recarregado | novo shell ou `source` do completion |
| `kd` funciona mas `knudge-mcp` não | só um binário no PATH | `which knudge-mcp`; reinstale |

```bash
kd self version
which kd knudge-mcp
kd --version
```

## 2. Corpus e integridade

| Sintoma | Causa | Ação |
|---|---|---|
| `nota ausente: <id>` | layout antigo / id errado | `kd doctor --fix` |
| `tipo desconhecido` | corpus legado | `kd doctor --fix` |
| erro de schema/`body_hash` | nota editada à mão | `kd doctor --fix` |
| índice divergente | `.idx/` corrompido | `kd doctor --fix` |
| conflito de merge | mesma afirmação, corpos divergentes | `kd doctor` aponta; resolva/substitua |

```bash
kd doctor
kd doctor --fix
kd doctor --explain
```

> **As notas são a verdade.** Nunca edite `.knudge/notas/` à mão — use `kd write`
> / `kd task`. O `doctor --fix` é reversível e **não apaga** notas.

## 3. Busca (`kd ask`)

| Sintoma | Causa | Ação |
|---|---|---|
| `[no_results]` | nada casou | ajuste a consulta; tente `--anchor`; confirme `--with-task` |
| Tarefa não aparece | `ask` é só conhecimento | use `--with-task` ou `kd task list` |
| Poucos/muitos resultados | `--limit` | `--limit N` ou `recall.default_limit` |
| Sem `why=semantic` | embeddings desligados | veja `11-embeddings-e-worker.md` |
| `ask` sem modo | sem query/âncora/id/around | exit 2; passe uma consulta |

## 4. Escrita (`kd write`)

| Sintoma | Causa | Ação |
|---|---|---|
| `rejected` | duplicata | `--update <ID>` |
| `merged` | quase-duplicata | revise a nota |
| `--type task` rejeitado | trabalho ≠ conhecimento | `kd task new` |
| chave desconhecida em lote | `--params`/`--batch` usam canônicas | use `statement`, não `summary` |

```bash
kd ask "rascunho da nota" --brief
kd write --update <ID> --summary "texto corrigido"
kd write --batch lote.jsonl --dry-run
```

## 5. Tarefas (`kd task`)

| Sintoma | Causa | Ação |
|---|---|---|
| `task list` sem filtro | exige escopo | filtro ou `--universe` |
| `close` recusa | falta evidência | `--outcome success` (+ `--note`) |
| `graph --program` vazio | épico sem âncora | ancore o épico em `plan/*.md` |
| hierarquia inválida | pai de nível errado | o pai precisa ser nível acima |

## 6. Embeddings

```bash
kd drain service --status      # saúde do worker
kd drain --status              # fila pendente
kd drain --digest              # tentar de novo
kd config set --key recall.semantic --value false   # desligar
```

Detalhes em `11-embeddings-e-worker.md` §13.

## 7. MCP

| Sintoma | Causa | Ação |
|---|---|---|
| Cliente não vê as tools | recipe não apontada | `kd self setup <cliente>` e aponte |
| Hints demais/poucos | `mcp.hints_cap` | `kd config set --key mcp.hints_cap --value N` |
| Sem hints no começo | modo observação | aguarde as sessões ou `mcp.observation_mode=false` |

## 8. Envelope `--json` e códigos

```bash
kd --json ask "cache" 2>/dev/null | jq .   # stdout é JSON puro
kd --json ask "cache" | jq '.warnings'      # degradação graciosa
kd ask ""; echo "exit=$?"                   # confirma o código
```

| Código | Significado |
|---|---|
| `0` | sucesso (inclui busca vazia e pipe fechado) |
| `2` | uso/argumento inválido |
| `3` | id/chave não encontrado |
| `4` | conflito |
| `5` | erro de arquivo |
| `6` | tempo esgotado |
| `7` | configuração inválida |
| `8` | nota/schema inválido |
| `70` | erro interno |
| `101` | panic (reservado) |

- **EPIPE** (pipe fechado) → exit **0**.
- `warnings[]` = degradação graciosa; `strict` promove a erro.

## 9. Ainda travado?

```bash
kd prime                 # releia o protocolo
kd doctor                # diagnóstico do corpus
kd rewind --budget 2000  # onde eu estava?
```
