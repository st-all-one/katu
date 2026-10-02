# `katu-policy` — Motor de Política

**Épico:** E02 · **Crate:** `crates/katu-policy` · **Crate puro** (sem I/O, sem providers)

O **motor de política** do katu: avalia **factos tipados** e devolve um veredicto **determinístico**.
É a tese do projeto — a superfície que impõe a política ao agente.

---

## 1. Visão Geral

O crate `katu-policy` implementa o motor que decide **permitir / negar / exigir aprovação / exigir
humano** a partir de factos tipados e de um `RuleSet` versionado.

**Fronteira:**
- **Não** depende de `katu-core`/`katu-tools`/`katu-providers`/`katu-tui` nem de `knudge-core`
  (verificado por `xtask check-layers`).
- Depende só de `serde`, `thiserror`, `toml`. Dev: `proptest`, `serde_json`.
- `#![forbid(unsafe_code)]`; sem `unwrap`/`expect`/`panic`; sem `HashMap` iterado.
- Sem I/O, sem relógio, sem regex sobre prosa (DF2). A resolução de symlinks é do kernel, **antes**
  do veredicto.

**Vocabulário fechado e versionado:** `RuleScope` e `Enforcement` são a superfície inteira; alargar é
uma decisão de kernel registada, nunca configuração de utilizador. `POLICY_VOCAB_VERSION = 3`.

---

## 2. Arquitetura

### 2.1 Composição

```
┌─────────────────────────────────────────────────────────────────┐
│                     katu-policy (motor puro)                    │
├─────────────────────────────────────────────────────────────────┤
│  facts   │  rule   │  decision   │  error                       │
│  (factos)│ (dados) │ (veredicto) │                              │
├─────────────────────────────────────────────────────────────────┤
│  engine (aplicação: âmbito → casamento → veredicto)             │
│  evaluate (seleção do veredicto de maior rank)                  │
├─────────────────────────────────────────────────────────────────┤
│  paths │ argv │ glob │ approval │ audit                         │
├─────────────────────────────────────────────────────────────────┤
│  confidence (Trials/Wilson/FDR/calibração)                      │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
                  (chamador: kernel / borda)
```

### 2.2 Módulos

| Módulo | Responsabilidade |
|--------|------------------|
| `lib.rs` | Reexportações públicas + `POLICY_VOCAB_VERSION` |
| `facts.rs` | `Timestamp`, `Phase`, `SearchMode`, `ToolArgs`, `ToolUse`, `Capability`, `BudgetState`, `Facts` |
| `facts/tool_name.rs` | `ToolName` (15 variantes), `is_file_change`, `parse` |
| `rule.rs` | `Rule`, `RuleScope`, `Enforcement`, `Severity`, `RuleCategory`, `Waiver`, `RuleExamples`, `RuleSet` |
| `decision.rs` | `Decision`, `Evidence`, `Reason`, `ApprovalRequest`, `ControlId` |
| `engine/mod.rs` | `impl Rule` — âmbito, casamento, veredicto; portas de capacidade |
| `engine/names.rs` | Nomes estáveis de tools/fases |
| `evaluate.rs` | `evaluate` puro + seleção do veredicto de maior `rank` |
| `paths.rs` | `ResolvedPath`, `ResolvedArgv` + normalização lexical |
| `argv.rs` | `inspect`, `ArgvInspection`, `ProgramKind` |
| `glob.rs` | `matches_glob` (glob único do projeto) |
| `approval.rs` | `capability_for`/`capability_for_request` |
| `audit.rs` | Auditoria de regras |
| `confidence.rs` | `Trials`, `Threshold`, `Confidence`, `Verdict`, `verdict`, `calibrate` |
| `confidence/fdr.rs` | `benjamini_hochberg`, `control_fdr`, p-value exacto |
| `error.rs` | `PolicyError` |

---

## 3. Módulos em Detalhe

### 3.1 Factos (`src/facts.rs`)

Nada de `String` crua para caminhos ou `argv`.

**`Timestamp`** — milissegundos desde a época, transparente em serde.

