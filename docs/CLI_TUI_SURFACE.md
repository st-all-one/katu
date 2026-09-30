# Superfície do CLI e da TUI

> **Revisão da superfície de utilizador** (comandos, flags e teclas) do binário `katu`, derivada do
> código em [`crates/katu/src/cli.rs`](../crates/katu/src/cli.rs),
> [`crates/katu/src/agent/command.rs`](../crates/katu/src/agent/command.rs),
> [`crates/katu/src/tui.rs`](../crates/katu/src/tui.rs) e
> [`crates/katu-tui/src/action.rs`](../crates/katu-tui/src/action.rs). É um **mapa**, não um
> manual: cada facto continua a ter o seu lar no código/plan.

## 0. Convenções transversais

- **Feature `memory-in-process`** (default): sem ela, os comandos `run`/`tui` **não compilam** e
  `memory`/`recall`/`remember` **falham fechado** (exit 10 `unavailable`).
- **Feature `profile`** (opt-in): liga o diagnóstico estruturado (ver `KATU_INSTRUMENT`).
- **stdout = dados, stderr = logs.** Um *pipe* fechado (`katu … | head`) é **sucesso**.
- `--json` é **global** (pode vir antes ou depois do subcomando): emite um envelope de máquina de
  uma linha em `stdout`. Sem `--json`, os objetos são impressos como `chave = valor`.
- **Códigos de saída** (contrato estável, `ErrorKind::exit_code`): `0` ok · `2` `invalid_input` ·
  `3` `io` · `4` `not_found` · `5` `conflict` · `6` `timeout` · `7` `config` · `8` `schema` ·
  `9` `unsafe_blocked` · `10` `unavailable` · `70` `internal`.

## 1. Comandos do CLI

| Comando | O que faz | Argumentos | Envelope (`data`) |
| --- | --- | --- | --- |
| `version` | Versão do binário + versão do vocabulário de política. | — | `{katu, policy_vocab}` |
| `doctor` | Diagnóstico de arranque: instrumentação, MSRV efetivo e saúde da memória. | — | `{instrumented, rust_version, memory?}` |
| `memory` | Estado do backend de memória (adaptador in-process do knudge). | — | `{backend, health, warnings, knowledge_dir}` |
| `sessions` | Lista as sessões do projeto (id, instante, objetivo, raiz), ordem temporal. | — | `{root, sessions:[{id, created_ms, goal, root}]}` |
| `recall` | Consulta a memória pelo caminho §42 (recall). | `query` (posicional) · `--limit` | `{project, ran, outcome, report}` |
| `remember` | Regista uma nota (recall prévio + escrita pelo gate §42). | `statement` (posicional) · `--anchor` | `{project, ran, outcome, report}` |
| `run` | Executa **um** turno do agente (provider ↔ kernel ↔ tools). | `goal` (posicional) · ver §3 | `{model, steps, chars, calls, cancelled, usage}` |
| `tui` | Abre a UI de terminal sobre o loop (multi-turno). | ver §3 | *(nenhum; sucesso vazio)* |

`katu` sem subcomando imprime a ajuda (`arg_required_else_help`). `katu --version`/`-V` e
`katu --help`/`-h` são do `clap`.

### 1.1 Detalhe por comando

- **`version`** — não toca em portas nem em ficheiros. `policy_vocab` é o `POLICY_VOCAB_VERSION`.
- **`doctor`** — sempre `instrumented` + `rust_version`; `memory` só com o adaptador compilado.
  Nunca falha por a memória estar indisponível (reporta-a dentro de `memory`).
- **`memory`** — abre o adaptador na **raiz do projeto**; sem adaptador, exit 10. Não cria sessão.
- **`sessions`** — só **lê** `<root>/.katu/sessions/index.jsonl`; nunca cria sessão nem toca no log.
  A raiz é descoberta a subir a partir do diretório atual.
- **`recall`** — monta o **runtime** (sessão + regras + memória) e consulta pelo caminho §42.
- **`remember`** — monta o runtime e escreve uma nota `Fact`, com `--anchor` opcional para código.
- **`run`** — monta o runtime, escolhe o modelo (explícito ou por **tier**), monta o pedido a partir
  do log e corre **um** turno; as tool calls passam pela ordem §42. O `usage` vem com `basis`
  (`provider_reported`/`estimated`/…).
- **`tui`** — igual ao `run`, mas entra no loop de eventos da UI (multi-turno). O `goal` interno é
  `"tui"`; o objetivo real de cada turno é a mensagem escrita na UI.

## 2. Flags do CLI

