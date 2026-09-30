# SURFACE_IMPLEMENTATION — E20: Superfície v2 (CLI + TUI)

> **Épico dedicado** à reformulação global da superfície de utilizador do katu, inspirada no
> contrato do `kd` ([`crates/knudge/wiki/specs/cli.md`](crates/knudge/wiki/specs/cli.md)). Não
> substitui os épicos existentes: **consome** E01 (fundação), E03 (memória), E10 (TUI), E12
> (providers) e E14 (governança) e mantém os gates deles.
>
> **Estado:** ◐ em implementação (S0 + S1 + S2 + pendentes: verbos, `prime`, body/stdin, config,
> `--params`/`--batch` universais, bootstrap `.katu/`, `doctor --fix`, id+exit do `run`, defaults).
> **Gate:** `make check` verde +
> [`docs/CLI_TUI_SURFACE.md`](docs/CLI_TUI_SURFACE.md) atualizado + ADRs 0019–0023 aceites + teto
> de superfície ajustado.
>
> **Fonte da verdade do uso atual:** [`docs/CLI_TUI_SURFACE.md`](docs/CLI_TUI_SURFACE.md). As
> decisões novas vivem em ADRs (ver §4); o plano descreve *passos*, não decisões (§44).

---

## 0. Princípios (invariantes do épico)

| Id | Invariante | Como se fiscaliza |
|---|---|---|
| **I1** | **Intenção explícita:** o CLI **nunca** infere o que o utilizador quer. Sem verbo/config, falha fechado; nunca adivinha. | teste por verbo; `--params` explícito |
| **I2** | **stdout = dados, stderr = logs**; EPIPE = sucesso (D71/R20–R23 do `kd`). | testes de CLI |
| **I3** | **Fail-closed:** memória ausente, config inválida, id desconhecido, provider errado → recusa com exit code estável. | testes por código de saída |
| **I4** | **Um facto, um lar** (§44): uso em `docs/CLI_TUI_SURFACE.md`; decisões em ADR; passos aqui. | `xtask check-docs` |
| **I5** | **Todo o verbo tem `prime` e `help`** (compacto, byte-idêntico por versão). | `xtask check-surface` + testes de `prime` |
| **I6** | **Nenhum número afirmado** (DF5): preços/embeddings ausentes ficam `unpriced`/`off`. | testes |

---

## 1. Mapa requisito → tarefa

| # | Requisito (dono) | Tarefa | Onda |
|---|---|---|---|
| 1 | Intenção sempre explícita, sem inferência | **I1** + E20-T00 | S0 |
| 2 | `katu prime` + um `prime` por grupo | E20-T01 | S1 |
| 3 | `help` útil e objetivo em todo o lado | E20-T02 | S1 |
| 4 | `katu` só abre a TUI, inicializando `.katu` | E20-T03 | S2 |
| 5 | `run` cria sessão, executa, sai devolvendo id + exit code; `--resume` continua | E20-T04 | S2 |
| 6 | Posicional = body; aceita stdin (heredoc/pipe) | E20-T05 | S0 |
| 7 | Verbos só `[prime, upgrade, config, memo, run, tui]`; `memo` = **consulta/visão geral** (sem escrita), **espelhando o `kd`** | E20-T06 | S0 |
| 8 | Arranque de sessão corre `memo doctor --fix` | E20-T07 | S2 |
| 9a | `--params={object}` universal (**XOR** flags) | E20-T08 | S1 |
| 9b | `--batch` (JSONL por ficheiro/heredoc) | E20-T08 | S1 |
| 10 | Config global única + override local | E20-T09 | S1 |
| 11 | TUI por `/` (mini-menus): `/model`, `/thinking` adaptado ao modelo | E20-T10 | S3 |
| 12 | `/plan` modo de planeamento + `.katu/plan/yymmddhhmmZ-*.md` | E20-T11 | S3 |
| 13 | `!` shell e `@` citação (só caminho) | E20-T12 | S3 |
| 14 | `AGENTS.md` prioridade máxima + skills `.agents/skill{,s}/*/SKILL.md` | E20-T13 | S4 |
| 15 | Seleção com o rato copia automaticamente | E20-T14 | S3 |
| 16 | `Esc` é o único a parar a rodada sem fechar a sessão | E20-T15 | S3 |
| 17 | *Steering* linear/síncrono (input durante o HTTP) | E20-T16 | S3 |
| 18 | Provider/modelo/thinking padrão + embeddings (2 IAs: local + principal) | E20-T17 | S4 |
| 19 | Novo projeto copia 1:1 a config global | E20-T18 | S2 |
| 20 | `.katu/` layout central + versionamento (default **versionado**) | E20-T19 | S2 |
| 21 | `memo drain --watch-service --install\|subscribe\|unsubscribe\|uninstall` | E20-T20 | S2 |

