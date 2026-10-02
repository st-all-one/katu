# Dossiê de Pendências — o que falta implementar no katu

Levantamento do que **não está implementado** no projeto, verificado no **código real**, nos
documentos de crate ([`katu.md`](katu.md) e os `CRATE.md`) e nos planos de referência
([`OPTIMIZATION_PLAN`](../_ref/plan/OPTIMIZATION_PLAN.md), [`IMPLEMENTATION_PLAN`](../_ref/plan/IMPLEMENTATION_PLAN.md),
[`SURFACE_IMPLEMENTATION`](../_ref/plan/SURFACE_IMPLEMENTATION.md)) e ADRs.

> **Estado geral:** o **MVK está assinado** (ADR 0001) e o plano de otimização **Q-01…Q-21 ·
> P-01…P-04 · S-01…S-05** está praticamente fechado (W7–W10 concluídos). O que resta não é o
> caminho crítico do MVP — é **trabalho futuro deferido**, **itens rejeitados com número**,
> **decisões opt-in à espera de A/B**, **lacunas concretas de código/gate** e **formalismos do
> Anexo A ainda por implementar**.

---

## 0. Como ler

**Legenda de estado**

| Marca | Significado |
|-------|-------------|
| ⛔ **Deferido** | Fora do escopo do MVP; só com decisão registada + consumidor atual |
| 🚫 **Rejeitado** | Medido e recusado **com o número** — não implementar |
| ⏳ **Opt-in / A/B** | Implementado mas desligado; a adoção por omissão exige A/B com o modelo |
| 🔴 **Lacuna** | Falta real de código/infra, verificada no repositório |
| ⚪ **Formalismo** | Proposta do Anexo A do plano de otimização, ainda não implementada |

**Eixos:** **Q** qualidade da execução do modelo · **P** performance bruta · **S** simplificação.

---

## 1. Sumário executivo

| Categoria | Quantos | Onde |
|-----------|---------|------|
| Epics futuros deferidos | 3 | §2 |
| Lacunas concretas de código/gate | ~18 | §3 |
| Decisões opt-in à espera de A/B | 5 | §4 |
| Formalismo do Anexo A não implementado | ~10 | §5 |
| Itens rejeitados com o número | ~10 | §6 |
| Infra/entrega (release, packaging) | 2 | §7 |

**As três lacunas mais acionáveis hoje:**

1. 🔴 **`--no-default-features` não compila** — quebra `make memory-swap` e o modo `--no-memory` do
   `install.sh`.
2. 🔴 **Não há releases nem packaging do `katu`** — o modo release do `install.sh` é especulativo.
3. ✅ **Cinco features implementadas** — ativadas por omissão por decisão do dono (§4),
   configuráveis para reverter.

### ✅ Resolvidos (2026-10-02)

- **§3.1** — `--no-default-features` agora compila; `make memory-swap` e `--no-memory` funcionam.
- **§3.2** — Workflow de release criado (`.github/workflows/release.yml`) + `scripts/package.sh`.
- **§3.3** — `katu upgrade` com canal via GitHub Releases (com comparação numérica de versões).
- **§3.4** — `policy/prices.toml` populado com preços de referência para modelos locais e remotos.
- **§3.8** — Caminho de escrita de memória ligado ao agente via `Runtime::remember`.
- **§3.9** — Adaptadores de portas verificados/ligados; `allow` históricos removidos e `StdRng` morto eliminado.
- **§3.11** — `evaluate` passa a ignorar `Perception` (consistente com o audit; não decide).
- **§4** — Duas flags opt-in ficaram **ligadas por omissão** (`prompt_state`, `durability`);
  `tool_voi` e `context_selection` voltaram aos defaults seguros (`false`/`suffix`) por causarem
  leituras repetidas/loop, e `structured_output` ficou `off` (parte a terminação do turno com o
  provider `llama`; ver §4).

---

## 2. Epics futuros (deferidos, fora do escopo do MVP)

### 2.1 ⛔ E08 — Adaptador MCP (2.ª implementação da porta `Memory`)

- **Documento:** [`09-adaptador-knudge.md`](../_ref/plan/09-adaptador-knudge.md) · **Estado:**
  `deferred`, nenhuma tarefa ativa.
- **Porquê deferido:** G4 (o knudge é memória **in-process**, não sidecar) e G7 (superfície CLI+TUI
  e só). Um 2.º backend sem consumidor é anti-YAGNI.