| Flag | Comandos | O que faz | O que espera / default |
| --- | --- | --- | --- |
| `--json` | **global** | Envelope de máquina em `stdout`. | booleano; default `false` |
| `--limit <N>` | `recall` | Máximo de resultados do recall. | inteiro `usize`; default `5` |
| `--anchor <PATH>` | `remember` | Âncora de código da nota. | caminho opcional; default ausente |
| `--provider <NAME>` | `run`, `tui` | Provider a usar. | `llama` \| `opencode-go` \| `opencode-zen`; default `llama` |
| `--model <MODEL>` | `run`, `tui` | Modelo explícito (vence o tier). | string; default pelo **tier** da fase |
| `--base <URL>` | `run`, `tui` | Base URL do endpoint. | URL; default por provider (§2.1) |
| `--max-tokens <N>` | `run`, `tui` | Teto de tokens de saída. | `u32`; default `512` |
| `--max-steps <N>` | `run`, `tui` | Máximo de passos (tool calls) por turno. | `u32`; default `8` |
| `--compact` | `run`, `tui` | Liga a compactação do histórico no turno (E09-T07). | booleano; default `false` |
| `--resume [<ID>]` | `run`, `tui` | Retoma sessão. | sem valor = `last` (mais recente); com valor = id `s_<16hex>` |

### 2.1 Defaults de provider/modelo/base

| Provider | Base por omissão | Modelo por omissão | Credencial |
| --- | --- | --- | --- |
| `llama` | `http://127.0.0.1:8080/v1` | `qwen` | — (servidor local) |
| `opencode-go` | `https://opencode.ai/zen/go/v1` | `longcat-2.5-preview-free` | `KATU_OPENCODE_KEY` |
| `opencode-zen` | `https://opencode.ai/zen/v1` | `longcat-2.5-preview-free` | `KATU_OPENCODE_KEY` |

Provider desconhecido → `invalid_input` (exit 2). `opencode-*` sem `KATU_OPENCODE_KEY` → exit 2.
`--resume` sem sessões (ou id inválido/desconhecido) → `invalid_input` (exit 2), fail-closed.

## 3. Variáveis de ambiente

| Variável | Efeito |
| --- | --- |
| `KATU_INSTRUMENT=1\|true` | Liga o sink de diagnóstico para `stderr` (feature `profile`). |
| `KATU_INSTRUMENT_FILTER=<prefixo>` | Filtra os eventos por subsistema (ex.: `tui`, `provider`). |
| `KATU_OPENCODE_KEY` | Chave dos providers `opencode-go`/`opencode-zen` (**obrigatória**). |
| `USER` / `USERNAME` | Assinante (`granted_by`) de aprovações/overrides; default `local`. |

## 4. A TUI

Abre com `katu tui [flags de §2]`. A UI é **pura** (estado + keymap + render em `katu-tui`); a
borda (`crates/katu/src/tui/handler.rs`) é quem fala com o modelo e executa os efeitos.

### 4.1 Layout

- **Cabeçalho** (1 linha): `katu · <modelo> · pensamento <grau> · fase <fase> · <estado>`
  e, quando existem, `· próximo <ação>` (checkpoint) e `· tokens … custo …` (uso do último turno).
- **Conversa** (esquerda): transcript da sessão, com scroll; teto de 200 entradas por quadro.
- **Atividade** (direita, 34 colunas): painel **efémero** — tools em curso (com **argumentos
  crus**), texto do modelo a chegar e recusas de política; teto de 100 linhas + cauda de 8 KiB do
  stream. Nunca entra no log nem na transcrição (§50.3).
- **Entrada** (3 linhas): linha de mensagem; o título muda com o modo.
- **Rodapé** (1 linha): dicas de teclas ou estado (`a trabalhar… · Esc cancela`, erros).

Render governado por orçamento (`Throttle`, ~60 fps, forçado em cada tecla/fim de turno); o
`ratatui` faz o **diff** de células.

### 4.2 Modos e teclas

Modos: `Normal` (navegação), `Insert` (edição), `Trash` (lixeira), `Transcript` (transcrição),
`Confirm` (**reservado** — ver §6) e o challenge de aprovação (sobreposição, §4.3).

