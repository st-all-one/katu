# 01 — Fundamentos, filosofia e instalação

## 1. O problema

Com agentes de IA, a sessão acaba e o contexto evapora. Na próxima vez, alguém
redescobre o mesmo fato no mesmo arquivo pelo mesmo erro. Quando o conhecimento
é registrado, ele se espalha: três notas dizendo quase a mesma coisa, duas
desatualizadas, ninguém sabendo qual vale.

Quatro falhas recorrentes:

- **Contaminação** — a memória enche de duplicatas e contradições; o agente
  "aprende" a coisa errada e repete.
- **Falha de processo** — decisões moram em conversas que ninguém lê; tarefas se
  perdem entre sessões.
- **Depreciação** — o que era verdade há seis meses continua sendo respondido
  como se fosse hoje.
- **Vida útil** — o repositório dura anos, mas o conhecimento dura uma sessão.

O knudge resolve isso **onde o código vive**, sem nuvem, banco ou daemon.

## 2. Filosofia em sete pontos

1. **As notas são a verdade; o índice é só um atalho.** Tudo é Markdown
   legível em `.knudge/notas/`; o índice é reconstruível.
2. **Uma afirmação por nota.** Notas pequenas são fáceis de achar, revisar,
   citar e mesclar — é o que torna o corpus *mergeável pelo git*.
3. **Busque antes de gravar.** A principal defesa contra contaminação: memória
   construída por adição verificada, não por acúmulo.
4. **Âncoras ligam a memória ao código.** A memória fica *situada*: o contexto
   certo, no momento certo.
5. **O knudge propõe; você decide.** Nada é apagado ou fundido em silêncio.
6. **Contexto pequeno, resposta direta.** `id | afirmação | score | motivo`.
7. **Determinismo e simplicidade.** Resultados reproduzíveis são testáveis,
   auditáveis e confiáveis.

## 3. Quando adotar (e quando não)

Adote se marcar **três ou mais**:

- [ ] uso (ou vou usar) um agente de IA neste projeto;
- [ ] o projeto vai durar mais que algumas semanas;
- [ ] já perdi tempo redescobrindo algo que alguém já sabia;
- [ ] há decisões que precisam de registro e contexto;
- [ ] o projeto está no git.

| Seu caso | Recomendação |
|---|---|
| Projeto de vida longa com agente | **Adote** |
| Perde tempo redescobrindo | **Adote** |
| Decisões espalhadas em conversas | **Adote** |
| Repositório já no git | **Adote** |
| Código que muda e conhecimento que envelhece | **Adote** |
| Protótipo/script descartável de um dia | **Não adote** |
| Corpus minúsculo e estável (< ~50 notas) que você lembra de cor | **Pense duas vezes** |
| Quer RAG sobre PDFs/documentos grandes | **Não adote** (é memória de projeto) |
| Quer wiki compartilhado com ACL | **Não adote** (local + git) |
| Projeto sem git | **Pense duas vezes** (perde versionamento/merge) |

## 4. Dois binários, três integrações

| Binário | Papel |
|---|---|
| **`kd`** | a CLI: `init`, `prime`, `ask`, `write`, `task`, `rewind`, `map`, `doctor`, `drain`, `config`, `forget`, `sync`, `self`, `maintenance` |
| **`knudge-mcp`** | servidor MCP sobre stdio para lembretes automáticos no agente |

Além deles, `kd init` materializa no projeto:

- `AGENTS.md` (bloco gerenciado `<!-- knudge:start/end -->`);
- `.agents/skill/kd/SKILL.md` (skill do agente, governada, com marcador de versão);
- `.knudge/config.toml` e `.git/info/exclude` (derivados fora do git).

## 5. Requisitos

| Requisito | Para quê | Obrigatório? |
|---|---|---|
| `git` | versionar o corpus e ligar âncoras ao código | Sim, no projeto |
| Linux, macOS ou Windows | o `kd` roda nos três | — |
| Rust 1.97+ (edição 2024) | só se compilar do source | Não (há binário) |
| `llama.cpp` + GGUF | busca semântica (paráfrases) | Não — opcional |
| `systemd --user`/`launchd` | worker de embeddings automático | Não (cron/manual) |

