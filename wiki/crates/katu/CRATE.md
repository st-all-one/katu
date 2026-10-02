# `katu` — Binário: CLI, Composição e Adaptador In-Process

**Épico:** E01/E04 · **Fase:** 0/2 · **Crate:** `crates/katu`

O **binário** do katu: CLI (`clap`), composição das portas, adaptador in-process do `knudge`,
loop de turnos, TUI e worker de auto-drain. É aqui que vive todo o código impuro confinado.

---

## 1. Visão Geral

O crate `katu` é o **ponto de entrada** do sistema. Responsável por:

- **CLI** (`clap`): verbos exclusivos `prime`/`upgrade`/`config`/`memo`/`run`/`tui`
- **Composição**: wiring das portas (`Clock`, `Rng`, `Fs`, `Env`, `Logger`)
- **Adaptador in-process** do `knudge-core` (memória de primeira classe)
- **Loop de turnos**: provider ↔ kernel, tool execution pela ordem §42
- **TUI**: UI de terminal sobre o loop de turnos
- **Worker de auto-drain**: agendamento systemd para dreno de embeddings

**Fronteira:** Pode depender de todos os crates, mas mantém o **firewall** a montante: os crates
puros não dependem dele.

---

## 2. Arquitetura

### 2.1 Composição

```
┌─────────────────────────────────────────────────────────────────┐
│                         katu (binário)                          │
├─────────────────────────────────────────────────────────────────┤
│  CLI (clap)  │  Runtime  │  Agent (loop)  │  TUI  │  Watch    │
├─────────────────────────────────────────────────────────────────┤
│                    Portas (traits)                              │
│  Clock │ Rng │ Fs │ Env │ Process │ Memory │ Provider           │
├─────────────────────────────────────────────────────────────────┤
│                    Adaptadores                                  │
│  SystemClock │ StdRng │ StdFs │ StdEnv │ StdProcess │ Knudge   │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2 Módulos

| Módulo | Responsabilidade |
|--------|------------------|
| `main.rs` | Entry point, setup de diagnóstico |
| `cli.rs` | Superfície CLI (`clap`) |
| `cli/` | Subcomandos: `prime`, `upgrade`, `config`, `memo`, `run`, `tui`, `sessions` |
| `config.rs` | Configuração (conjunto fechado de chaves, merge projeto > global) |
| `defaults.rs` | Padrões da config efetiva |
| `bootstrap.rs` | Bootstrap do `.katu/` (layout idempotente, snapshot da config) |
| `ports/` | Adaptadores das portas (`Clock`, `Rng`, `Fs`, `Env`, `Process`) |
| `memory/` | Adaptador in-process do `knudge-core` |
| `runtime.rs` | Runtime do agente (ponto de composição do loop) |
| `runtime/` | Submódulos: `context`, `verify`, `memory`, `control`, `skills`, `transcript`, `plan_mode`, `checkpoint` |
| `agent/` | Loop de turnos (provider ↔ kernel) |
| `agent/turn/` | Execução de turno: `run`, `batch`, `voi`, `request` |
| `agent/router/` | Roteador de tool calls (JSON → `ToolUse` resolvido) |
| `tui.rs` | Comando `tui` (UI de terminal) |
| `tui/` | Submódulos: `handler`, `control`, `transcript`, `trash`, `verify` |
| `watch_service.rs` | Worker de auto-drain (systemd timer) |
| `scope.rs` | Contrato de escopo e `feature_list` |
| `tier.rs` | Política de tiers (fase → classe de modelo) |
| `pricing.rs` | Preços por modelo (transparência de custo) |
| `report.rs` | Envelope de máquina (`--json`) |
| `diag.rs` | Sink de diagnóstico para `stderr` |

---

## 3. Módulos em Detalhe

### 3.1 CLI (`src/cli.rs` + `src/cli/`)

**Verbos exclusivos** (sem inferência):

- `prime`: contexto de arranque estático (byte-idêntico por versão)
- `upgrade`: sincronização de versão (canal ainda não configurado)
- `config`: configuração global e do projeto (`get`/`set`/`unset`/`list`)
- `memo`: consulta e visão geral da memória (`ask`/`knowledge`/`doctor`/`sessions`/`drain`/`prime`)
- `run`: uma rodada do agente
- `tui`: UI de terminal (multi-turno)

**Superfície v2 (E20):**
- `--params` (XOR com flags) e `--batch` JSONL
- `--json` por comando
- `--log-level` global (default `quiet`)
- Sem subcomando, `katu` abre a TUI

**Padrões da config efetiva** (`src/defaults.rs`):
- `provider`/`model`/`base`/`thinking`/`behavior.auto_compact`/`recall.default_limit`
- Alimentam `run`/`tui`/`memo ask`

### 3.2 Configuração (`src/config.rs`)

**Conjunto fechado de chaves** (sem inferência):

```rust
pub(crate) const KEYS: &[KeySpec] = &[
    KeySpec { key: "provider", kind: Kind::Text, doc: "Provider por omissão." },
    KeySpec { key: "model", kind: Kind::Text, doc: "Modelo por omissão." },
    KeySpec { key: "thinking", kind: Kind::Text, doc: "Grau de pensamento por omissão." },
    // ...
];
```

**Precedência:** projeto > global (merge chave a chave)

**Caminhos:**
- Global: `~/.config/local/katu/katu.toml`
- Projeto: `<root>/.katu/katu.toml`

### 3.3 Bootstrap (`src/bootstrap.rs`)

**Layout idempotente** do `.katu/`:

```
.katu/
├── audit/          # sempre fora do git
├── knowledge/      # notas + .idx/
├── guardrails/     # guardrails do projeto
├── trash/          # sempre fora do git
├── log/            # sempre fora do git
├── plan/           # artefactos de plano
└── katu.toml       # snapshot da config
```

**Versionamento:**
- `audit/`, `trash/`, `log/` ficam **sempre** fora do git
- O resto segue `git.versioned` (default `true`)

### 3.4 Portas (`src/ports/`)

**Adaptadores finos** (único sítio que toca o SO):

| Adaptador | Porta | Responsabilidade |
|-----------|-------|------------------|
| `SystemClock` | `Clock` | Relógio do sistema |
| `StdRng` | `Rng` | RNG do sistema (jitter) |
| `StdFs` | `Fs` | Sistema de ficheiros real (escrita atômica endurecida) |
| `StdEnv` | `Env` | Ambiente real do processo |
| `StdProcess` | `Process` | Execução real com timeout e process group |

**Escrita atômica endurecida** (`StdFs`):
- Temporário exclusivo `O_EXCL`/`0600`
- Nome imprevisível (contador atômico + PID)
- `fsync` antes do `rename`

**Process group** (`StdProcess`):
- Filho corre com o **utilizador** que evocou o katu
- No timeout, o grupo inteiro é morto com `kill(-pgid, SIGKILL)`
- **Único ponto `unsafe`** do projeto (ADR 0016)

### 3.5 Memória (`src/memory/`)

**Adaptador in-process** da porta `Memory` sobre o `knudge-core`:

- **Fachada `Knudge`** é `!Sync`, protegida por `Mutex`
- **Cache de índice/grafo** invalidado por cada escrita
- **`pre_edit`** decide *supersede* em *dry-run* fiel ao `update`
- **`memo doctor`** expõe `memory.status()` e a suíte de conformidade

**Paridade `memo`/`kd`** (E20-T06):
- Filtros, modos `--rank`/`--tags`/`--suggest`
- `--id`/`--around` e o mapa estrutural (`memo knowledge`)

**Dreno de embeddings** (`src/memory/drain.rs`):
- `memo drain --digest [--force]` drena a fila de embeddings
- Fail-closed sem provedor
- Worker de auto-drain (`--watch-service`) vive em `src/watch_service.rs`

### 3.6 Runtime (`src/runtime.rs`)

**Ponto de composição** do loop real:

- Descobre a raiz do projeto
- Abre o adaptador de memória
- **Recusa arrancar** sem memória saudável (fail-closed)
- Expõe `recall`/`remember` pelo caminho §42

**Submódulos:**

| Módulo | Responsabilidade |
|--------|------------------|
| `context.rs` | Contexto efetivo + compactação |
| `verify.rs` | Gate de verificação sobre o log |
| `memory.rs` | Caminhos de recall/escrita |
| `control.rs` | Controlo de modelo/pensamento |
| `skills.rs` | Contexto do projeto (`AGENTS.md` + skills) |
| `transcript.rs` | Transcrição durável (projeção do log) |
| `plan_mode.rs` | Modo de planeamento (regra `plan-write-only-katu`) |
| `checkpoint.rs` | Checkpoint de fase visível na UI |

### 3.7 Agent (`src/agent/`)

**Loop de turnos** (E12-T05/E10):

- Liga o provider ao kernel
- Monta `ProviderRequest` via `Session::context`
- Executa cada tool call pela ordem §42 (logar → política → efeito)
- **Guard de loop** (Q-12/F7): CUSUM + e-value *anytime-valid*

**Submódulos:**

| Módulo | Responsabilidade |
|--------|------------------|
| `catalog.rs` | Catálogo de tools enviado ao modelo |
| `command.rs` | Comando `run` (monta runtime + provider) |
| `plan.rs` | Execução da tool `plan` |
| `shell.rs` | Execução de `!<cmd>` |
| `router/` | Roteador de tool calls |
| `turn/` | Execução de turno |

### 3.8 Router (`src/agent/router/`)

**Roteador de tool calls** (E12-T05):

- Mapeia os argumentos JSON do modelo num `ToolUse` resolvido
- Resolve caminhos **antes** do veredicto (E07-T02)
- **Fail-closed**: argumento em falta, view inválida ou tool desconhecida **não** executam nada

**Famílias de tools:**

| Tool | Descrição |
|------|-----------|
| `read` | Leitura de ficheiro (view/range/símbolo) |
| `write` | Escrita de ficheiro novo |
| `edit` | Patch otimista (uma ou várias substituições atómicas) |
| `move` | Movimentação de ficheiro |
| `search` | Busca (modos `ls`/`grep`/`glob`) |
| `exec` | Execução de comando (`sh -c`) |
| `trash` | Lixeira (mover/restaurar/esvaziar) |
| `plan` | Validação/registo de plano |
| `memory` | Recall/escrita de memória |

### 3.9 Turn (`src/agent/turn/`)

**Execução de turno** (E12-T05/E10):

| Módulo | Responsabilidade |
|--------|------------------|
| `run.rs` | Loop de passos (provider → sink efémero → tools §42) |
| `batch.rs` | Execução concorrente de tool calls `Shared` |
| `voi.rs` | Gate de Value of Information |
| `request.rs` | Montagem do pedido ao provider |

**Lote paralelo** (B-01):
- As calls `Shared` de um lote correm em `std::thread::scope`
- Spawn **eager** (encadear spawn/join serializa o lote)
- Teto é `MAX_PARALLEL_CALLS = 8` (limite de **custo**, não de paralelismo)

**Gate de VOI** (A3/W8-4):
- Não repete uma só-leitura já satisfeita no turno
- **Nunca** salta o irreconstruível
- **Opt-in** (`behavior.tool_voi`, default **off** até A/B com o modelo); só atua com a seleção
  `suffix` (com `utility` a unidade lida pode ser descartada e o gate mentiria)

### 3.10 TUI (`src/tui.rs` + `src/tui/`)

**UI de terminal** (E10-T01/T02/T05):

- A UI (`katu-tui`) é **pura** (estado central + keymap + render)
- A borda implementa o `Handler` que corre o turno e injeta `Update`s
- **Streaming ao vivo** para o painel de atividade (sem entrar no log)
- **Steering** consultado **entre passos** e injetado como mensagem de utilizador

**Submódulos:**

| Módulo | Responsabilidade |
|--------|------------------|
| `handler.rs` | Executor do loop de turnos para a UI |
| `control.rs` | Controlo de modelo/pensamento na TUI |
| `transcript.rs` | Transcrição durável em ficheiro + vista read-only |
| `trash.rs` | Lixeira na TUI (listar, restaurar, esvaziar) |
| `verify.rs` | Gate de verificação e override humano |

### 3.11 Watch Service (`src/watch_service.rs`)

**Worker de auto-drain** (E20-T20):

- Instala/remove o agendador que corre `katu memo drain --digest`
- Usa um **timer systemd `--user`** (Linux)
- **Fail-closed**: se o `systemctl` faltar ou falhar, o comando recusa com `unavailable`

---

## 4. Abordagens de Engenharia

### 4.1 Arquitetura em Camadas

```
┌─────────────────────────────────────────────────────────────────┐
│  Borda (CLI/TUI)                                                │
├─────────────────────────────────────────────────────────────────┤
│  Adaptadores (Portas)                                           │
├─────────────────────────────────────────────────────────────────┤
│  Núcleo (Kernel, Policy, Memory, Provider)                      │
└─────────────────────────────────────────────────────────────────┘
```

**Regra:** Os crates puros não dependem do binário. O binário depende de todos.

### 4.2 Portas e Adaptadores

**Portas** (traits em `katu-core`):
- `Clock`, `Rng`, `Fs`, `Env`, `Process`, `Memory`, `Provider`

**Adaptadores** (em `crates/katu`):
- `SystemClock`, `StdRng`, `StdFs`, `StdEnv`, `StdProcess`, `KnudgeMemory`

**Benefício:** Testabilidade (portas falsas) e substituibilidade (adaptadores reais vs. falsos).

### 4.3 Ordem §42 (Logar → Política → Efeito)

Toda a execução de tools segue a ordem:

1. **Logar**: `Session::tool_call` regista o pedido
2. **Política**: `katu-policy` decide (Allow/Deny/RequireApproval)
3. **Efeito**: Executor corre a tool

**Garantia:** `Model-visible ⟺ logged` (E04)

### 4.4 Fail-Closed

**Princípio:** Na dúvida, recusa.

**Exemplos:**
- Sem memória saudável → recusa arrancar
- Sem provedor de embeddings → não indexa nem falha
- Sem `scope_contract.json` → tool `plan` indisponível
- Sem `AGENTS.md` → fail-open (não quebra o arranque)

### 4.5 Dado Versionado (DF3)

**Regras e políticas** são **dado**, não código:

- `policy/memory.toml` — regras do protocolo de memória
- `policy/containment.toml` — regras de contenção (soft)
- `policy/tiers.toml` — política de tiers (fase → classe de modelo)
- `policy/prices.toml` — preços por modelo

**Benefício:** Auditable, versionado no repositório, sem recompilação.

### 4.6 Instrumentação Estruturada (DF9/E19)

**Sink de diagnóstico** (`src/diag.rs`):
- Formato estável: `level=… kind=… [function=…] event=… dur_ns=… fields=…`
- Redação de campos sensíveis (tokens, chaves)
- `--log-level` controla o nível (default `quiet`)

**Eventos:**
- `KATU_RUN`, `KATU_SHUTDOWN`, `KATU_SETUP`
- `KERNEL_TURN`, `TOOL_CALL`, `TOOL_EXEC`
- `MEMORY_RECALL`, `MEMORY_WRITE`
- `POLICY_LOAD`, `POLICY_AUDIT`

### 4.7 Escrita Atômica Endurecida

**Problema:** Symlink plantado pode redirecionar a escrita.

**Solução** (`StdFs`):
1. Cria temporário com `O_EXCL`/`0600`
2. Nome imprevisível (contador atômico + PID)
3. `fsync` no temporário
4. `rename` para o destino

**Garantia:** Não segue um symlink plantado.

### 4.8 Process Group

**Problema:** Netos podem sobreviver ao timeout.

**Solução** (`StdProcess`):
- Filho corre num **process group** próprio (`process_group(0)`)
- No timeout, mata o **grupo** inteiro (`kill(-pgid, SIGKILL)`)
- **Único ponto `unsafe`** do projeto (ADR 0016)

### 4.9 Guard de Loop (Q-12/F7)

**Problema:** Ciclo de leitura sem progresso.

**Solução:**
- Observador efémero (`ActivitySink`)
- CUSUM + e-value *anytime-valid* sobre a assinatura das chamadas
- Corta o turno no 5.º passo com `AgentError::LoopDetected`

### 4.10 Gate de VOI (A3/W8-4)

**Problema:** Repetição de só-leitura já satisfeita.

**Solução:**
- Mede o valor da informação já disponível no turno
- `VOI = 0 < custo` ⇒ **não chamar**
- **Nunca** salta o irreconstruível
- **Opt-in** (`behavior.tool_voi`, default **off** até A/B)

### 4.11 Lote Paralelo (B-01)

**Problema:** Execução sequencial de calls `Shared`.

**Solução:**
- `std::thread::scope` com spawn **eager**
- Teto `MAX_PARALLEL_CALLS = 8` (limite de **custo**)
- Curva USL medida em `bench/e18/pool`

### 4.12 Compactação de Contexto (E09-T07)

**Problema:** Histórico cresce sem limite.

**Solução:**
- Orçamento de contexto (`DEFAULT_CONTEXT_BUDGET`)
- Compactação determinística (prime + digest + sufixo cru)
- **Um único dono** do teto (`DEFAULT_CONTEXT_BUDGET`)

### 4.13 Modo de Planeamento (E20-T11)

**Problema:** Agente escreve fora de `.katu/` em modo de planeamento.

**Solução:**
- Regra `plan-write-only-katu` (DenyWriteOutside)
- Regra `plan-no-shell` (Deny)
- Artefacto `.katu/plan/<UTC>.md`

### 4.14 Challenge-and-Response (§33)

**Problema:** Ações destrutivas ou sensíveis.

**Solução:**
- `ChallengePrompt` com `tool`, `rule`, `scope`
- Humano assina (`reason` + `granted_by`)
- Registo append-only em `overrides.jsonl`

---

## 5. Gaps, Flags e Pendências

### 5.1 Gaps Conhecidos

| Gap | Descrição | Estado |
|-----|-----------|--------|
| `upgrade` | Canal de atualização não configurado | Recusa explicitamente (fail-closed) |
| `behavior.tool_voi` | Gate de VOI | Default **off** até A/B; só atua com `suffix` |
| `behavior.prompt_state` | Secção `estado` no prime | Default **on** (Q-04) |
| `embeddings.command` | Comando para lançar o serviço | Reservado; não lança sozinho |
| `memo knowledge --semantic` | Modo semântico do mapa | Adiado |
| `memo knowledge --communities` | Comunidades | Adiado |
| `memo knowledge --write` | Escrita no mapa | Adiado |
| Executor em background | Turno síncrono | Trabalho futuro |

### 5.2 Flags de Compilação

| Flag | Descrição |
|------|-----------|
| `default` | `memory-in-process` |
| `profile` | Instrumentação transversal (DF9/E19) |
| `memory-in-process` | Adaptador in-process do knudge (E03-T02) |

### 5.3 Pendências de A/B

| Feature | Pendência |
|---------|-----------|
| `behavior.tool_voi` | A/B com modelo que emita *tool calls* nativas |
| `behavior.prompt_state` | A/B com o modelo (Q-04) |
| `behavior.context_selection` | A/B: `suffix` vs `utility` (Q-02b/Q-03) |

### 5.4 Limitações Conhecidas

| Limitação | Descrição |
|-----------|-----------|
| `MAX_PARALLEL_CALLS = 8` | Limite de **custo**, não de paralelismo; medido em máquina com 16 cores |
| `DEFAULT_CONTEXT_BUDGET` | Teto fixo; não adaptativo |
| `StdProcess` | Só unix (process group) |
| `watch_service` | Só Linux (systemd) |
| `KnudgeMemory` | `!Sync`; protegido por `Mutex` |

### 5.5 Dívida Técnica

| Item | Descrição |
|------|-----------|
| `Runtime::remember` | Caminho de escrita reservado (agente/`kd`); exercido pelos testes |
| `Runtime::note` | Construtor do caminho de escrita reservado |
| `agent::turn::voi` | Default **off** até A/B |
| `runtime::context::set_selection` | Só existe para testes e um interruptor futuro |

---

## 6. Testes

### 6.1 Suíte de Testes

| Módulo | Testes |
|--------|--------|
| `ports/fs` | Escrita atômica, symlink plantado |
| `ports/process` | Timeout, process group |
| `memory` | Contrato, pre_edit, pre_write, query |
| `runtime` | Recall, escrita, resume, durability, plan, skills, state |
| `agent` | Loop, tool calls, guard, VOI, steering, verify, context |
| `agent/router` | Resolução de caminhos, despacho, fail-closed |
| `tui` | Handler, usage_line |
| `watch_service` | Subscribe, install, uninstall |
| `config` | Merge, parse, unknown key |
| `cli` | Log level, json, init, git mode |

### 6.2 Benches

| Bench | Descrição |
|-------|-----------|
| `bench/e18/memory` | A/B do caminho `memory.write` + gate (P-03) |
| `bench/e18/pool` | Curva USL do pool de lote (E1) |
| `bench/e18/voi` | A/B do gate de VOI |
| `bench/e18/approval` | Aprovação one-shot |
| `bench/e18/durability` | A/B da durabilidade do log (P-01) |
| `bench/mvk` | Harness de medição do MVK (E05-T06) |

---

## 7. Referências

- **MODULE.md:** [`crates/katu/MODULE.md`](../../../crates/katu/MODULE.md)
- **ADRs:** [`wiki/_ref/adr/`](../../_ref/adr/)
- **Políticas:** [`policy/`](../../../policy/)
- **Benches:** [`bench/`](../../../bench/)