---

## 2. Ondas

```
S0  Contratos + esqueleto        T00 · T06 · T05
S1  Textos + entrada + config    T01 · T02 · T08 · T09
S2  Sessão, layout e memo        T19 ── T07 · T18 · T20 ── T03 · T04
S3  TUI v2                       T10 · T11 · T12 · T14 · T15 ── T16
S4  Contexto e IA                T13 · T17
```

- **S0** fixa a decisão (ADR) e o **esqueleto de verbos** + convenção de entrada. É o núcleo de risco.
- **S1** acrescenta os textos estáticos (`prime`/`help`) e as vias de entrada (`--params`/`--batch`)
  e a configuração.
- **S2** faz o `.katu/` ser o centro e liga a sessão ao `memo doctor --fix`.
- **S3** é TUI (E10) — depende de S0/S2.
- **S4** é contexto/IA (E03/E12) — depende da config (T09).

---

## 3. Tarefas

### E20-T00 ◐ ADR da superfície v2 + doc de uso
- **Entregáveis:** ADRs 0019–0023 (§4); `docs/CLI_TUI_SURFACE.md` reescrito.
- **Feito:** ADR 0019 aceite; `docs/CLI_TUI_SURFACE.md` reescrito para a v2. Falta: ADRs 0020–0023.
- **Aceite:** ADRs com `## Alternatives considered`; `check-docs` verde; doc de uso descreve cada
  verbo/flag/tecla e o que **espera**.

### E20-T06 ◐ Verbos exclusivos e agrupamento `memo`
- **Feito:** topo = `prime`, `upgrade`, `config`, `memo`, `run`, `tui`; `memo` =
  `ask`/`knowledge`/`doctor`/`sessions`/`drain`/`prime`; verbos antigos → exit 2; `memo` não
  escreve (`remember`/`memory` removidos). Falta: `knowledge`/`drain` reais (stubs fail-closed) e
  as flags `kd` completas de `ask`.
- **Entregáveis:** topo = `prime`, `upgrade`, `config`, `memo`, `run`, `tui`; `memo` =
  `ask`/`knowledge`/`doctor`/`sessions`/`drain`/`prime`, **espelhando o `kd`** (flags em §5.1);
  `doctor`/`sessions`/`recall`/`remember`/`memory` deixam de existir no topo (sem
  retrocompatibilidade — caminho antigo exit 2). **`memo` não escreve memória** (sem
  `write`/`remember`/`forget`): a escrita é do agente (tools) e do `kd`.
- **Aceite:** `katu --help` lista só os 6 verbos; caminhos antigos falham com exit 2; nenhum
  subcomando de `memo` cria notas (teste); as flags de `memo ask`/`doctor`/`drain` cobrem as do
  `kd` correspondente.

### E20-T05 ☑ Posicional é body (stdin/heredoc/pipe)
- **Entregáveis:** em `run` (e `memo ask`) o posicional é **conteúdo**; `-` lê stdin; ausente +
  stdin não-TTY lê stdin; ausente + TTY → `invalid_input`.
- **Feito:** `crates/katu/src/cli/input.rs` (`resolve`); ligado a `run` e `memo ask`.
- **Aceite:** `echo x | katu run` e `katu run - <<< x` equivalentes; sem conteúdo e TTY → exit 2.