## 6. Instalação

### Linux/macOS (recomendada)

```bash
curl --proto '=https' --tlsv1.2 --show-error --fail \
  https://raw.githubusercontent.com/st-all-one/knudge/main/install.sh | bash
```

O instalador: baixa o release pré-compilado e **confere o SHA-256** antes de
instalar; instala `kd` + `knudge-mcp` em `~/.local/bin`; ajusta o PATH
(`~/.profile`, `~/.bashrc`, `~/.zshrc`); instala completions de `bash`, `zsh` e
`fish`.

Variações:

```bash
curl ... | VERSION=v0.5.2 bash          # fixar versão
curl ... | INSTALL_DIR=/usr/local/bin bash
./install.sh --from-source               # compilar (Rust 1.97+)
./install.sh --uninstall                 # remove binários (não toca nas notas)
```

### Windows

Use **Git Bash** ou **WSL**. No Windows nativo o script embutido do worker é
Unix; use `--script worker.ps1` ou a instalação manual de embeddings.

### Verificação

```bash
kd self version      # kd 0.5.2
which kd knudge-mcp  # confirma os dois binários
kd prime             # protocolo ("help da IA")
kd --help            # ajuda geral
```

## 7. Quickstart (o ciclo)

Dentro do projeto, na raiz do repositório git:

```bash
# 1) fundar a memória do projeto
kd init

# 2) buscar antes de gravar
kd ask "como o gateway limita requisições" --brief

# 3) gravar o que aprendeu
kd write --summary "O gateway limita 100 rps por chave" --type fact \
  --tag gateway --anchor src/gateway.rs

# 4) planejar/executar trabalho
kd task new --summary "Migrar para o schema V2" --scope epic --anchor plan/v2.md
kd task list --ready --sort impact

# 5) retomar contexto entre sessões
kd rewind --budget 2000

# 6) versionar a memória
kd sync --message "notas: decisão do rate limit"
```

`kd init` cria `.knudge/{config.toml,notas/,eventos/,.idx/,cache/}`; cria
`templates.toml`/`validators.toml` sob demanda; e (havendo git) ajusta
`.git/info/exclude` e `.gitattributes`.

## 8. Embeddings (opcional, recomendado)

Sem embeddings, o `kd ask` usa textual + âncoras. Com embeddings, também
encontra paráfrases/sinônimos.

```bash
kd drain service --install   # baixa llama.cpp+GGUF, sobe servidor :8889, cadastra o projeto
kd drain service --status    # saúde do agendador/servidor/fila
kd drain --digest            # indexa o pendente
kd config set --key recall.semantic --value false   # desliga o canal
```

> ⚠️ **Sempre use `-ub 2048`** ao subir o `llama-server` manualmente: o default
> `512` rejeita notas longas e o drain falha com `indexed=0`.

Detalhe completo em `11-embeddings-e-worker.md`.

## 9. Estrutura em disco

```
.knudge/
  config.toml            # config efetiva do projeto
  notas/<tipo>/<id>.md   # VERDADE (versionada)
  eventos/events*.jsonl  # auditoria append-only (versionada)
  templates.toml         # templates de plan (sob demanda)
  validators.toml        # catálogo de checks/gates (sob demanda)
  emb_cache.jsonl        # cache vetorial (opt-in versionado)
  setup/<cliente>.json   # recipes MCP geradas por kd self setup
  .idx/                  # DERIVADO (fora do git): índice, âncoras, drift,
                         # uso, sugestões, checkpoint, contexts/, embeddings.jsonl
  cache/                 # derivado
  .locks/                # locks advisory efêmeros
```

Não há layout alternativo suportado: o caminho é `notas/<tipo>/<id>.md`
(derivável do prefixo do id; há leitura tolerante ao layout plano legado).

## 10. Desinstalar

```bash
./install.sh --uninstall        # binários + completions (não toca no corpus)
make uninstall                  # se instalou via make
kd drain service --uninstall    # remove worker/servidor (preserva o GGUF)
```

Desinstalar o binário **não apaga nota nenhuma**. Para aposentar conhecimento,
use `kd forget` (soft, reversível).
