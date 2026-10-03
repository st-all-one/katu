# Superfície do CLI e da TUI

> **Revisão da superfície de utilizador** (comandos, flags e teclas) do binário `katu`, derivada do
> código em [`crates/katu/src/cli.rs`](../../../crates/katu/src/cli.rs),
> [`crates/katu/src/cli/`](../../../crates/katu/src/cli),
> [`crates/katu/src/agent/command.rs`](../../../crates/katu/src/agent/command.rs) e
> [`crates/katu-tui/src/action.rs`](../../../crates/katu-tui/src/action.rs). É um **mapa**, não um
> manual. O épico que reformula esta superfície é
> [`SURFACE_IMPLEMENTATION.md`](../plan/SURFACE_IMPLEMENTATION.md) (E20); a decisão está na
> [ADR 0019](../adr/0019-superficie-cli-tui-v2.md).

## 0. Convenções transversais

- **Verbos exclusivos:** `prime`, `upgrade`, `config`, `memo`, `run`, `tui` (+ `help`). `version`
  deixa de ser verbo (`--version`/`-V` são do `clap`). `katu` **sem subcomando** abre a TUI.
- **Posicional = conteúdo** (`run`, `memo ask`): `-` lê `stdin`; ausente + `stdin` não-TTY lê
  `stdin`; ausente + TTY → `invalid_input` (exit 2). Nunca se inventa conteúdo (I1).
- **`--json` é por comando** (não global): existe em `prime`/`upgrade`/`config`/`memo`/`run`; a
  `tui` **não** tem. `katu --json` (sem verbo) deixa de existir.
- **`--log-level <NÍVEL>` é global** (stderr), default **`quiet`**; **não** existe `--quiet`.
- **stdout = dados, stderr = logs.** Um *pipe* fechado (`katu … | head`) é **sucesso**.
- **Feature `memory-in-process`** (default): sem ela, `run`/`tui` **não compilam** e `memo ask`
  **falha fechado** (exit 10 `unavailable`).
- **Feature `profile`** (opt-in): liga o diagnóstico estruturado.
- **Códigos de saída** (contrato estável, `ErrorKind::exit_code`): `0` ok · `2` `invalid_input` ·
  `3` `io` · `4` `not_found` · `5` `conflict` · `6` `timeout` · `7` `config` · `8` `schema` ·
  `9` `unsafe_blocked` · `10` `unavailable` · `70` `internal`.

## 1. Comandos do CLI

| Comando | O que faz | Envelope (`data`) |
| --- | --- | --- |
| `prime [--long] [--group <g>]` | Contexto de arranque **estático** (byte-idêntico por versão) para IA/utilizador. | `{group, prime}` |
| `upgrade` | Sincronização de versão. **Canal ainda não configurado** → recusa (exit 10). | — |
| `config <init\|login\|get\|set\|unset\|list>` | Configuração global e do projeto (efetiva = projeto > global); `login` liga o agente a um provider. | `{key, value\|entries, scope}` · `login`: `{provider, model, base, logged_in, config, warning?, check?}` |
| `memo <sub>` | Memória: **consulta/visão geral** (nunca escreve). | ver §1.1 |
| `run [BODY]` | Executa **uma** rodada e sai; devolve a resposta + id da sessão + exit code. | `{session, round_exit, model, steps, chars, calls, cancelled, usage, text}` |
| `tui` | Abre a UI de terminal (multi-turno). | *(nenhum; sucesso vazio)* |
| *(sem verbo)* | Abre a TUI; **sem TTY falha fechado** (exit 3). | — |

Verbos antigos (`version`, `doctor`, `memory`, `sessions`, `recall`, `remember`) **deixam de
existir** no topo → `invalid_input` (exit 2), sem retrocompatibilidade.

### 1.1 Detalhe por comando

- **`prime`** — grupos: `global`, `memo`, `run`, `tui`, `config`, `upgrade`. Texto **estático**
  (não inclui o `AGENTS.md`, que entra no contexto do turno — E20-T13). `--long` acrescenta
  gramática e escopo. Cada grupo expõe `<grupo> prime` **equivalente** a
  `katu prime --group <grupo>` (`memo prime`, `config prime`); os verbos-folha usam
  `katu prime --group <g>`. Aceita `--params`/`--batch`.