### E20-T01 ☑ `prime` global e por grupo
- **Feito:** `katu prime [--long] [--group <g>]`, texto estático por grupo (`cli/prime.rs`).
- **Entregáveis:** `katu prime [--long] [--group <g>]` (g ∈ global/memo/run/tui/config/upgrade);
  texto **estático**, byte-idêntico por versão, otimizado para IA e utilizador (uso, comandos,
  dados a observar).
- **Aceite:** duas invocações devolvem o mesmo byte-a-byte; `--long` acrescenta gramática/escopo;
  cada grupo tem prime não vazio (teste por grupo); o prime **não** inclui o `AGENTS.md` (esse
  entra no contexto do turno, T13).

### E20-T02 ☑ `help` útil e objetivo
- **Feito:** `about` por verbo (doc comments), `long_about`/`after_help` na raiz com o ciclo de uso e
  **`after_help` por grupo** (`memo`, `config`); `katu help [verbo]` do `clap`.
- **Entregáveis:** `about`/`long_about` por verbo e por grupo; `after_help` com ciclo de uso;
  `katu help [verbo]`.
- **Aceite:** cada verbo explica o que faz, o que espera e o que **não** faz; teste de presença.

### E20-T08 ☑ `--params` universal (XOR) e `--batch` JSONL
- **Feito:** `--params` (XOR com flags, `deny_unknown_fields`, `-` = stdin) em `run`, `tui`, `prime`
  e `memo ask`; `--batch` (valida tudo antes de executar) em `run`, `prime` e `memo ask`. `tui` não
  tem `--batch` (uma UI por invocação).
- **Entregáveis:** `--params '{...}'` representa **toda** a config de um comando; `--params -` lê
  de stdin; **`--params` e flags explícitas são exclusivos** — usar ambos → `invalid_input`
  (exit 2), sem inferência; `--batch <ficheiro|->` processa JSONL (uma linha = um item).
- **Aceite:** `--params` sozinho equivale às flags; `--params` + flag → exit 2; `--params` e
  `--batch` exclusivos; lote vazio é no-op; linha inválida recusa sem executar as restantes.

### E20-T09 ☑ Configuração global única + override local
- **Feito:** `crates/katu/src/config.rs` (conjunto fechado, paths por SO, merge projeto>global) +
  `cli/config_cmd.rs` (`get/set/unset/list [--global]`); os padrões efetivos alimentam `run`/`tui`
  (`provider`/`model`/`base`/`behavior.auto_compact`) e `memo ask` (`recall.default_limit`) via
  `src/defaults.rs`.
- **Entregáveis:** `katu.toml` global em `~/.config/local/katu/katu.toml` (Linux),
  `~/Library/Application Support/katu/katu.toml` (macOS), `%APPDATA%\katu\katu.toml` (Windows);
  override em `<projeto>/.katu/katu.toml`; `katu config get/set/unset/list [--global]`;
  criação da cadeia de pastas; conjunto fechado de chaves (valor inválido → lista + sugestão).
  **Snapshot 1:1** no `init`: o projeto copia a config global e **não** segue mudanças globais
  futuras; `config set` local edita o snapshot; precedência é do ficheiro do projeto.
- **Aceite:** precedência projeto > global (teste); chave desconhecida recusada; `config set`
  gera diff de uma linha; segredos só no global; alterar o global não altera um projeto já
  inicializado (snapshot).

### E20-T19 ☑ `.katu/` layout central + versionamento
- **Feito:** `crates/katu/src/bootstrap.rs` (layout idempotente, snapshot, guardrails, blocos
  geridos em `.gitignore`/`.gitattributes`/`.git/info/exclude`) + `bootstrap/git.rs`; `--init` ligado
  em `cli.rs`, com `--git-excluded`/`--git-tracked` e **`--force`** (preserva só
  `knowledge/`/`guardrails/`/`audit/`).