**`Phase`** (7, caminho único): `Task`, `KnowledgeConsulted`, `Planned`, `Implemented`, `Verified`,
`Persisted`, `Closed` (ordenação por `Ord` para `RequireBefore`).

**`SearchMode`:** `Grep`, `Find`, `Ls`.

**`ToolArgs`** (9 variantes, tag `kind`): `Read { path }`, `Write { path, bytes }`,
`Edit { path }`, `Move { from, to }`, `Trash { path }`, `Exec { argv, cwd }`,
`Search { root, query, mode }`, `Plan`, `Other`.

**`ToolUse`:** `name`, `args`, `resolved_paths: Vec<ResolvedPath>`, `argv: Option<ResolvedArgv>`,
`cwd`.

**`Capability`** (9 variantes, tag `kind`): `ReadPath`, `WritePath`, `DeletePath`, `Workspace`,
`Exec { program }`, `Command { tool }`, `Net { host }`, `SpawnPty`, `McpSession { id }`.

**`BudgetState`:** `writes: u32`, `bytes: u64`, `execs: u32`.

**`Facts`:** `now_millis`, `phase`, `tool`, `capabilities`, `budget`, `completed: BTreeSet<ToolName>`.

**`ToolName`** (15 variantes): `Read`, `Write`, `Edit`, `Move`, `Trash`, `Exec`, `Search`,
`MemoryRecall`, `MemoryWrite`, `MemoryOutcome`, `MemoryClose`, `Plan`, `Compact`, `Model`,
`Thinking`. Helpers:
- `is_file_change()` → `Write|Edit|Move|Trash` (entra no *diff* da verificação E09-T03).
- `parse(name)` → vocabulário fechado; nome desconhecido = `None` (o adaptador recusa, fail-closed).

### 3.2 Regra como dado (`src/rule.rs`)

**`RuleScope`** (4): `Path { root }`, `Command { tool }`, `Phase { phase }`, `Budget { cap }`.

**`BudgetCap`** (3): `Writes(u32)`, `Bytes(u64)`, `Execs(u32)`.

**`Enforcement`** (10 variantes — a superfície inteira):

| Variante | Semântica |
|----------|-----------|
| `DenyCommand { tool }` | Nega uma tool (porta falha-fechado) |
| `DenyWrite { root }` | Nega escrita sob `root` |
| `DenyWriteOutside { root }` | Nega escrita **fora** de `root` (**muro duro**, modo plano) |
| `DenyRead { root }` | Nega leitura sob `root` |
| `DenySensitiveRead { globs }` | Nega leitura sensível (glob por componente) |
| `DenyDelete { root }` | Nega envio para lixo sob `root` |
| `RequireBefore { phase }` | Exige fase antes |
| `RequireAfter { tool }` | Exige tool antes |
| `Budget { cap }` | Aplica teto de orçamento |
| `Advisory` | Só informa |

**`Severity`:** `Critical` (nega) / `Warn` (pede aprovação) — DF11.
**`RuleCategory`:** `Enforced` / `Advisory` / `Perception`.

**`Rule`** (`deny_unknown_fields`): `id`, `statement`, `scope`, `enforcement`, `severity`, `category`,
`remedy: Option<String>` (o que **passaria**, Q-08), `expires_at`, `waiver`, `examples`.
- `is_active(now)` — `false` se expirada ou suspensa por `waiver` (com expiração).

**`RuleSet`:** `vocab: u32`, `rules: Vec<Rule>`.
- `from_toml`/`to_toml`; `check_vocab` recusa versão desconhecida (fail-closed).

### 3.3 Veredicto (`src/decision.rs`)

**`Decision`** (`tag = "outcome"`):

| Variante | Campos |
|----------|--------|
| `Allow` | — |
| `Deny` | `reason`, `rule_id`, `evidence` |
| `RequireApproval` | `request: ApprovalRequest` |
| `NeedsHuman` | `reason`, `missing_control: ControlId` |

**`rank()`** — `Allow (1) < RequireApproval (2) < Deny (3) < NeedsHuman (4)` (maior vence).