- **`upgrade`** — fail-closed: reporta que o canal não está configurado; não inventa origem
  (sincronização futura contra GitHub Releases).
- **`config`** — `get`/`list` leem a config **efetiva** (projeto > global); `set`/`unset` escrevem
  no projeto (ou na global com `--global`). Conjunto **fechado** de chaves (E20-T09); chave/valor
  inválido → exit 2. O `init` copia a global 1:1 (snapshot, E20-T18). O `login` (E21) liga **um**
  provider de agente (`opencode`/`opencode-zen`/`llama`) — sem `--provider` pergunta
  interativamente; a chave do opencode fica na config **global** (`katu.toml`, chave
  `opencode_api_key`, modo `0600`, fora do conjunto de chaves), e o embedding (`embeddings.*`)
  **não** é tocado. `--logout` apaga a chave guardada. Com um terminal interactivo, o login
  **verifica** a chave com um `chat/completions` mínimo (o `/models` não autentica) e reporta um
  `401`; um `KATU_OPENCODE_KEY` no ambiente que se sobreponha à chave guardada é avisado.
- **`memo`** — só consulta, espelhando o `kd`:
  - `memo ask [QUERY]` — consulta rica (espelha `kd ask`): filtros `--type`/`--class`/`--tag`/
    `--status`/`--scope`/`--anchor`, `--with-task`/`--full-content`/`--brief`, modos `--rank`/
    `--tags`/`--suggest`, `--id`/`--around`/`--via`/`--depth`, `--limit`/`--top-k`/`--relation`;
    `--since`/`--until`/`--as-of` aceites (semântica temporal adiada). Posicional = body; `--params`
    (XOR flags) e `--batch` JSONL como os restantes comandos (**E20-T06**).
  - `memo knowledge [--axis <e>] [--scope <c>] [--members] [--type/--class/--tag/--anchor]
    [--around/--depth] [--universe] [--limit N]` — **mapa estrutural** real de conhecimento
    (**E20-T06**); `--semantic`/`--communities`/`--write` adiados.
  - `memo doctor [--fix]` — diagnóstico do backend; `--fix` garante o layout do projeto (**E20-T07**).
  - `memo sessions` — lista as sessões do projeto (para `--resume`).
  - `memo drain [--status\|--digest [--force]]` — `--status` é read-only (não toca no índice);
    `--digest` drena a fila de embeddings (`--force` apaga `.idx/` e redigeri; **E20-T20**). Sem
    `embeddings.url` na config, fica **`off`** (não inventa endpoint; **E20-T17**).
  - `memo drain --watch-service [--install\|--subscribe\|--unsubscribe\|--uninstall]` — instala/remove
    um **timer systemd `--user`** que corre `--digest` nos projetos subscritos (**E20-T20**).
  - `memo prime [--long]` — prime do grupo.
  A **escrita** de memória é do **agente** (tools no loop) e do `kd`; o `memo` nunca cria notas.
- **`run`** — monta o runtime, escolhe o modelo (explícito ou por **tier**), corre **uma** rodada;
  as tool calls passam pela ordem §42. Saída humana: texto + **id da sessão em destaque** (após uma
  linha em branco); envelope: `text` + `session` + `round_exit`. O `usage` vem com `basis`.
- **`tui`** — igual ao `run`, mas entra no loop de eventos da UI (multi-turno). O `goal` interno é
  `"tui"`; o objetivo real de cada turno é a mensagem escrita na UI.

## 2. Flags do CLI

### 2.1 Globais

| Flag | O que faz | Default |
| --- | --- | --- |
| `--log-level <quiet\|error\|warn\|info\|debug\|trace>` | Nível do diagnóstico em `stderr`. | `quiet` |
| `--init` | Faz o bootstrap do `.katu/` e sai (layout + snapshot da config). | `false` |
| `--git-excluded` | Com `--init`: exclui o `.katu/` do git. | — |
| `--git-tracked` | Com `--init`: versiona o `.katu/` (default global). | — |
| `--force` | Com `--init`: refaz o bootstrap preservando só o conhecimento (`knowledge/`, `guardrails/`, `audit/`). | `false` |