- **O que já existe:** a porta `Memory` com tipos do katu, o adaptador isolado, a suíte de
  conformidade (`assert_contract`) e o gate `check-memory-swap` — tudo pronto para um 2.º backend.
- **O que falta:** cliente MCP stdio (E08-T01), mapeamento das 4 tools de hint, framing, timeout,
  reconexão; paridade de backends (E08-T02).
- **Condição para retomar:** decisão `DF` registada + necessidade real da porta MCP.

### 2.2 ⛔ E11 — Plugin host e ABI (WASM)

- **Documento:** [`12-plugins-e-abi.md`](../_ref/plan/12-plugins-e-abi.md) · **Estado:** deferido.
- **Porquê deferido:** o seam de controlo é a **política** (dado), não plugins. ABI carimbada +
  manifests + pontos de extensão é maquinaria pesada e só faz sentido com produto estável.
- **O que já existe:** o **modelo** de capacidades (`Capability`, `Decision`) no plano principal.
- **O que falta:** o **runtime** de plugins — host WASM, `ExtensionCapability`, manifests, hooks
  (que nunca são regras: se precisa de I/O ou estado, é hook, não `evaluate`).
- **Condição para retomar:** decisão registada + consumidor atual; nunca dentro de `evaluate`.

### 2.3 ⛔ E17 — Jail de SO real (bwrap/Landlock/seccomp)

- **Documento:** [`18-jail-futuro.md`](../_ref/plan/18-jail-futuro.md) · **Estado:** deferido pós-MVP.
- **Porquê deferido:** no MVP o katu é **global de facto** e a contenção é **soft** (barreira
  declarada, **não** fronteira de segurança). A jail converte-a em fronteira de kernel, mas custa
  plataforma (matriz kernels/distros, escape tests).
- **O que já existe:** o gancho `SandboxEnforcement` (`Full`/`Partial` sempre falham com
  `ContainmentError::Unavailable`, fail-closed), `SandboxMode`, `WorkspaceRoot`, `Capability::Workspace`.
- **O que falta:** `SandboxBackend` (Bwrap/Landlock/Seccomp/Rlimits), `PathGrant` da camada
  `CliTui`, config global `[jail]`, suite de escapes.
- **Decisão registada:** OA13 — reusar `bwrap` como binário externo validado.

---

## 3. Lacunas concretas de código/gate (verificadas)

### 3.1 ✅ `--no-default-features` não compila

- **Ficheiro:** `crates/katu/src/cli/memo.rs:11` faz `use crate::defaults;` **incondicionalmente**,
  mas `mod defaults;` só existe com a feature `memory-in-process` (`crates/katu/src/main.rs:17`).
- **Efeito:** `cargo check -p katu --no-default-features` falha (`unresolved import crate::defaults`);
  o alvo `make memory-swap` (que corre esse comando) e o `--no-memory` do `install.sh` ficam quebrados.
- **Correção:** gatear `use crate::defaults` (e os ramos que o usam) com `#[cfg(feature =
  "memory-in-process")]`, ou tornar `defaults` incondicional.
- **Estado:** ✅ Resolvido (2026-10-02) — `use crate::defaults` gateado com `#[cfg(feature = "memory-in-process")]`.

### 3.2 ✅ Sem releases nem packaging do `katu`

- **Verificado:** `.github/workflows/` tem só `ci.yml`, `pr-fast.yml`, `pr-msrv.yml` — **nenhum**
  workflow de release. `scripts/` não tem empacotador (só `bench-pos`, `check_file_length`,
  `katu-idle`, `llama`, `profile`).
- **Efeito:** o modo release do `install.sh` (asset `katu-<versão>-<target>.tar.gz` + `sha256sums.txt`)
  não tem artefacto publicado; só o `--from-source` funciona hoje.
- **Falta:** workflow de release por tag + `scripts/package.sh` + `sha256sums.txt`.
- **Estado:** ✅ Resolvido (2026-10-02) — Workflow `.github/workflows/release.yml` criado + `scripts/package.sh` funcional.

### 3.3 ✅ `katu upgrade` sem canal

- **Ficheiro:** `crates/katu/src/cli/upgrade.rs` · **Estado:** recusa explicitamente (fail-closed),
  canal de atualização não configurado. Falta o mecanismo de canal (como o `install.sh` resolve a
  versão).
- **Estado:** ✅ Resolvido (2026-10-02) — Canal de atualização implementado via GitHub Releases API.

### 3.4 ✅ Custos `unpriced`