**`Evidence`** (estruturada, nunca prosa): `file_line: Option<String>`, `fact`, `argument`,
`rule_id`, `remedy: Option<String>` (Q-08). `with_remedy` copia da regra.

### 3.4 Aplicação (`src/engine/mod.rs`)

`impl Rule` — o coração do casamento.

**`scope_matches`:** `Path` (algum `resolved_paths` sob `root`), `Command` (nome igual),
`Phase` (fase igual), `Budget` (`true`).

**`applies`** — por variante de `Enforcement`:
- `DenyWrite` → `write_hit` (só tools de escrita) filtrado por `write_capability_covers`.
- `DenyWriteOutside` → `write_outside_hit` (algum caminho **fora** de `root`), **sem** capacidade.
- `DenyRead` → `read_hit` filtrado por `read_capability_covers`.
- `DenySensitiveRead` → `sensitive_read_hit` (componente casa glob) filtrado por
  `explicit_read_capability_covers`.
- `DenyDelete` → `delete_hit` filtrado por `delete_capability_covers`.
- `DenyCommand` → nome igual **e** `!command_capability`.
- `RequireBefore` → `facts.phase < phase`.
- `RequireAfter` → `!facts.completed.contains(tool)`.
- `Budget` → `budget_exceeded`.
- `Advisory` → `None`.

**`verdict`:** `Budget` → sempre `NeedsHuman { missing_control: "budget" }`; `Advisory` → `Allow`;
as restantes passam por `severity_verdict`: `Warn` → `RequireApproval`, senão → `Deny`.

**Portas de capacidade:**
- `read_capability_covers`: `ReadPath` **ou** `Workspace` (implícito).
- `explicit_read_capability_covers`: **só** `ReadPath` (o workspace não destranca sensíveis).
- `write_capability_covers`: `WritePath` **ou** `Workspace`.
- `delete_capability_covers`: **só** `DeletePath`.
- `command_capability`: `Command` nominal; `Exec { program }` (só `argv` verificável e não
  destrutivo **nem de rede** cujo programa casa exatamente); `Net { host }` (só `argv` de rede,
  `*` = qualquer).
- `exec_program_covers` / `net_capability_covers` delegam em `crate::inspect`.

**Helpers de leitura/escrita:** `is_write_tool` = `Write|Edit|Move`; `is_read_tool` =
`Read|Search` (a busca traz a raiz resolvida, para `DenyRead`/`DenySensitiveRead` avaliarem o que
será varrido).

### 3.5 Motor `evaluate` (`src/evaluate.rs`)

```rust
pub fn evaluate(facts: &Facts, rules: &RuleSet) -> Result<Decision, PolicyError>
```

- `rules.check_vocab()?` (fail-closed).
- Ignora regras `RuleCategory::Advisory`.
- Para cada regra: `applies` → `verdict`; guarda o de maior `rank`.
- Ordem de custo crescente (§32): âmbito → exemplo negativo → orçamento.
- Determinístico (teste `evaluation_is_deterministic`).

### 3.6 Caminhos (`src/paths.rs`)

**`ResolvedPath`** — absoluto e lexicalmente normal (sem `.`/`..`); a resolução de symlinks é do
kernel (via porta `Fs`) **antes** do veredicto. `from_canonical` é a única porta (recusa não
absoluto). `is_under(root)` respeita fronteiras (`/workshop` não está sob `/work`; `/` cobre tudo).
Serde via `try_from`/`into` `String`.

**`ResolvedArgv`** — lista não vazia; `program()` (primeiro), `as_slice()`.

**`normalize`** — resolve `.`/`..` lexicalmente, sem tocar no SO (`Component::RootDir|CurDir|Prefix`
ignorados, `ParentDir` faz `pop`).

Proptest: nenhum `..` sobrevive.

### 3.7 Inspetor de `argv` (`src/argv.rs`)

Classificação **determinística**, **sem regex**, por **igualdade de strings**.

**Listas (constantes):**
- `INTERPRETERS`: `sh`, `bash`, `zsh`, `dash`, `ksh`, `fish`, `csh`, `tcsh`, `ash`, `python`,
  `python2`, `python3`, `pypy`, `perl`, `ruby`, `node`, `deno`, `php`, `lua`, `awk`, `gawk`,
  `mawk`, `sed`.