### 2.2 Por comando

| Flag | Comandos | O que faz | Default |
| --- | --- | --- | --- |
| `--json` | `prime`, `upgrade`, `config`, `memo`, `run` | Envelope de máquina em `stdout`. | `false` |
| `--long` | `prime` | Acrescenta gramática e escopo. | `false` |
| `--group <g>` | `prime` | Grupo a emitir. | `global` |
| `--limit <N>` | `memo ask` | Máximo de resultados do recall. | `5` |
| `--params <JSON\|->` | `prime`, `run`, `tui`, `memo ask` | Config universal do comando (**XOR** com as flags; `-` = stdin). | ausente |
| `--batch <ficheiro\|->` | `prime`, `run`, `memo ask` | Lote JSONL (uma linha = um item). | ausente |
| `--provider <NAME>` | `run`, `tui` | Provider a usar. | `llama` |
| `--model <MODEL>` | `run`, `tui` | Modelo explícito (vence o tier). | pelo **tier** da fase |
| `--base <URL>` | `run`, `tui` | Base URL do endpoint. | por provider (§2.3) |
| `--thinking <GRAU>` | `run`, `tui` | Grau de pensamento (`off`/`low`/`medium`/`high`). | config, senão `off` |
| `--max-tokens <N>` | `run`, `tui` | Teto de tokens de saída. | `250000` |
| `--max-steps <N>` | `run`, `tui` | Máximo de passos (tool calls) por turno. | `100` |
| `--compact` | `run`, `tui` | Liga a compactação do histórico no turno (E09-T07). | `false` |
| `--resume [<ID>]` | `run`, `tui` | Retoma sessão. | sem valor = `last` |

**`--params`/`--batch` (E20-T08):** `--params '{…}'` (config universal do comando, **XOR** com as
flags explícitas; `-` = stdin) existe em `prime`/`run`/`tui`/`memo ask`; `--batch <ficheiro|->`
(JSONL; valida tudo antes de executar) em `prime`/`run`/`memo ask`.

**Defaults da config (E20-T17):** `provider`, `model`, `base`, `structured_output`,
`behavior.auto_compact`, `behavior.context_selection` (`suffix`/`utility`), `behavior.prompt_state`,
`behavior.tool_voi` (só atua com `suffix`), `behavior.durability` (`event`/`turn`; ADR 0024) e
`recall.default_limit` da
config efetiva (projeto > global) são o default de `run`/`tui`/`memo ask` (flags > `--params` >
config > default do comando). O `install.sh` prepara a global com `katu config init --global`
(idempotente; não sobrescreve). O `katu config login` escreve `provider`/`model`/`base` na **global**
(a chave, no `katu.toml` global, modo 0600), pelo que a escolha vale em todos os projetos até ser trocada.

**Contexto do turno (Q-02b/Q-03/Q-04, só por config):** `behavior.context_selection` escolhe a
política de seleção do contexto (`suffix` é o default, histórico e seguro; `utility` entra por
config explícita) e `behavior.prompt_state` liga a secção `estado` no prime (default **on**). O gate
de VOI (`behavior.tool_voi`, default **off**) só atua com `suffix`: com `utility` a unidade lida pode
ser descartada e o gate diria “já presente no contexto” sem o estar. Os números publicados são um
*proxy* de informação (`bench/e18/select/`) e a adoção pode ser revertida pela própria chave de
config. O envelope de `run --json` publica o que correu
(`context_selection`, `state`) para que a medição não dependa do que se supõe.