- **Entregáveis:** `.katu/{katu.toml,audit,knowledge/{notas,.idx},guardrails,trash,log,plan}`;
  `guardrails/` copia as travas padrão (globais em `~/.config/local/katu/guardrails/`) e é a fonte
  local; `trash/` e `log/` git-excluded; **`audit/`, `trash/` e `log/` ficam sempre excluídos**; a
  config decide se o resto do `.katu/` é versionado (**default global: versionado**);
  `katu --init --git-excluded` / `--git-tracked` força um repositório; `katu --init --force` refaz
  o bootstrap preservando só `knowledge/`/`guardrails/`/`audit/`;
  criar/atualizar `.gitignore`, `.gitattributes` e `.git/info/exclude` quando em repo git
  (criar se faltarem).
- **Aceite:** layout idempotente; `--git-excluded` insere a linha no `.git/info/exclude`;
  `--git-tracked` remove-a; `--force` preserva o conhecimento e reseta o resto; `audit/`/`trash/`/
  `log/` nunca versionados; ficheiros git criados quando ausentes; teste por caso.

### E20-T07 ☑ `memo doctor --fix` no arranque da sessão
- **Feito:** `memo doctor --fix` = `bootstrap::ensure_current` (idempotente); o arranque de sessão
  (`run`/`tui`/default TUI) corre o mesmo `ensure` antes do primeiro turno.
- **Entregáveis:** abrir sessão (CLI e TUI) corre `memo doctor --fix` (idempotente) antes do
  primeiro turno; reporta o que corrigiu.
- **Aceite:** arranque num `.katu/` incompleto repara e continua; `--fix` não altera nada num
  projeto saudável (no-op determinístico).

### E20-T18 ☑ Cópia 1:1 da config global no `init`
- **Feito:** `bootstrap::ensure_config` copia a global byte-a-byte (ou escreve o default, sem
  global) quando `<projeto>/.katu/katu.toml` não existe.
- **Entregáveis:** ao inicializar um projeto, copiar **literalmente** a config global para
  `<projeto>/.katu/katu.toml` (segredos ficam no global).
- **Aceite:** cópia byte-a-byte do que não é segredo; sem global, escreve o default.

### E20-T20 ◐ `memo drain` e worker de embeddings
- **Feito:** `memo drain --status` (read-only, não toca no índice) e **`--digest [--force]`** —
  dreno real pelo pipeline do `knudge-core` (`http`/`lightweight`/`none`, da config do knudge; o
  bridge para as chaves `embeddings.*` do `katu.toml` fica na E20-T17); `--force` apaga `.idx/`
  (derivado) e redigeri; fail-closed sem provedor (não inventa endpoint); o conhecimento vive em
  `.katu/knowledge` (E20-T19). Falta: `--watch-service` (worker de auto-drain) — contrato presente,
  ainda fail-closed.
- **Entregáveis:** `memo drain` (`--status`/`--digest [--force]`) e `memo drain --watch-service
  --install|--subscribe|--un-subscribe|--uninstall` (provedor de embedding, reutilizando o worker
  `knudge-idle.sh` e o provedor HTTP do knudge).
- **Aceite:** `--status` não toca no índice; `--digest` esvazia/estagna; `--force` reconstrói;
  `--watch-service` instala e remove o worker (**pendente**); fail-closed sem provider.

### E20-T03 ◐ `katu` sozinho abre a TUI e inicializa o projeto
- **Feito:** default TUI com `ensure` (bootstrap) e falha fechada sem TTY. Falta: a TUI v2 (S3).
- **Entregáveis:** comando opcional (default `tui`); bootstrap do `.katu` no arranque
  (T19) + `memo doctor --fix` (T07); se `stdout` não for TTY, falha fechado com erro claro.
- **Aceite:** `katu` sem verbo abre a UI num TTY; sem TTY devolve erro de I/O (não bloqueia);
  `.katu/` criado na primeira execução.