- `INLINE_FLAGS`: `-c`, `-e`, `--eval`, `-E`.
- `NESTED_FLAGS`: `-exec`, `-execdir`, `-ok`, `-okdir`, `--exec`.
- `DESTRUCTIVE_FLAGS`: `-delete`, `-remove`, `--delete`.
- `NETWORK_PROGRAMS`: `curl`, `wget`, `ssh`, `scp`, `sftp`, `rsync`, `nc`, `netcat`, `ncat`,
  `telnet`, `ping`, `ftp`, `socat`.

**`ProgramKind`:** `Command`, `Interpreter`, `InlineInterpreter`.

**`ArgvInspection`:** `program` (basename), `kind`, `destructive`, `nested`, `network`,
`host: Option<String>`.
- `is_interpreter()` — não `Command`.
- `is_opaque()` — `InlineInterpreter` ou tem `nested` (não verificável).
- `is_plain()` — não opaco, não destrutivo e **não de rede** (a rede exige `Capability::Net`).

**`inspect`** — basename do programa; varre as flags; `host_of` extrai autoridade de URL ou
`user@host` (só para programas de rede).

Nota deliberada: um programa citado (`r''m`) **não** é normalizado — evita semântica de shell.

### 3.8 Glob (`src/glob.rs`)

`matches_glob(pattern, path)` — `*` = qualquer sequência, `?` = um caractere. Recursivo, sem regex,
sem backtracking sobre entradas não confiáveis. É a **única** semântica de glob do projeto (o
`katu-core::plan` reexporta-a).

### 3.9 Aprovação (`src/approval.rs`)

`capability_for(use_, rules, rule_id) -> Option<Capability>` — deriva a capacidade **mínima** que
destranca a regra:
- `DenyRead` → `ReadPath` (primeiro caminho resolvido).
- `DenySensitiveRead` → `ReadPath` do primeiro caminho sensível.
- `DenyWrite` → `WritePath`.
- `DenyDelete` → `DeletePath`.
- `DenyCommand { Exec }` → `Net { host }` (rede, host explícito) ou `Exec { program }` (argv plain);
  opaco/destrutivo/sem host → `None` (fail-closed). Outras tools → `Command { tool }`.
- Restantes (orçamento/fase/dependência) → `None` (não sobreponíveis).

`capability_for_request` é a conveniência sobre `ApprovalRequest`. **O agente não assina**: um
humano responde ao *challenge* e o kernel concede a capacidade.

### 3.10 Auditoria (`src/audit.rs`)

`audit(rules, now_millis) -> AuditReport` — puro e determinístico.

**`AuditIssue`** (4):
- `EnforcedWithoutNegativeExample { id }` — `Enforced` sem exemplo negativo (§51.7).
- `AdvisoryEnforcementMarkedEnforced { id }` — `enforcement = Advisory` mas categoria `Enforced`.
- `DuplicateRuleId { id }`.
- `EmptyStatement { id }`.

**`AuditReport`:** `enforced: Vec<RuleSummary>`, `advisory: Vec<RuleSummary>`, `issues`;
`is_clean()`. `RuleSummary` traz `category`, `activity` (`Active`/`Inactive`) e `examples`
(`HasNegative`/`MissingNegative`).

### 3.11 Confiança medida (`src/confidence.rs`)

`Enforced` é uma categoria **declarada**; aqui é **provada**.

**`Trials`** — posterior `Beta(1+s, 1+n−s)` (prior uniforme evita `p = 1` com amostra pequena):
- `observe_honored`/`observe_violation`/`merge`/`from_parts` (`successes > trials` normalizado).
- `mean_milli()` — `(1+s)/(2+n)`.
- `wilson_lower_milli(z_milli)` — limite inferior do intervalo de Wilson (unilateral); `n = 0` → `0`.

**`Threshold`** (dado, nunca derivado dos dados — DF8):
- `DEFAULT`: `theta_milli = 900` (θ = 0,90), `n_min = 5`, `z_milli = 1_645` (≈95 % unilateral),
  `q_milli = 50` (FDR 5 %).