- **Ficheiro:** `bench/published.toml` — 3 métricas com `basis = "unpriced"` e valor `0`:
  `mvk.cross_tool.gain_ratio`, `provider.request_compression.saved_ratio`,
  `w81.grammar.invalid_json_turns_avoided_ratio`.
- **Causa:** `PriceTable` vazia por omissão (`katu-providers`) → tudo `unpriced` até a borda a
  preencher. Falta popular os preços reais por modelo.
- **Estado:** ✅ Resolvido (2026-10-02) — `policy/prices.toml` populado com preços de referência para modelos locais (Qwen, Llama, Granite) e remotos (OpenAI, Anthropic, Google, DeepSeek, Mistral, Qwen API).

### 3.5 ✅/⏳ Providers sem validação ao vivo (e2e)

- **Ficheiro:** `katu-providers` · **Estado:** `responses`/`messages`/`google` implementados;
  `google` no built-in opencode é explicitamente `Unsupported` (só via declarativo).
- **Verificado (2026-10-02):** já existem smoke tests ao vivo em `crates/katu-providers/tests/live.rs`
  para os três dialetos (`opencode_responses_smoke`/`opencode_messages_smoke`/`opencode_google_smoke`),
  com auto-*skip* sem chave; cobertos por `make test:integration`. Continuam por fazer (deliberados ou
  dependentes de rede): WebSocket/HTTP2 (`Unsupported`), gzip do pedido desligado (endpoints rejeitam)
  e jitter no retry (fora de escopo por desenho — `retry.rs`: "um cliente, não uma manada").

### 3.6 🔴 `outline` heurístico (sem tree-sitter)

- **Ficheiro:** `katu-tools/src/outline.rs` · **Estado:** heurístico, **não** é parser (não ignora
  `{}` em strings/comentários). Tree-sitter fica **gated por medição** (DF12).

### 3.7 🔴 `plan` e `memory` (tools) parciais

- **`PlanTool`:** o plano vem do **artefacto** carregado no arranque, **não** dos argumentos do
  modelo (`katu-tools`).
- **`memory`:** a tool só **pede**; o gate (`pre_write`/dedup/âncora) vive no kernel.

### 3.8 ✅ Caminho de escrita de memória reservado

- **Ficheiro:** `crates/katu/src/runtime/memory.rs` — `Runtime::remember` e `Runtime::note` estão
  `#[allow(dead_code)]`: "caminho de escrita reservado (agente/kd); exercido pelos testes do
  runtime". A superfície `memo` **só consulta** (E20-T06). Falta ligar a escrita pelo agente.
- **Estado:** ✅ Resolvido (2026-10-02) — Agente agora usa `Runtime::remember` para escrever na memória (recall prévio + gate de E05).

### 3.9 ✅ Adaptadores de portas `dead_code`

- **Ficheiro:** `crates/katu/src/ports/mod.rs` — `StdProcess`/`StdFs` com `allow(unused_imports)`
  ("adaptadores ligados ao kernel em E04/E10"). `crates/katu/src/main.rs:35` idem. Falta ligar os
  adaptadores ao loop (parte é histórica; verificar o que ainda está solto).
- **Estado:** ✅ Resolvido (2026-10-02) — verificado: `StdProcess`/`StdFs`/`StdEnv` já são
  consumidos em produção (`agent`, `tui`, `watch_service`, CLI); os `allow` históricos foram
  removidos. O único item solto (`StdRng`, reservado para jitter) foi eliminado — o retry declara
  jitter como fora de escopo (`retry.rs`: "um cliente, não uma manada").

### 3.10 ⏳ TUI: executor em background e benchmarks

- **Executor em background:** o turno é **síncrono** (trabalho futuro).
- **`xtask bench-render` / `gate:render`:** ✅ implementados (`xtask/src/render_bench.rs`) e
  versionados (`bench/render/budget.toml`); o CI corre `gate:render`. A verificação de zero
  alocações no hot path (E18-T10) foi **medida e travada** (`crates/katu/tests/render_alloc.rs`): o
  render aloca ~1,1k vezes por quadro; refazê-lo com dados emprestados fica para decisão própria.
- **`Controls`:** só o utilizador muda; a borda aplica ao **próximo** turno.
- **`pending_menu`:** menu a abrir quando a borda publicar as capacidades do novo modelo.

### 3.11 ✅ Capacidades reservadas sem consumidor

- **`SpawnPty` / `McpSession`** (`katu-policy`): no vocabulário, sem consumidor.
- **`Perception`:** categoria existe mas `evaluate` só ignora `RuleCategory::Advisory` — uma regra
  `Perception` com enforcement não-`Advisory` ainda é aplicada pelo motor (o `audit` classifica-a
  como advisory, o motor não).
