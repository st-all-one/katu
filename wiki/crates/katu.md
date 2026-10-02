# katu — Resumo Global do Projeto

Síntese baseada nos documentos de crate em [`wiki/crates/`](.): [`katu-core`](katu-core/CRATE.md),
[`katu-policy`](katu-policy/CRATE.md), [`katu-tools`](katu-tools/CRATE.md),
[`katu-providers`](katu-providers/CRATE.md), [`katu-tui`](katu-tui/CRATE.md) e
[`katu`](katu/CRATE.md).

---

## 1. O que é

**katu** é um **agente de codificação** de terminal cuja tese não é o modelo, mas a **política**: a
superfície fechada que impõe regras ao agente. O kernel trata o agente como uma **máquina de estados
pura**:

> O estado é um **valor**, a transição é uma **função**, o log é a **fonte da verdade**.

O sistema é construído sobre três invariantes duros:

- **`Model-visible ⟺ logged`** — o que o modelo vê é exatamente o que está no log append-only.
- **Fail-closed** — na dúvida (vocabulário, transição, aprovação, orçamento, `argv`), **recusa**;
  nenhuma recusa altera o estado nem corta a evidência (§42).
- **O agente não assina** — aprovações, overrides e re-enables exigem **humano** (MAC assinado).

O projeto é **dado versionado** (DF3): regras e políticas vivem em TOML no repositório
(`policy/*.toml`, `.katu/katu.toml`), auditáveis e alteráveis sem recompilar.

---

## 2. Arquitetura Global

```
┌──────────────────────────────────────────────────────────────────────┐
│  Borda (binário `katu`)                                              │
│  CLI (clap) │ Runtime │ Agent (loop) │ TUI │ Watch service           │
├──────────────────────────────────────────────────────────────────────┤
│  Adaptadores (portas)                                                │
│  SystemClock │ StdRng │ StdFs │ StdEnv │ StdProcess │ KnudgeMemory   │
├──────────────────────────────────────────────────────────────────────┤
│  Núcleo (crates puros)                                               │
│  katu-core (kernel) │ katu-policy (motor) │ katu-tools (ferramentas)│
│  katu-providers (modelos) │ katu-tui (UI pura)                       │
└──────────────────────────────────────────────────────────────────────┘
```

**Firewalls de dependência** (verificados por `xtask check-layers`):

- Os crates puros **nunca** dependem do binário; o binário depende de todos.
- **LLM-free:** `katu-core`, `katu-policy` e `katu-tools` **não** dependem de `katu-providers`.
  A porta `Provider` vive no núcleo; o adaptador é que fala com o modelo.
- `katu-tui` não depende de `katu-tools`/`katu-providers`/`knudge`: a UI só vê estado puro e emite
  `Command`s que a borda executa.
- `katu-policy` não depende de `katu-core`; é o motor mais isolado (sem I/O, sem relógio).

**Portas** (traits em `katu-core`): `Clock`, `Rng`, `Fs`, `Env`, `Process`, `Memory`, `Provider`.
Todo o código impuro do SO fica confinado aos adaptadores do binário. A escrita atómica endurecida
(`O_EXCL`/`0600`, `fsync`, `rename`) e o *process group* (matar o grupo no timeout) são o único ponto
`unsafe` do projeto (ADR 0016).

---

## 3. Os Seis Crates

| Crate | Papel | Épico | Puro? |
|-------|-------|-------|-------|
| [`katu-core`](katu-core/CRATE.md) | Kernel: máquina de estados, log, contexto, estatística, auditoria, TOON | E04 | Sim (kernel) |
| [`katu-policy`](katu-policy/CRATE.md) | Motor de política: factos → veredicto determinístico | E02 | Sim (sem I/O) |
| [`katu-tools`](katu-tools/CRATE.md) | Superfície fechada de tools + contenção soft | E06/E07 | Sim |
| [`katu-providers`](katu-providers/CRATE.md) | Adaptadores de modelo (o único que fala com LLMs) | E12 | Cliente |
| [`katu-tui`](katu-tui/CRATE.md) | Interface de terminal (estado central + render puro) | E10 | UI pura |
| [`katu`](katu/CRATE.md) | Binário: CLI, composição, loop de turnos, TUI | E01/E04 | Não (borda) |

### 3.1 `katu-core` — o kernel

A máquina de estados do agente. `step(State, Event) -> Result<State, Refusal>` é puro; o log
`session.v1.jsonl` é append-only e reconstruível. Contém:

- **Kernel:** estado, eventos tipados, transição, projeções, log (durabilidade *Event*/*Turn*),
  orçamento, custo (5 camadas), *loop guard* (CUSUM + e-value), confiança, snapshot (hash canónico).
- **Contexto:** orçamento de tokens (`BYTES_PER_TOKEN_MILLI = 3 631`), seleção submodular de
  unidades, compactação determinística, prime versionado.
- **Estatística:** percentis *nearest-rank*, MAD, IC robusto/normal, *bootstrap* determinístico.
- **Auditoria:** índice invertido + Bloom + codec binário (delta/LEB128).
- **TOON:** formato colunar D39 ao modelo (SWAR, emissão numa só alocação).
- **Evidência tipada** (DF5), **verificação** determinística, **plano tipado** com merge por menor
  privilégio, **contenção soft**, **taint** (*spotlighting*), **feedback**, **memória** (porta),
  **provider** (porta), **diagnóstico** (DF9), **erro**.

### 3.2 `katu-policy` — o motor de política

A superfície que impõe a política. Recebe **factos tipados** e um `RuleSet` versionado e devolve
`Allow` / `RequireApproval` / `Deny` / `NeedsHuman`. Vocabulário **fechado** e versionado
(`POLICY_VOCAB_VERSION = 3`): `RuleScope` (Path/Command/Phase/Budget) e `Enforcement` (10 variantes)
são a superfície inteira. A confiança é **medida, não declarada**: `Enforced` só com `n ≥ n_min` e
limite de Wilson `≥ θ`; o FDR (Benjamini-Hochberg) é *fail-closed* (só **tira** promoções).

### 3.3 `katu-tools` — ferramentas e contenção soft

**Registry fechado de 11 tools** (`read`, `write`, `edit`, `move`, `trash`, `bash`, `grep`, `find`,
`ls`, `plan`, `memory`) com schema declarativo e **linter anti-prompt-poisoning**. Os executores são
determinísticos (`BTreeMap`, sem `HashMap` iterado). A contenção é **soft** — declarada honestamente
como *não* fronteira de segurança (a jail real é E17/futura).

### 3.4 `katu-providers` — os modelos

O único crate que fala com LLMs. Caminho **built-in first-party** (`opencode go/zen` + `llama.cpp`)
e caminho **declarativo** (JSON estilo `goose`). Dialetos de wire: `ChatCompletions`, `Responses`,
`Messages`, `Google`. Transporte `ureq` 3 (rustls), SSE incremental *zero-alloc*, retry com
classificação de erros. WebSocket/HTTP2 explicitamente `Unsupported` (ADR 0012/0013).

### 3.5 `katu-tui` — a interface

UX de codificação sobre `ratatui` + `crossterm`, com **estado central** (`App`), **render puro** e
**keymap puro** (`map_key`). Challenge-and-response para aprovações, mini-menus de modelo/pensamento,
transcrição durável, lixeira. Sem servidor, sem rede (G7).

### 3.6 `katu` — o binário

O ponto de entrada e a **única** camada impura. CLI (`prime`/`upgrade`/`config`/`memo`/`run`/`tui`),
composição das portas, adaptador in-process do `knudge-core`, loop de turnos, TUI e worker de
auto-drain (systemd). Recusa arrancar sem memória saudável (fail-closed).

---

## 4. O Fluxo de um Turno

```
Utilizador
   │
   ▼
katu (borda) ── monta ProviderRequest via Session::context
   │
   ▼
katu-providers ── stream normalizado (ProviderEvent)
   │
   ▼
katu (agent/turn) ── para cada tool call, na ordem §42:
   │
   ├─ 1. LOGAR     Session::tool_call  (o pedido entra no log ANTES do efeito)
   ├─ 2. POLÍTICA  katu-policy::evaluate(facts)  → Allow / RequireApproval / Deny
   └─ 3. EFEITO    Tool::execute (katu-tools)     → ToolResult (logado)
   │
   ▼
katu-core::step(State, Event) ── transição pura; recusa NÃO altera o estado
   │
   ▼
log append-only (fonte da verdade) ── Model-visible ⟺ logged
```

Guardas no caminho: **loop guard** (CUSUM + e-value *anytime-valid*, corta no 5.º passo repetido),
**gate de VOI** (não repete só-leitura; opt-in), **lote paralelo** (`MAX_PARALLEL_CALLS = 8`, limite
de custo), **cost governor** (5 camadas, do específico ao global) e **verificação** determinística
(escopo + feedback + cobertura).

---

## 5. Princípios Transversais (DFs)

| Princípio | Onde se materializa |
|-----------|---------------------|
| **Estado como valor / transição como função** | `katu-core::step`; sem singletons |
| **Log é a fonte da verdade** | `session.v1.jsonl`; `Model-visible ⟺ logged` |
| **Fail-closed** | Vocabulário, transição, aprovação, orçamento, `argv` |
| **O agente não assina** | `ApprovalGranted` (MAC), `Override`, `Reenable` |
| **Política é dado, não código** (DF3) | `policy/*.toml`, `.katu/katu.toml` |
| **Determinismo** | `BTreeMap`, percentis *nearest-rank*, SplitMix64 semeado, desempate por índice |
| **Evidência viaja com o número** (DF5) | `Metric` + `EvidenceBasis` + artefacto |
| **Custo zero por defeito** (DF9) | `instrument` compilado fora; no-op sem feature |
| **Um só sítio para cada facto** | `hash`, `glob`, `toon::schema`, rácio de tokens |
| **Firewall LLM-free** | Núcleo não depende de `katu-providers` |
| **Honestidade** | Contenção soft declarada; conformal rejeitado com o número |

---

## 6. A Matemática do Projeto

O katu decide e resume com estatística explícita, não com heurísticas escondidas:

| Técnica | Uso |
|---------|-----|
| **FNV-1a 64** | Hash canónico, assinatura de chamada, Bloom, content-id |
| **IDF + submodular** (`ln(1+n/df)`) | Seleção de contexto com garantia `1 − 1/e` (Nemhauser) |
| **MMR + RRF + Jaccard** | Diversidade e fusão de rankings na seleção |
| **Divergência JS** | Gatilho de compactação (`τ_JS`) |
| **CUSUM + e-value (Ville)** | Deteção de loop *anytime-valid* (parada opcional) |
| **Wilson LB / Beta-Bernoulli** | Confiança medida por regra (política) |
| **Binomial upper tail + BH** | Promoção a `Enforced` e controlo de FDR |
| **Percentis / MAD / bootstrap SplitMix64** | Resumos determinísticos e IC |
| **Bloom (10 bits/termo, k=4)** | Filtro de auditoria sem falsos negativos |
| **SWAR + LEB128** | Emissão TOON rápida e codec binário delta |
| **Rácio de tokens** (`3 631` bytes/token) | Orçamento de contexto e gate de prompt |

Rejeições documentadas com o número: **conformal** (cobertura colapsa com resíduos correlacionados) e
**cache de condensação de prompt** (passagem linear não paga um `fsync`).

---

## 7. Superfícies e Configuração

- **CLI** (`katu`): verbos exclusivos sem inferência — `prime`, `upgrade`, `config`, `memo`, `run`,
  `tui`. `--params` (XOR com flags), `--batch` JSONL, `--json`, `--log-level`.
- **TUI**: multi-turno, streaming efémero, steering entre passos, transcrição, lixeira.
- **Config**: conjunto **fechado** de chaves, precedência projeto > global; caminhos
  `~/.config/local/katu/katu.toml` e `<root>/.katu/katu.toml`.
- **Layout `.katu/`** (idempotente): `audit/`, `knowledge/`, `guardrails/`, `trash/`, `log/`,
  `plan/`, `katu.toml`. `audit/`, `trash/` e `log/` ficam **sempre** fora do git.

---

## 8. Estado, Limitações e Pendências Globais

**Fase:** MVK (Fase 0/2). Medições em `bench/e18/`.

**Limitações declaradas (transversais):**

- **Contenção soft** — sem jail de SO no MVP (E17 futura); não é fronteira de segurança.
- **Taint estrutural** — separa dado de instrução; **não** impede o modelo de ser influenciado.
- **Sem WebSocket/HTTP2** — `ureq` 3 é HTTP/1.1 (ADR 0012/0013).
- **Plataforma** — `StdProcess` e `watch_service` só unix/Linux.
- **Calibração in-sample** — mede o conservadorismo do limite, não validação fora da amostra.

**Pendências de A/B (default off até medição com o modelo):**

| Feature | Estado |
|---------|--------|
| `behavior.tool_voi` | Gate de VOI — default **off** |
| `behavior.prompt_state` | Secção `estado` no prime — default **off** |
| `behavior.context_selection` | `suffix` vs `utility` — default `suffix` |

**Dívida registada:** `upgrade` sem canal configurado (recusa), validação e2e em falta para
`responses`/`messages`/`google`, `PriceTable` vazia (`unpriced`), `outline` heurístico (tree-sitter
gated por medição), capacidades `SpawnPty`/`McpSession` reservadas sem consumidor.

---

## 9. Como Navegar

| Quero saber… | Ver |
|--------------|-----|
| A máquina de estados, log, contexto, estatística | [`katu-core/CRATE.md`](katu-core/CRATE.md) |
| Como uma decisão é tomada | [`katu-policy/CRATE.md`](katu-policy/CRATE.md) |
| As ferramentas e a contenção | [`katu-tools/CRATE.md`](katu-tools/CRATE.md) |
| Como fala com os modelos | [`katu-providers/CRATE.md`](katu-providers/CRATE.md) |
| A interface de terminal | [`katu-tui/CRATE.md`](katu-tui/CRATE.md) |
| O binário, CLI e composição | [`katu/CRATE.md`](katu/CRATE.md) |
| Regras do agente e referência | [`../_ref/`](../_ref/README.md) |

Cada documento de crate segue a mesma estrutura: Visão Geral · Arquitetura · Módulos em Detalhe ·
Abordagens de Engenharia · Gaps/Flags/Pendências · Testes · Referências.