**`Confidence`:** `Enforced` (provada), `Advisory` (demovida), `Unmeasured` (sem observações).

**`Verdict`:** `id`, `confidence`, `trials`, `successes`, `lower_milli`, `contradiction`, `reason`
(com a evidência).
- `verdict(id, trials, threshold)` — `Unmeasured` sem ensaios; `Enforced` se
  `n ≥ n_min && LB ≥ θ`; senão `Advisory`. `contradiction` = `Advisory` com `n ≥ n_min` e ≥1 falha.
- Com registo perfeito, o LB cruza 0,90 a **n = 25** (LB `902 ≥ 900`); com FDR, a **n = 29**.

**Calibração (C3/W8-2):**
- `CALIBRATION_BINS = 10`; `CalibrationBin` (`lower_milli`, `upper_milli`, `trials`,
  `predicted_milli`, `observed_milli`).
- `Calibration`: `trials`, `brier_milli`, `ece_milli`, `bins`.
- `calibrate(verdicts)` — ECE, Brier e diagrama de fiabilidade (base `inferred`). Mede o
  **conservadorismo** do limite *in-sample*, não o acerto do modelo.

### 3.12 FDR (`src/confidence/fdr.rs`)

Controlo de múltiplas comparações sobre a família de regras `Enforced`.

- `P_MICRO_SCALE = 1_000_000` (micro); `P_FLOOR_MICRO = 1` (piso; abaixo diz "muito pequeno").
- `Trials::promotion_p_micro(theta_milli)` — p-value **exacto unilateral** (`H0: p ≥ θ`), cauda
  binomial superior `P[X ≥ s | n, θ]`; `n = 0` → `p = 1`.
- `binomial_upper_tail` — termos recursivos de `k = n` para baixo (`P[X = n] = θⁿ`), estável para
  `n` grande.
- `MultipleTests`: `q_milli`, `tested`, `rejected`, `rejections: Vec<bool>`.
- `benjamini_hochberg(p_micro, q_milli)` — rejeita `H0_(i)` para `i ≤ k`, o maior índice com
  `p_(i) ≤ q·i/m`; ordenação desempata pelo índice (determinístico).
- `control_fdr(&mut [Verdict], threshold)` — aplica BH com `q = threshold.q_milli`; **fail-closed**:
  só pode **tirar** promoções, nunca dar. A promoção que não sobrevive é demovida a `Advisory` com
  `p`/`q` no motivo.

### 3.13 Erros (`src/error.rs`)

`PolicyError`: `NonAbsolutePath`, `InvalidArgv`, `UnknownVocab { found, expected }`, `Toml`.

---

## 4. Abordagens de Engenharia

### 4.1 Factos Tipados, Nunca `String` Crua

**Princípio:** Caminhos e `argv` são `ResolvedPath`/`ResolvedArgv`; a decisão nunca olha para prosa.

**Benefício:** Elimina a classe de falha de `starts_with("secrets")` (o golden `/work/secrets2`
prova-o); a normalização lexical é o invariante.

### 4.2 Vocabulário Fechado e Versionado

**Princípio:** `RuleScope` e `Enforcement` são enums; `POLICY_VOCAB_VERSION` versiona o conjunto.

**Benefício:** Alargar é decisão de kernel registada, nunca configuração de utilizador. Um
`RuleSet` com versão desconhecida é recusado (fail-closed).

### 4.3 Motor Puro

**Princípio:** `evaluate` não faz I/O, não lê relógio, não usa regex sobre texto.

**Benefício:** Determinístico e testável; a resolução de symlinks é feita pelo kernel antes.

### 4.4 Fail-Closed em Toda a Linha

**Exemplos:**
- Vocabulário desconhecido → erro.
- `Capability::Exec` não destranca `argv` opaco/destrutivo.
- Rede sem host reconhecível → só `*` destranca.
- Sem capacidade derivável → **não** há override.
- `control_fdr` só demove.

### 4.5 Portas de Capacidade Falha-Fechado

