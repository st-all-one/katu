# 12 — MCP: usar o knudge de dentro de um agente

O binário **`knudge-mcp`** expõe a memória por **MCP** (Model Context Protocol)
sobre **stdio** — sem servidor de rede. O MCP não inventa API nova: dispara os
mesmos gatilhos de memória que a CLI oferece, mas de forma automática.

## 1. Rodar o servidor

```bash
knudge-mcp            # transporte stdio (padrão)
knudge-mcp --stdio    # explícito
knudge-mcp --help
knudge-mcp --version
```

O transporte é **uma linha JSON por mensagem** em stdin/stdout. O servidor lê a
config do projeto a partir do diretório atual. `EPIPE`/EOF → **exit 0**.

## 2. Handshake e versões

- `initialize` → `{protocolVersion, capabilities.tools, serverInfo}`.
- `PROTOCOL_VERSION = "2025-06-18"`;
  `SUPPORTED_VERSIONS = ["2025-06-18", "2025-03-26", "2024-11-05"]`;
  versão desconhecida cai na atual.
- Notificações (`notifications/*`) não geram resposta; parse inválido responde
  com `id: null`.
- `tools/call` **nunca** derruba o servidor: argumento inválido vira
  `isError: true`.

## 3. Tools expostas (4)

Três gatilhos + uma tool de status. `HintEngine` é **puro** (sem terminal/FS/
relógio).

| `Trigger` | Tool | Dispara | Fonte |
|---|---|---|---|
| `PreWrite` | `knudge_pre_write` | antes de gravar (quase-duplicados) | candidatos do dedup |
| `PreEdit` | `knudge_pre_edit` | antes de editar arquivo (working set) | manifest do `rewind --files` |
| `SessionEnd` | `knudge_session_end` | fim de sessão (`learn`) | write-gap + propostas |
| — | `knudge_status` | estado do motor | `HintEngine` |

`HintKind` ∈ `duplicate`, `context`, `write_gap`, `missing_link`, `merge`.

### Exemplos conceituais

```jsonc
// antes de gravar
{ "candidates": [{ "id": "x", "statement": "rate limit", "score": 0.8 }] }

// antes de editar
{ "items": [{ "id": "y", "statement": "cache LRU", "score": 0.6 }] }

// fim de sessão
{ "writes": 0, "proposals": [{ "kind": "create_note", "ids": ["z"] }] }
```

## 4. Hints são ponteiros, não corpos

Os hints são **ponteiros** (`id` + afirmação + score). O corpo fica no `kd`,
nunca no contexto do modelo — isso mantém o contexto enxuto. Quando o agente
precisa do corpo: `kd ask --id <ID> --full-content`.

## 5. Motor (`HintEngine`)

- `new(cap, observation_sessions)`: cap de hints e modo observação.
- Dedup **por sessão**; `is_observing` decide se sugere ou só observa.
- `pre_write`, `pre_edit`, `session_end` produzem `Vec<Hint>` (ponteiros).

## 6. Configuração

`McpConfig` lê de `.knudge/config.toml`:

| Chave | Default | Efeito |
|---|---|---|
| `mcp.hints_cap` | `3` | máximo de hints por gatilho |
| `mcp.observation_mode` | `true` | só observa nas primeiras sessões |
| `mcp.observation_sessions` | — | quantas sessões ficam em observação |

## 7. Configurar no cliente

```bash
kd self setup claude      # grava .knudge/setup/claude.json com a recipe
kd self setup cursor
kd self setup codex
kd self setup pi
cat .knudge/setup/claude.json
```

O `setup` **não** edita a configuração do cliente: deixa o arquivo pronto para
você apontar o cliente para `knudge-mcp` (ou registrar o comando manualmente).

## 8. Fluxo típico

1. O agente vai **gravar** → `knudge_pre_write` avisa se já existe.
2. O agente vai **editar** `src/x.rs` → `knudge_pre_edit` devolve o que já se sabe
   do arquivo.
3. Ao **encerrar** → `knudge_session_end` sugere o que virar nota.
4. A qualquer momento → `knudge_status`.

## 9. Quando usar / não usar

- **Use** quando o agente suporta MCP e você quer lembretes automáticos de dedup
  e contexto.
- **Não use** como substituto da CLI: o MCP cobre poucos gatilhos; a superfície
  completa está no `kd` (protocolo em `kd prime`).

## 10. Troubleshooting

| Sintoma | Causa | Ação |
|---|---|---|
| Cliente não vê as tools | recipe não apontada | `kd self setup <cliente>` e aponte |
| Hints demais/poucos | `mcp.hints_cap` | `kd config set --key mcp.hints_cap --value N` |
| Sem hints no começo | modo observação | aguarde as sessões ou `mcp.observation_mode=false` |