### E20-T04 ☑ `run` como sessão de uma rodada
- **Feito:** o envelope inclui `session` + `round_exit`; o texto humano destaca o id após uma linha
  em branco; **teste e2e live** (`crates/katu/tests/live.rs`, gated por `KATU_LLAMA_URL`) prova que
  o id devolvido é aceito por `--resume` na execução seguinte e que um id desconhecido recusa
  (`invalid_input`, exit 2 — corrigido o mapeamento de `UnknownSession`).
- **Entregáveis:** `katu run [BODY]` cria sessão, executa **uma** rodada, sai e devolve o
  **exit code da rodada** e o **id da sessão em destaque** (texto humano: id após uma linha em
  branco; envelope: campos `session` e `round_exit`, com `exit` do processo = o da rodada);
  `--json` (por comando) faz a saída final ser o envelope; `katu run --resume <ID> <BODY>`
  continua a mesma sessão; `--resume` sem valor = a mais recente.
- **Aceite:** o id devolvido é aceito por `--resume` na execução seguinte (e2e); id desconhecido
  recusa (exit 2); o texto mostra o id em destaque.

### E20-T10 ☐ TUI por comandos `/`
- **Entregáveis:** entrada por `/` com mini-menus; `/model` (substituir modelo) e `/thinking`
  (submenu **adaptado às capacidades** do modelo, aberto automaticamente ao mudar de modelo);
  `/help`, `/compact`, `/verify`, `/trash`, `/transcript`, `/quit`. **`?` abre uma sobreposição de
  ajuda** com todos os comandos `/`, os padrões (`!<cmd>`, `@<path>`, `/<comando>`) e as teclas,
  disponível em qualquer modo (incl. durante um turno).
- **Aceite:** teclas de atalho antigas removidas ou delegadas; o submenu de thinking só oferece
  graus que o modelo suporta; `?` mostra a lista completa de comandos/padrões e fecha com `Esc`;
  testes de keymap puros.

### E20-T11 ☐ `/plan` — modo de planeamento
- **Entregáveis:** `/plan` é um **estado do runtime** que injeta uma regra de política
  **Enforced** “escrita só sob `.katu/`” (registada no log); os planos são `.md` densos em
  `.katu/plan/yymmddhhmmZ-*.md`, gerados pelo sistema de tarefas do knudge.
- **Aceite:** no modo plano, uma tool de escrita fora de `.katu/` é **negada** com evidência
  (teste pelo caminho real); a regra aparece no log; o nome do ficheiro segue o formato UTC.

### E20-T12 ☐ `!` shell e `@` citação
- **Entregáveis:** `!<cmd>` executa shell a partir da raiz de inicialização **pela
  política/contenção** (como a tool `bash`) e é **bloqueado no modo `/plan`**; `@<path>` cita
  ficheiro/diretório e passa **só o caminho exato** ao modelo (sem anexar conteúdo).
- **Aceite:** `!` passa pela política (negação com evidência) e é recusado no `/plan`; `@` injeta
  o caminho citado no contexto do próximo turno e nada mais.

### E20-T14 ☐ Cópia por seleção de rato
- **Entregáveis:** selecionar texto na TUI copia-o automaticamente para o clipboard via **OSC 52**
  (sem dependência nova; mantém o firewall `katu-tui`).
- **Aceite:** teste do caminho de cópia (escrita OSC 52 abstrata); terminal sem suporte degrada
  sem erro.

### E20-T15 ☐ `Esc` para a rodada sem fechar a sessão
- **Entregáveis:** `Esc` é o **único** comando que interrompe a rodada corrente mantendo a sessão
  de conversa aberta; `Ctrl-C`/`q` continuam a sair da UI.
- **Aceite:** após `Esc`, o próximo turno continua a mesma sessão; `q` sai; testes.

### E20-T16 ☐ *Steering* linear/síncrono
- **Entregáveis:** uma **thread leitora de input** só durante o turno alimenta uma fila **FIFO**;
  ela é consultada **durante o HTTP** (entre chunks/passos) e o prompt empilhado é enviado no passo
  seguinte, com prioridade. É **linear/síncrono** do ponto de vista do humano; sem executor
  concorrente.