- **Estado:** ✅ Parcial (2026-10-02) — `evaluate` corrigido para ignorar `Perception` (coerente com
  o audit); `SpawnPty`/`McpSession` continuam reservados (deferidos com o jail/MCP).

### 3.12 🔴 Plataforma limitada

- **`StdProcess`** (process group) e **`watch_service`** (systemd timer): só **unix/Linux**.
- **Contenção soft:** sem jail de SO (ver §2.3).

### 3.13 🔴 Embeddings e modos de conhecimento adiados

- **`embeddings.command`:** comando para lançar o serviço de embeddings — reservado, não lança sozinho.
- **`memo knowledge --semantic` / `--communities` / `--write`:** modos adiados.
- **Dreno de embeddings:** fail-closed sem provedor.

### 3.14 🔴 Rotação de logs

- **Deliberadamente fora:** o projeto **nunca apaga automaticamente**. Os segmentos de auditoria são
  imutáveis; falta uma política de retenção/rotação explícita.

### 3.15 🔴 Outros itens de dívida registados no código

| Item | Crate | Descrição |
|------|-------|-----------|
| `tool_arguments` | providers | Reconstrói argumentos do `ToolUse` **resolvido** (melhor esforço); o log não guarda os argumentos crus |
| Inferência in-process (L2) | providers | Fora de escopo; atrás de feature (comentário em `llama.rs`) |
| `search` determinístico | tools | Sem `HashMap` (custo de performance em troca de determinismo) |
| `walk` | tools | `MAX_FILES = 4096`, `MAX_DEPTH = 16`; padrões de `.gitignore` simples |
| `DEFAULT_CONTEXT_BUDGET` | core | Teto fixo; não adaptativo |
| `MAX_PARALLEL_CALLS = 8` | katu | Limite de **custo**, não de paralelismo; medido em máquina com 16 cores |
| `runtime::context::set_selection` | katu | Só existe para testes e um interruptor futuro |
| `agent::turn::voi` | katu | Opt-in (`behavior.tool_voi = true`) e só atua com a seleção `suffix`; com `utility` a unidade lida pode ser descartada |

---

## 4. Decisões opt-in — ativação por omissão ⏳→✅ (uma revertida)

Tudo isto estava **implementado mas desligado**; a adoção por omissão exigia A/B com um modelo que
emitisse *tool calls* nativas. **Decisão do dono (2026-10-02): ativadas por omissão**; `tool_voi`
e `context_selection` foram **revertidas** para o default seguro depois de causarem leituras
repetidas/loop (ver nota), tal como `structured_output`. Continuam configuráveis (projeto > global)
para reverter sem alterar código.

| Feature | Default | Reversão (config) | O que mede | Fonte |
|---------|---------|-------------------|------------|-------|
| `behavior.tool_voi` | **off** | `true` (só com `suffix`) | tool calls evitadas; 0 irreconstruíveis saltados | A3/W8-4 |
| `behavior.prompt_state` | **on** | `false` | secção `estado` no prime (Q-04) | Q-04 |
| `behavior.context_selection` | `suffix` | `utility` | `suffix` vs `utility` (Q-02b/Q-03) | Q-02b |
| `structured_output` | **off** ⏳ | `true` | turnos com JSON inválido evitados; hoje `unpriced` | W8-1 |
| `behavior.durability` | `turn` | `event` | *group commit* do log (P-01) | P-01 |

**Nota de método:** a ativação **não** foi precedida de A/B — é uma decisão explícita do dono (o preço
continua a ser o da §4 original: reverter pela config e pelo bump de `PRIME_VERSION`). Os números
publicados permanecem `unpriced` onde não há medição real; nada foi inventado.

**`structured_output` foi revertido para `off` (2026-10-02).** O `response_format`
`json_schema` com `oneOf` de tool calls (ADR 0025) **não tem variante de resposta final**: ligado, o
modelo local fica obrigado a emitir sempre uma tool call, o loop nunca vê o passo sem chamadas e o
turno corre até ao teto de passos sem devolver texto. É a causa de "executa uma ação e para" na TUI
com o provider `llama`. Só reativar depois de (a) o schema ter uma variante terminal de texto /
"sem tool" e (b) A/B garantir que não parte a terminação.