**Princípio:** `DenyWrite`/`DenyRead`/`DenyDelete`/`DenyCommand` disparam salvo capacidade
correspondente.

**Benefício:** O normal exige grant; o sensível exige grant explícito.

### 4.6 Muro Duro do Modo Plano

**Princípio:** `DenyWriteOutside { root }` nega escrita **fora** de `root` mesmo com capacidade.

**Benefício:** `/plan` escreve só sob `.katu/` (ADR 0022); nenhuma capacidade o contorna.

### 4.7 Workspace Implícito vs. `ReadPath` Explícito

**Princípio:** `Workspace` destranca o normal dentro da raiz, mas **nunca** `DenySensitiveRead`.

**Benefício:** `.ssh`/`.env` exigem aprovação humana explícita (E07-T05).

### 4.8 Inspetor de `argv` Determinístico

**Princípio:** Classificação por igualdade de strings (interpretadores, flags inline/aninhadas/
destrutivas, programas de rede).

**Benefício:** Sem regex, sem semântica de shell; `bash -c`, `find -delete`, `find -exec`, `r''m`
continuam negados.

### 4.9 Rede Exige `Net`

**Princípio:** Programas de rede não são destrancados por `Exec`; só `Capability::Net { host }`.

**Benefício:** Exfiltração exige um grant explícito, com host casado.

### 4.10 Veredicto por `rank`

**Princípio:** `evaluate` guarda o veredicto de maior `rank` (Allow < RequireApproval < Deny <
NeedsHuman).

**Benefício:** Uma regra `Critical` sobrepõe sempre uma `Warn`; a ordem das regras não muda o
resultado.

### 4.11 Severidade Decide Negar vs. Pedir

**Princípio:** `Critical` → `Deny`; `Warn` → `RequireApproval`; `Budget` → sempre `NeedsHuman`.

**Benefício:** A mesma regra pode ser muro ou pedido, sem duplicar vocabulário (DF11).

### 4.12 Evidência Estruturada com Remédio

**Princípio:** `Evidence` é tipada; `remedy` (Q-08) diz o que **passaria**.

**Benefício:** O modelo recebe como corrigir, não uma negação opaca.

### 4.13 Confiança Medida

**Princípio:** `Enforced` é provada por `n ≥ n_min` **e** `LB ≥ θ`; senão demove com evidência.

**Benefício:** Uma regra declarada sem suporte estatístico é exposta, não aceita cegamente.

### 4.14 FDR Fail-Closed

**Princípio:** BH sobre a família de promoções; só pode tirar.

**Benefício:** Evita ~`α·m` promoções falsas; 40 regras a 25 honras cada → o BH não promove nenhuma
(`p = 0,0718 > 0,05`).

### 4.15 Calibração In-Sample

**Princípio:** ECE/Brier/diagrama medem o conservadorismo do LB face à frequência empírica.

**Benefício:** Torna auditável a distância entre o número publicado e o log; base `inferred`.

### 4.16 Glob Único

**Princípio:** `matches_glob` é a única semântica de glob do projeto.

**Benefício:** Sem duas semânticas divergentes; reexportado pelo `katu-core::plan`.

### 4.17 Auditoria de Regras

**Princípio:** `Enforced` exige exemplo negativo; duplicados e enunciados vazios são sinalizados.

**Benefício:** Uma regra sem exemplo negativo é um erro detetável, não uma promessa.

---

## 5. Gaps, Limitações e Pendências

### 5.1 Limitações Declaradas

| Limitação | Descrição |
|-----------|-----------|
| Sem I/O | O crate não lê o log nem o FS; o chamador extrai as observações e resolve symlinks |
| Pré-condições semânticas | Dedup ≥ 0.92, âncora e "uma afirmação" chegam como **capacidade** do adaptador de memória (E03); o motor não as calcula |
| Rede sem host | `argv` de rede sem host reconhecível → só `Capability::Net { host: "*" }` destranca |
| Programa citado | `r''m` não é normalizado de propósito (evita semântica de shell) — mas não é um falso negativo: `DenyCommand` dispara |

### 5.2 Gaps / Dívida