- **Aceite:** um prompt de steering é aplicado no passo seguinte sem reiniciar a rodada; ordem FIFO;
  testes com provider falso.

### E20-T13 ☐ `AGENTS.md` e skills
- **Entregáveis:** `AGENTS.md` da raiz é a **fonte de verdade máxima** do projeto no contexto;
  `.agents/skill{,s}/*/SKILL.md` são skills acionáveis (descobertas e oferecidas).
- **Aceite:** o prime/contexto inclui o `AGENTS.md`; uma skill é acionável por nome; ausência não
  quebra o arranque.

### E20-T17 ◐ Padrões globais + embeddings (2 IAs, serviço externo)
- **Feito:** os padrões de `provider`/`model`/`base`/`behavior.auto_compact`/`recall.default_limit`
  ligam-se a `run`/`tui`/`memo ask` (flags > `--params` > config > default do comando). Falta: o
  sistema de embeddings (URL/modelo/comando) e a segunda IA.
- **Entregáveis:** config global de provider/modelo/thinking padrão e do sistema de embeddings;
  operação com **duas** IAs (embedding + execução). O embedding reutiliza o modelo recomendado do
  knudge e é **sempre um serviço externo plugável** por config: **URL** (`http://host:porta/v1`)
  + nome do modelo, com `command` **opcional** para o katu lançar o `llama-server`. Sobrescrevível
  localmente.
- **Aceite:** default aplicado quando não há override; endpoint de embedding ausente fica `off`
  (nunca inventado); teste de precedência; nenhuma dependência de rede embutida.

---

## 4. Decisões a registar (ADRs)

| ADR | Tema |
|---|---|
| **0019** | Superfície v2: verbos, entrada por body, `prime`/`help`, `memo` (só consulta) |
| **0020** | Configuração global/local (`katu.toml`), precedência e `--params` (XOR flags) |
| **0021** | `.katu/` como layout central e política de versionamento (default versionado) |
| **0022** | TUI por comandos `/`, steering linear e modo de planeamento |
| **0023** | Duas IAs (embedding + execução), serviço externo plugável |

---

## 5. Decisões tomadas (dono)

### Superfície e verbos
1. **`upgrade`** — **stub reservado**: sem canal agora; sincronização futura contra **GitHub
   Releases**. Hoje reporta explicitamente que o canal não está configurado (fail-closed).
2. **`memo`** — **só consulta/visão geral** e **espelha o `kd`** (§5.1). **Não** escreve memória:
   a escrita é do **agente** (tools no loop) e do `kd`; `remember`/`write`/`forget` saem da
   superfície do katu.
3. **`--params` vs flags** — **exclusivos**: ou `--params` (todas as chaves), ou flags explícitas;
   ambos → `invalid_input` (exit 2).
4. **`run`** — devolve o **exit code da rodada** e o **id da sessão** em destaque (texto: id após
   uma linha em branco; envelope: `session` + `round_exit`, com `exit` do processo = o da rodada).
5. **`--json`** — **por comando**, não global: existe em `run`, `prime`, `config`, `memo`,
   `upgrade`; **não** existe em `tui`. `katu --json` (sem verbo) **deixa de existir**.
6. **`--log-level`** — flag **global** (stderr), default **`quiet`**; **`--quiet` não existe**.
7. **`--init`** — **flag de topo** (não verbo): `katu --init [--git-excluded|--git-tracked]` faz
   bootstrap e sai; `katu` sem flag abre a TUI.
8. **`katu` sem TTY** — erro de I/O fechado (não bloqueia).
9. **`prime` por grupo** — todo o grupo com subcomandos expõe `<grupo> prime` **equivalente** a
   `katu prime --group <grupo>` (`memo prime`, `config prime`); os verbos-folha (`run`, `tui`,
   `upgrade`) usam `katu prime --group <g>`.

### Config e layout
9. **Config** — global única + **snapshot 1:1** no `init` (o projeto não segue mudanças globais
   futuras); `config set` local edita o snapshot; precedência é do ficheiro do projeto.