**`tool_voi` e `context_selection` revertidos (2026-10-02).** O gate de VOI saltava uma leitura
alegando “já presente no contexto”, mas com `context_selection = utility` a unidade lida pode ser
descartada — o modelo repetia a leitura, o guard de loop cortava o turno (“erro e reinicia”). Os
defaults voltam a `false`/`suffix` e o gate passa a **exigir** `suffix` (com `utility` não atua).

**Leitura de ficheiro inexistente:** o `read` devolvia `Unavailable { control: "read" }` (parecia
falha transitória) em vez de um relatório `read.missing` legível; o modelo repetia a leitura até o
guard cortar. Corrigido no adaptador, e o aviso de truncagem passou a indicar a continuação
(`read <id>@<linha>`) em vez de só dizer “truncado”.

**Chave renomeada:** `provider.structured_output` → `structured_output` (2026-10-02). A chave antiga
colidia com o escalar `provider` (TOML não admite `provider = "..."` e `[provider]` na mesma tabela),
pelo que `set_key` descartava a escrita em silêncio. Há agora o guard `keys_have_no_scalar_table_collisions`.

---

## 5. Formalismos do Anexo A ainda não implementados ⚪

Do [`OPTIMIZATION_PLAN` §Anexo A](../_ref/plan/OPTIMIZATION_PLAN.md). **Feitos:** B1, A1, A2, A3,
C1, C2 (rejeitado), C3, C5, C7, D1, D2, D3, E1, B-01…B-07. **Em falta:**

| # | Formalismo | Onde | Estado |
|---|-----------|------|--------|
| A5 | Cross-encoder reranking | hits de memória antes do prime | ⚪ não feito (o knudge tem `reranking_ann.md`) |
| A6 | Late interaction (ColBERT) | knudge (fronteira) | ⚪ só consumo; não reimplementar |
| B2 | Speculative decoding | provider `llama` | ⚪ não feito |
| B3 | Continuous batching + KV/prefix cache | servidor local + Q-14 | ⚪ não feito |
| B4 | Planeamento como POMDP / MCTS-lite com VOI | `agent/plan.rs` + plano | ⚪ não feito (F8 rejeitado) |
| B5 | Bandit contextual (Thompson por hash) | escolha de tool/provider | ⚪ não feito |
| B6 | Constrained tool choice / logit bias | request por dialeto | ⚪ não feito |
| C4 | Bayes hierárquico (partial pooling) | F6 | ⚪ não feito |
| C6 | Sketches de streaming (t-digest/HLL) | percentis do provider, novidade | ⚪ não feito |
| E2 | Tail-at-scale hedging + HdrHistogram | F4 | ⚪ não feito (Q-13 não fechou) |

**Nota:** A5/A6 são **consumo** do knudge; nenhum reimplementa o motor de retrieval.

---

## 6. Itens rejeitados com o número 🚫 (não implementar)

| Item | Número que o rejeitou | Fonte |
|------|----------------------|-------|
| **A4** retrieval semântico de tools (top-k) | ganho máximo seguro **14,0 %** < 20 % | Q-18 |
| **C2** conformal prediction | cobertura **913–947 ‰** < 950 ‰ prometidas; **0** ensaios de calibração (`n_cal ≥ 19`) | Anexo A C2 |
| **E18-T08** PERT/CPM sobre o DAG do plano | **0 arestas** → caminho crítico é a lista; CPM devolveria a ordem atual | §W10 |
| **E18-T09** PPR semeado pelo working set | **0 arestas** no ranking plano de `Memory::search`; o RRF já funde | §W10 |
| **A2 / DPP** (determinantal) | as trocas ganham **utilidade**, não redundância; o MMR já resolve a diversidade | Anexo A A2 |
| **B-08** modo `batch` declarativo | mantém o **loop nativo**; ganho de ~80 % do PTC é **alegado, não medido** | ADR 0026 |
| **B-09** runtime programável real (quickjs/wasmtime) | contradiz binário único / G7 / zero-dep | Anexo B |
| **B-10** background jobs | nova capacidade (G3); hoje um timeout falha fechado | Anexo B |
| **B-05** par de eventos de sub-chamada | só faz sentido **dentro** de B-08 (rejeitado); não antecipar | Anexo B |
| **E18-T05** partilha estrutural do `State` | cópia é `memcpy` de alguns KiB dentro dos 321 µs — abaixo do ruído | §W10 |
| **Cache de condensação do prompt** | `fs.write` com `sync_all` **25,8 ms** vs. µs da passagem linear | Q-19 |