| Item | Descrição |
|------|-----------|
| Calibração in-sample | Mede o conservadorismo do limite no próprio log, **não** validação fora da amostra (declarado) |
| `Perception` | Existe no vocabulário mas `evaluate` só ignora `RuleCategory::Advisory`; uma regra `Perception` com `enforcement` não-`Advisory` ainda é aplicada pelo motor (o `audit` classifica-a como advisory, o motor não) |
| `SpawnPty`/`McpSession` | Capacidades no vocabulário sem consumidor neste crate (reservadas) |

### 5.3 Flags / Constantes de Operação

| Constante | Valor | Uso |
|-----------|-------|-----|
| `POLICY_VOCAB_VERSION` | 3 | Versão do vocabulário (fail-closed) |
| `Threshold::DEFAULT.theta_milli` | 900 | θ = 0,90 |
| `Threshold::DEFAULT.n_min` | 5 | Ensaios mínimos |
| `Threshold::DEFAULT.z_milli` | 1 645 | ≈95 % unilateral |
| `Threshold::DEFAULT.q_milli` | 50 | FDR 5 % |
| `CALIBRATION_BINS` | 10 | Baldes do diagrama de fiabilidade |
| `P_MICRO_SCALE` | 1 000 000 | Escala dos p-valores |
| `P_FLOOR_MICRO` | 1 | Piso do p-value (1 µ) |
| `Enforcement` | 10 variantes | Superfície inteira do vocabulário |

---

## 6. Testes

### 6.1 Testes Unitários (por módulo)

| Módulo | Testes |
|--------|--------|
| `facts/tool_name` | `is_file_change` só nas tools de disco |
| `paths` | Recusa relativo, resolve `..`, `is_under` com fronteiras, argv vazio, proptest |
| `argv` | Plain, basename, `find -delete`/`-exec`, metacharacters, programa citado, rede+host |
| `engine` | Âmbito/casamento/veredicto |
| `evaluate` | Deny/Allow por raiz, `DenyWriteOutside`, `RequireAfter`, `Budget`→`NeedsHuman`, `Warn`→aprovação, determinismo, vocabulário desconhecido |
| `rule` | TOML/vocabulário |
| `glob` | Prefixo/sufixo, componentes sensíveis |
| `approval` | `ReadPath` para sensível/fora, `Budget`/fase não sobreponíveis, `exec` opaco não sobreponível |
| `audit` | `Enforced` sem exemplo, duplicado/vazio, `Advisory` marcado `Enforced` |
| `confidence` | `Trials`, Wilson, `verdict`, calibração |
| `confidence/fdr` | p-value exacto, BH, `control_fdr` |

### 6.2 Testes de Integração

| Ficheiro | Cobertura |
|----------|-----------|
| `tests/golden.rs` | Matriz de veredictos (`tests/golden/verdicts.tsv`); existe para **falhar** se alguém trocar o motor puro por regex |
| `tests/memory_policy.rs` | Carrega `policy/memory.toml` real; as 5 regras `Enforced` decidem como o protocolo exige |
| `tests/containment.rs` | Carrega `policy/containment.toml` real; sensíveis deny-by-default + fora do workspace sob aprovação |

### 6.3 Testes Notáveis

- **Golden** — dois grupos: **fronteira de caminho** (`/work/secrets2`, `secrets/../public/x`) e
  **`exec` opaco** (`bash -c`, `&&`, `find -delete`, `docker run`, `python -c`); o texto do `argv`
  **não** muda o veredicto.
- **Containment** — a distinção-chave: `Capability::Workspace` (implícita) destranca o normal dentro
  da raiz, mas **nunca** um caminho sensível.

---

## 7. Referências

- **MODULE.md:** [`crates/katu-policy/MODULE.md`](../../crates/katu-policy/MODULE.md)
- **Artefactos de política:** [`policy/`](../../policy/) (`memory.toml`, `containment.toml`,
  `coverage-ledger.json`)
- **Bench de confiança:** [`bench/e18/confidence`](../../bench/e18/confidence/PROTOCOL.md)
- **ADRs:** 0022 (vocabulário v3 / modo plano)