10. **`.katu/`** — default global **versionado**; `audit/`, `trash/` e `log/` ficam **sempre**
    git-excluded; `--git-excluded` exclui a árvore inteira e `--git-tracked` força o resto.
11. **Embeddings** — **reutilizar** o knudge (mesmo modelo recomendado, mesmo `llama.cpp`), como
    **serviço externo por URL** (`http://host:porta/v1`) + nome do modelo; opcionalmente `command`
    para o katu lançar o `llama-server`. Nada de rede embutida.

### TUI e agente
12. **`prime` não inclui `AGENTS.md`** (mantém-se byte-idêntico); o `AGENTS.md` entra no
    **contexto do turno** como fonte de verdade máxima.
13. **`/plan`** — **estado do runtime** que injeta uma regra **Enforced** “escrita só sob `.katu/`”,
    registada no log.
14. **`!` shell** — passa pela **política/contenção** (como a tool `bash`) e é **bloqueado no
    modo `/plan`**.
15. **Steering** — **thread leitora de input** só durante o turno → fila **FIFO** consultada entre
    chunks/passos; linear do ponto de vista do humano.
16. **Cópia por rato** — **OSC 52** (sem dependência nova).
17. **`@`** — **só o caminho** (sem anexar conteúdo).

### 5.1 `memo` espelha o `kd` (versão CLI)

| `memo` | Espelha | Flags (inspiradas no `kd`) |
|---|---|---|
| `ask` | `kd ask` | `--limit` `--anchor` `--type` `--class` `--tag` `--status` `--scope` `--since` `--until` `--brief` `--full-content` `--id` `--around` `--rank` `--tags` `--params` `--batch` |
| `knowledge` | `kd map` (+ `ask --rank`/`--tags`) | `--axis` `--scope` `--semantic` `--members` `--universe` |
| `doctor` | `kd doctor` | `--fix` `--explain` |
| `sessions` | (índice temporal do katu) | — |
| `drain` | `kd drain` | `--status` `--digest [--force]`; `--watch-service --install\|--subscribe\|--unsubscribe\|--uninstall` |
| `prime` | — | `--long` |

Chaves do `katu.toml` e nomes seguem o `kd`/knudge onde existirem. **Sem** `write`/`remember`/`forget`.

---

## 6. Definition of Done (do épico)

- [ ] `cargo fmt --check` + `clippy --workspace --all-targets -- -D warnings` verdes.
- [ ] `cargo test --workspace` verde, com testes por verbo/flag/tecla nova.
- [ ] `make check` verde (inclui `check-docs`, `check-surface`, `check-catalog`).
- [ ] `docs/CLI_TUI_SURFACE.md` reescrito e ligado no router.
- [ ] ADRs 0019–0023 aceites; nenhuma ADR editada para outra decisão.
- [ ] `surface.toml` ajustado (verbos/diag/xtask) se a superfície crescer.
- [ ] Ficheiros de produção ≤ 300 linhas; zero `unwrap`/`expect`/`panic` em `src/`.
- [ ] `unsafe` continua a ser **um** ponto (ADR 0018).
- [ ] Nenhum número/serviço afirmado sem artefacto (DF5).

## 7. Impacto na superfície existente

- **Remove:** `version` (fica `--version`), `doctor`/`sessions`/`recall`/`remember`/`memory` do
  topo (passam a `memo`; `remember` deixa de existir); **`--json` global** e **`--quiet`**.
- **Adiciona:** `prime`, `upgrade`, `config`, `memo` (`ask`/`knowledge`/`doctor`/`sessions`/`drain`);
  flags `--params`/`--batch`/`--group`/`--log-level` (default `quiet`) e `--json` **por comando**;
  comandos `/` na TUI.
- **Diag:** novos eventos previstos (`cli.prime`, `memo.drain`, `tui.slash`, `plan.mode`,
  `tui.steer`, `mouse.copy`) — atualizar `surface.toml` e regenerar `docs/catalog.md`.
- **Testes:** a suíte atual de CLI (topo) muda; os testes de `run`/`--resume`/cancelamento mantêm-se.