**Não absorver (registado no Anexo B):** Node/TypeScript, REPL persistente, paralelismo sem
classificação, `both` por default.

---

## 7. Infra, entrega e CI

| Item | Estado | Falta |
|------|--------|-------|
| Workflow de release | ✅ presente | `.github/workflows/release.yml` por tag |
| Packaging | ✅ presente | `scripts/package.sh` (tar.gz/zip + `sha256sums.txt`) |
| `install.sh` modo release | ✅ real | depende dos dois acima (agora publicados) |
| CI | ✅ presente | `ci.yml`, `pr-fast.yml`, `pr-msrv.yml` |
| `make memory-swap` | ✅ verde | ver §3.1 |
| `katu upgrade` | ✅ canal | canal via GitHub Releases (§3.3) |

---

## 8. Riscos e kill criteria (do roadmap)

Fonte: [`17-roadmap-riscos.md`](../_ref/plan/17-roadmap-riscos.md). Revisão obrigatória no fim de
cada fase.

| # | Risco | Prob. | Impacto | Mitigação |
|---|-------|-------|---------|-----------|
| R1 | GDK alpha quebra API | alta | médio | pinar versão; isolar atrás de trait próprio |
| R2 | Escopo "controle" vira produto inteiro | alta | **alto** | teto de superfície (`surface.toml`); kill criteria |
| R3 | Jail de SO cross-platform é caro | média | médio | adiada (E17); soft declarada |
| R4 | Performance regride por persistência | média | médio | benchmarks + gate de CI |
| R5 | Reimplementar commodity | média | alto | regra "commodity = dependência" |

**Gates por fase (assinatura obrigatória):** Fase 0 `make check` verde · Fase 1 regras `Enforced` +
porta substituível · Fase 2 **MVK ✅ assinado** · Fase 3 negação pelo executor · Fase 4 nenhum número
sem base · Fase 5 terminal panic-safe · Fase 6 firewall LLM-free intacta.

---

## 9. Prioridade sugerida

1. ✅ **Corrigir `--no-default-features`** (§3.1) — `make memory-swap` e o `--no-memory` funcionam.
2. ✅ **Publicar releases + packaging** (§3.2/§7) — `install.sh` release com artefacto real.
3. ✅ **Popular `PriceTable`** (§3.4) — custos deixam de ser `unpriced` por omissão.
4. ✅ **Ligar o caminho de escrita de memória** (`Runtime::remember`) ao agente (§3.8).
5. ⏳ **Correr os A/B pendentes** (§4) quando houver modelo que emita tool calls nativas.
6. ✅ **Validação e2e dos dialetos** `responses`/`messages`/`google` (§3.5) — smoke tests existentes
   (auto-*skip*); faltam só WebSocket/HTTP2 e gzip (deliberados/rede).
7. ⚪ **Formalismos do Anexo A** (§5) por tier de prioridade (impacto × mensurabilidade × encaixe).

---

## 10. Referências

- **Docs de crate:** [`katu.md`](katu.md) · [`katu-core`](katu-core/CRATE.md) ·
  [`katu-policy`](katu-policy/CRATE.md) · [`katu-tools`](katu-tools/CRATE.md) ·
  [`katu-providers`](katu-providers/CRATE.md) · [`katu-tui`](katu-tui/CRATE.md) ·
  [`katu`](katu/CRATE.md)
- **Planos:** [`OPTIMIZATION_PLAN`](../_ref/plan/OPTIMIZATION_PLAN.md) ·
  [`IMPLEMENTATION_PLAN`](../_ref/plan/IMPLEMENTATION_PLAN.md) ·
  [`SURFACE_IMPLEMENTATION`](../_ref/plan/SURFACE_IMPLEMENTATION.md) ·
  [`19-otimizacao-profunda`](../_ref/plan/19-otimizacao-profunda.md) ·
  [`17-roadmap-riscos`](../_ref/plan/17-roadmap-riscos.md)
- **Epics futuros:** [`09-adaptador-knudge`](../_ref/plan/09-adaptador-knudge.md) ·
  [`12-plugins-e-abi`](../_ref/plan/12-plugins-e-abi.md) ·
  [`18-jail-futuro`](../_ref/plan/18-jail-futuro.md)
- **Gates versionados:** [`surface.toml`](../../surface.toml) · [`coverage.toml`](../../coverage.toml) ·
  [`layers.toml`](../../layers.toml) · [`bench/published.toml`](../../bench/published.toml)
- **ADRs:** [`wiki/_ref/adr/`](../_ref/adr/README.md) (26 no total)