**Corte por loop (Q-12/F7):** o turno observa cada passo **antes** de executar as tools e corta um
ciclo de leitura sem progresso com `conflict` (exit `5`) e a mensagem
`loop detectado no passo N (sprt): …` — nunca em silêncio. O evento `agent.loop` fica no diagnóstico e
o turno é **fechado** no log (a sessão continua utilizável). Um ciclo que **escreve** (progresso) não é
cortado: `bash "make"` em *polling* é legítimo. Medido em `bench/e18/loop/`: **0** falsos positivos
em 200 turnos normais, corte no 4.º passo de um ciclo puro.

### 2.3 Defaults de provider/modelo/base

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
| `KATU_INSTRUMENT=1\|true` | Liga o sink de diagnóstico para `stderr` (feature `profile`), mesmo com `--log-level quiet`. |
| `KATU_INSTRUMENT_FILTER=<prefixo>` | Filtra os eventos por subsistema (ex.: `tui`, `provider`). |
| `KATU_OPENCODE_KEY` | Chave dos providers `opencode-go`/`opencode-zen` (**obrigatória**). |
| `USER` / `USERNAME` | Assinante (`granted_by`) de aprovações/overrides; default `local`. |

## 4. A TUI

Abre com `katu tui [flags de §2]` ou `katu` (sem verbo). A UI é **pura** (estado + keymap + render
em `katu-tui`); a borda (`crates/katu/src/tui/handler.rs`) é quem fala com o modelo e executa os
efeitos.

> **Nota (E20-T10…T16, onda S3):** `/` + mini-menus + ajuda `?` (**E20-T10**), `Esc`/`Ctrl-C` como
> cancelamento (**E20-T15**/L-P1), cópia por seleção de rato via OSC 52 (**E20-T14**) e *steering*
> FIFO (**E20-T16**) já estão implementados, tal como `/plan` (**E20-T11**) e `!`/`@` (**E20-T12**).
> `?` abre a sobreposição com todos os comandos `/`, os padrões (`!<cmd>`, `@<path>`, `/<comando>`) e
> as teclas.
>
> **Steering:** durante o turno, escrever e premir `Enter` enfileira um prompt que é aplicado no
> **passo seguinte** (FIFO), sem reiniciar a rodada; o buffer aparece na linha de entrada. `Esc` e
> `Ctrl-C` cancelam; o input é sondado mesmo com o stream parado (L-P1).

### 4.1 Layout

- **Cabeçalho** (1 linha): `katu · <modelo> · pensamento <grau> · fase <fase> · <estado>`
  e, quando existem, `· próximo <ação>` (checkpoint) e `· tokens … custo …` (uso do último turno).
- **Conversa** (esquerda): transcript da sessão, com scroll; teto de 200 entradas por quadro.
- **Atividade** (direita, 34 colunas): painel **efémero** — tools em curso (com **argumentos
  crus**), texto do modelo a chegar e recusas de política; teto de 100 linhas + cauda de 8 KiB do
  stream. Nunca entra no log nem na transcrição (§50.3).
- **Entrada** (3 linhas): linha de mensagem; o título muda com o modo.
- **Rodapé** (1 linha): dicas de teclas ou estado (`a trabalhar… · Esc/Ctrl-C cancela`, erros).

Render governado por orçamento (`Throttle`, ~60 fps, forçado em cada tecla/fim de turno); o
`ratatui` faz o **diff** de células.

### 4.2 Modos e teclas

Modos: `Normal` (navegação), `Insert` (edição/mensagem ou comando `/`), `Menu` (mini-menu de
`/model`/`/thinking`), `Help` (ajuda `?`), `Trash` (lixeira), `Transcript` (transcrição),
`Confirm` (**reservado** — ver §6) e o challenge de aprovação (sobreposição, §4.3).