| Modo | Tecla | Ação |
| --- | --- | --- |
| Normal | `q` | Sai da UI |
| Normal | `Enter` / `i` | Entra em `Insert` |
| Normal | `m` | Cicla o **modelo** (próximo turno) |
| Normal | `t` | Cicla o **grau de pensamento** (Off→Low→Medium→High→Off) |
| Normal | `l` | Abre a **lixeira** |
| Normal | `T` | Abre a **transcrição** durável (read-only) |
| Normal | `c` | Liga/desliga a **compactação** (com pré-visualização) |
| Normal | `v` | Corre o **gate de verificação** (override por challenge se bloquear) |
| Normal | `↑` / `↓` | Rola a conversa |
| Insert | `Enter` | Submete a mensagem (inicia o turno) |
| Insert | `Esc` | Volta a `Normal` (sem submeter) |
| Insert | `Backspace` | Apaga o último caractere |
| Insert | caracteres | Escrevem na mensagem (teclas `Ctrl` ignoradas) |
| Trash | `↑` / `↓` | Escolhe a entrada |
| Trash | `r` | **Restaura** a entrada selecionada |
| Trash | `x` | **Esvazia** a lixeira (destrutivo; challenge) |
| Trash | `Esc` / `q` | Fecha a sobreposição |
| Transcript | `↑` / `↓` | Rola a transcrição |
| Transcript | `Esc` / `q` | Fecha a vista |
| qualquer | `Ctrl-C` | Sai da UI (em todos os modos) |

**Durante um turno** (estado `a trabalhar…`), `Esc` ou `Ctrl-C` pedem **cancelamento cooperativo**:
o texto parcial é registado, o turno fecha limpo e a UI mostra `turno cancelado` (evento
`tui.cancel`).

### 4.3 Challenge de aprovação / override

Uma `RequireApproval` de política (ou um check de verificação bloqueado) abre uma sobreposição que
**não** é um *rubber-stamp*: exige responder a três perguntas **e** escrever uma justificação.

| Tecla | Ação |
| --- | --- |
| `Tab` / `Shift-Tab` | Alterna o foco (checklist ↔ justificação) |
| `↑` / `↓` | Move o cursor no checklist |
| `Space` | Marca/desmarca a pergunta (ou insere espaço na justificação) |
| caracteres | Escrevem na justificação (com o foco nela) |
| `Backspace` | Apaga na justificação |
| `Enter` | Submete **só** quando completo (todas marcadas + justificação) |
| `Esc` / `Ctrl-C` | Cancela (fail-closed: mantém o bloqueio) |

Quem assina é `USER`/`USERNAME` (default `local`); o agente **nunca** assina.

### 4.4 Comandos e atualizações (UI ↔ borda)

A UI emite `Command`s (efeitos) e a borda devolve `Update`s. **Comandos**: `Submit`, `SetModel`,
`SetThinking`, `Trash`, `Transcript`, `Restore`, `EmptyTrash`, `Compact`, `Verify`, `Quit`.
**Atualizações**: `Assistant`, `Tool`, `Info`, `Error`, `Phase`, `Live`, `Models`, `NextAction`,
`Usage`, `Trash`, `Transcript`, `Cancelled`, `Done`.

Efeitos na borda: o turno usa o **tier** da fase salvo modelo explícito; no fim escreve o
**checkpoint** de fase (`<root>/.katu/…`) e a **transcrição** durável (`<root>/.katu/transcript.md`);
o `verify` grava o relatório e, se bloqueado, regista `overrides.jsonl` por challenge; o
`EmptyTrash` remove permanentemente após challenge (evento `tui.trash_empty`).

## 5. Ficheiros de política e dados consumidos

| Ficheiro | Papel |
| --- | --- |
| `policy/*.toml` | Regras de política (memória + contenção) carregadas no arranque. |
| `policy/tiers.toml` | Mapa fase → **tier** → modelo (default de `--model`). |
| `policy/prices.toml` | Preços por modelo para o custo na TUI (vazio = `unpriced`). |
| `scope_contract.json` + `feature_list.json` | Contrato de escopo validado no arranque (E09-T04). |
| `<root>/.katu/sessions/…` | Log/índice de sessões (retomada). |
| `<root>/.katu/transcript.md` | Transcrição durável (projeção do log). |
| `<root>/.katu/trash/…` | Lixeira recuperável (E06-T09). |

## 6. Lacunas conhecidas (superfície reservada)

- **Modo `Confirm`** existe no keymap (`y`/`n`) mas **nenhum fluxo o ativa** hoje: `Action::Confirm`
  apenas mostra `nada a confirmar`. É superfície **reservada**, não funcionalidade.
- **`run` é de um só turno**; o multi-turno vive na TUI.
- **Cancelamento é cooperativo** e só é lido durante o stream (a UI corre o turno na sua thread).
- **Kill do grupo de processos** (E07-T04) é o **único** ponto `unsafe` do projeto
  ([ADR 0018](adr/0018-kill-do-grupo-com-unsafe-unico.md)).