| Modo | Tecla | Ação |
| --- | --- | --- |
| Normal | `q` | Sai da UI |
| Normal | `Enter` / `i` | Entra em `Insert` |
| Normal | `/` | Inicia um comando (`/model`, `/thinking`, …) |
| Normal | `?` | Abre a **ajuda** (comandos, padrões e teclas) |
| Normal | `↑` / `↓` | Rola a conversa |
| Insert | `Enter` | Submete a mensagem/comando (inicia o turno) |
| Insert | `Esc` | Volta a `Normal` (sem submeter) |
| Insert | `Backspace` | Apaga o último caractere |
| Insert | caracteres | Escrevem na mensagem (teclas `Ctrl` ignoradas) |
| Menu | `↑` / `↓` | Escolhe a opção |
| Menu | `Enter` | Confirma (aplica ao próximo turno) |
| Menu | `Esc` / `q` | Fecha o menu |
| Help | `Esc` / `q` / `?` | Fecha a ajuda |
| Trash | `↑` / `↓` | Escolhe a entrada |
| Trash | `r` | **Restaura** a entrada selecionada |
| Trash | `x` | **Esvazia** a lixeira (destrutivo; challenge) |
| Trash | `Esc` / `q` | Fecha a sobreposição |
| Transcript | `↑` / `↓` | Rola a transcrição |
| Transcript | `Esc` / `q` | Fecha a vista |
| qualquer | `Ctrl-C` | Sai da UI (em todos os modos) |

**Comandos `/`** (na linha de mensagem): `/model` (menu de modelos), `/thinking` (menu dos graus
**suportados pelo modelo**), `/login [opencode [chave] | llama [url]]` (menu interativo ou já com
argumentos; a chave é mascarada — E21), `/logout` (termina a sessão do agente), `/compact`,
`/verify`, `/trash`, `/transcript`, `/help`, `/quit`, `/skill:<nome>` (força o carregamento de uma
skill, E20-T13).
Comando desconhecido → erro na barra de estado (não é enviado ao modelo). `@<path>` **cita** um
caminho: fica pendente e é prefixado (só o caminho, sem conteúdo) ao próximo turno (E20-T12).
`!<cmd>` executa `sh -c` **pela política** (§42); no modo `/plan` é recusado com evidência
(`plan-no-shell`). `/plan` liga o modo de planeamento (regra `plan-write-only-katu`: escrita só sob
`.katu/`) e escreve `.katu/plan/<yymmddhhmmZ>-<slug>.md`; a barra de estado mostra `PLANO`.

O **contexto do turno** inclui o `AGENTS.md` da raiz (fonte de verdade máxima, no topo do prompt de
sistema) e o **catálogo de skills** descobertas em `.agents/skill{,s}/*/SKILL.md` (nome/descrição/
caminho); o modelo lê o `SKILL.md` com a tool `read` quando precisa (E20-T13).

**Rato:** arrastar com o botão esquerdo seleciona texto e copia-o automaticamente para o clipboard
via **OSC 52** (E20-T14); terminais sem suporte ignoram a sequência. A captura de rato é ligada no
arranque e desligada no restauro.

**Durante um turno** (estado `a trabalhar…`), **`Esc`** pede **cancelamento cooperativo**: o texto
parcial é registado, o turno fecha limpo e a UI mostra `turno cancelado` (evento `tui.cancel`).
`Ctrl-C`/`q` saem da UI (no loop principal), **não** cancelam a rodada (E20-T15).

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
**Atualizações**: `Assistant`, `Tool`, `Info`, `Error`, `Phase`, `Live`, `Models`,
`ThinkingOptions`, `NextAction`, `Usage`, `Trash`, `Transcript`, `Cancelled`, `Done`.

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

- **Verbos em esqueleto:** `upgrade`, `memo knowledge` — existem no contrato mas **recusam**
  (exit 10) até às tarefas E20-T02/T06.
- **`--params`/`--batch`** existem em `prime`/`run`/`tui`/`memo ask`; `config` (subcomandos) e os
  verbos em esqueleto ficam para quando tiverem dados.
- **Modo `Confirm`** existe no keymap (`y`/`n`) mas **nenhum fluxo o ativa** hoje: `Action::Confirm`
  apenas mostra `nada a confirmar`. É superfície **reservada**.
- **`run` é de um só turno**; o multi-turno vive na TUI.
- **Cancelamento é cooperativo** e só é lido durante o stream.
- **Kill do grupo de processos** (E07-T04) é o **único** ponto `unsafe` do projeto
  ([ADR 0018](../adr/0018-kill-do-grupo-com-unsafe-unico.md)).
