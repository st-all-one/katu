# katu — Brainstorm de decisões

> Documento vivo de decisões e trade-offs do `katu`. Não é uma especificação fechada: é o registro do raciocínio que leva às escolhas. Baseia-se nos dossiês [`../pi-rs/`](../pi-rs/) e [`../goose-rs/`](../goose-rs/).

---

## 0. Objetivo do produto (a tese)

Construir um **agente de codificação mínimo em Rust**, cujo diferencial é:

1. **Controle total em runtime** — regras rígidas aplicadas durante a execução.
2. **Bloqueio em tempo real** — uma ação proibida é impedida *antes* de produzir efeito.
3. **Plugins** — extensível, mas com poder limitado e explícito.
4. **Performance crítica** — startup e latência por turno mínimos.
5. **Usabilidade crítica** — UX desenhada para o fluxo de codificação.

Fora de escopo (por decisão): desktop Electron, P2P, voz, inferência local, gateways de plataforma, amplitude de dezenas de providers.

**Frase-guia:** *um kernel pequeno onde a política de runtime é o núcleo, não um acessório.*

> **Evolução da tese (§11):** o recorte maduro não é "mais um agente de codificação mínimo", e sim **um runtime onde a disciplina de memória (knudge) é invariante fiscalizada do loop**. Ver capítulos 10–13.

---

## 1. Diagnóstico: por que o goose não é a resposta direta

O goose entrega muito do **mecanismo**, mas não o **posicionamento**:

| Requisito do katu | Goose tem? | Onde (referência) |
|---|---|---|
| Bloqueio em runtime | Parcial | Hooks `PreToolUse` → `Deny`; `GooseMode` + inspectors |
| Regras rígidas | Parcial | Permissões + hooks (mas **fail-open** por padrão) |
| Plugins | Sim | `plugin.json` (skills + hooks), formatos importáveis |
| Codificação | Sim | Developer extension, tools, MCP |
| **Mínimo** | **Não** | 278K linhas Rust + 89K TS; desktop, P2P, voz, 48 providers |
| **Performance** | Boa, mas ampla | Rust, porém kernel pesado (SQLite a cada passo; `Agent` de 6k linhas) |
| **UX focada em coding** | Não | Superfície de plataforma, genérica |

**Insight central:** no goose, o controle/fiscalização é uma **camada plugada por cima de um kernel grande**. No katu, o controle **é o kernel**. Essa inversão de prioridade define tudo.

Corolário: adotar o goose inteiro contradiz "mínimo" e coloca o controle — justamente o diferencial — fora do nosso controle.

---

## 2. Separação fundamental: commodity vs. tese

### Commodity (custoso, genérico, **não** diferencia — reaproveitar)
- Adaptadores de provider e wire formats (OpenAI/Anthropic/Google)
- Registro canônico de modelos (pricing, limites, capacidades)
- Compaction/sumarização de contexto
- Plumbing de MCP (client stdio/HTTP, OAuth, notificações)
- Convenções de sessão/persistência

### Tese (onde **nós** ganhamos — construir e possuir)
- **Motor de regras/política com bloqueio duro em runtime + auditoria**
- **Plugin host com modelo de capacidades** (o plugin não pode tudo)
- **Execução isolada de verdade** (sandbox, não apenas "hook nega")
- **Hot path de performance** (startup, tempo por turno)
- **UX/TUI de codificação** desenhada para o fluxo

**Regra de decisão:** o goose pode dominar o commodity; **não** deve ser dono da tese.

---

## 3. Opções consideradas

### Opção A — Forkar o goose inteiro e podar ❌
- **Prós:** herda hooks, permissões, MCP, providers, sessão.
- **Contras:** 278K linhas acopladas; migração de dois loops em curso; `Agent` monolítico; upstream veloz → merges brutais. "Mínimo" morre.
- **Veredito:** descartada. Manutenção te engole.

### Opção B — Depender do GDK e ser dono do kernel ✅ **(recomendada)**
- **Prós:** reaproveita o commodity (providers, formats, canonical, compaction, talvez o agent loop) e mantém sob nosso controle exatamente as camadas da tese.
- **Contras:** GDK é `0.1.0-alpha` (API instável); exige disciplina de versionamento/vendorização.
- **Veredito:** escolhida. É para isso que o GDK existe.

### Opção C — Ser cliente ACP do `goose serve` ⚠️
- **Prós:** prototipagem rapidíssima; quase zero código de motor.
- **Contras:** vira apresentação de terceiro; bloqueio depende do processo goose; latência de IPC; hot path fora do nosso controle.
- **Veredito:** atalho de MVP, **não** produto se controle é a tese.

### Opção D — Do zero ⚠️
- **Prós:** controle e performance máximos.
- **Contras:** reinventa provider/formats/compaction — meses não diferenciais.
- **Veredito:** só para o *kernel* e camadas da tese; não para o commodity.

---

## 4. Decisão principal: híbrido "own kernel, borrow commodity"

Workspace pequeno com fronteira clara:

```
katu-policy      ← TESE: regras, matchers, bloqueio em runtime, audit trail
katu-sandbox     ← TESE: isolamento real (seccomp/landlock/container) para execução
katu-tools       ← codificação tunada: read/write/edit/bash/grep/find/ls/git
katu-plugins     ← host de plugins + modelo de capacidades + adaptador MCP
katu-core        ← kernel mínimo: conversa, sessão, event stream, loop
katu-providers   ← camada fina sobre GDK/declarative (só os providers que importam)
katu-tui / katu  ← crossterm; UX focada em codificação
```

### As três decisões que definem o resultado

1. **O kernel não "plugа" política — ele executa política.**
   Toda tool call passa por `katu-policy` *antes* de existir efeito. Decisão tipada e auditável:
   `Decision::Allow | Decision::Deny { reason, rule_id }`.
   (Contraste: no goose isso é hook opcional sobre o loop.)

2. **Bloqueio em runtime é duplo e obrigatório.**
   - **In-process:** negação semântica, rápida, com contexto da conversa.
   - **No SO:** contenção real (sandbox), porque negação no processo não impede um comando de escapar.
   Isso nos posiciona à frente do goose (hooks + Docker opcional).

3. **Plugins com capacidades, não com poder total.**
   Um plugin declara o que pode (fs read/write, net egress, exec, env). O host concede e revoga.
   (Contraste: hook do goose executa comando shell com poder amplo.)

---

## 5. Reuso pragmático do GDK

| Camada | Ação | Racional |
|---|---|---|
| `goose-provider-types` | **Depender** | Message/Conversation, formats, canonical — commodity volátil |
| `goose-context-management` | **Depender** | Compaction/sumarização madura |
| `goose-agent` (loop) | **Avaliar depender** | Máquina de estados genérica (`Step`/`Operation`/`Inference`/efeitos), depende só de `goose-provider-types` + `rmcp`; implementaríamos `Operation`s próprios (incl. política) |
| session storage / exec | **Vendorizar/reescrever** | Pequeno e crítico ao controle |
| `goose` / `goose-cli` / `goose-providers` inteiros | **Evitar** | Arrasta reqwest/oauth/aws/local-inference; peso e acoplamento |

**Padrão a copiar (não o código):** a *operação com efeitos reentrante* do goose — o pipeline que se reconstrói da conversa persistida e registra "o que já foi feito" (determinismo reconstruível).

**Licença:** goose é **Apache-2.0** — reuso/fork seguros com atribuição/NOTICE. Pi é **MIT**.

---

## 6. Plano em fases

- **Fase 0 — especificar o diferenciador.** Modelo de política (regras, prioridade, negação, auditoria) e modelo de capacidades de plugin. *Isso é o produto.*
- **Fase 1 — kernel mínimo + tools + 1 provider**, com bloqueio in-process. Metas: startup < 50 ms; sem overhead evitável por turno.
- **Fase 2 — sandbox + plugin host + MCP.** Bloqueio duro de verdade.
- **Fase 3 — passada de performance e TUI** focada no fluxo de codificação.
- **Fase 4 — amplitude de providers** (reaproveitando GDK/declarative).

---

## 7. Decisões em aberto (a resolver)

1. **Modelo de política:** regras declarativas (TOML/YAML) + DSL embutida? Quantos "estágios" (pré-tool, pós-tool, pré-escrita, egress)?
2. **Semântica de negação:** nega = erro devolvido ao modelo (recuperável) ou parada dura? Ambos?
3. **Granularidade de capacidades de plugin:** por tool? por path? por domínio de rede? por subprocesso?
4. **Sandbox:** container, ou primitivas de SO (landlock/seccomp/namespaces)? Fallback em macOS/Windows?
5. **Persistência:** SQLite (como goose/Pi) ou JSONL append-only? Impacto no startup e retomada.
6. **Retomabilidade:** reconstruir da conversa persistida (determinismo reconstruível) ou estado em memória + checkpoints?
7. **Loop:** adotar `goose-agent` ou escrever kernel próprio? Depende do quanto a política precisa ser entrelaçada.
8. **TUI:** `crossterm` cru (controle fino) vs. framework (`ratatui`)?
9. **Sessão/retomada e auditoria:** audit trail é a fonte da verdade ou derivado?
10. **Escopo de providers no MVP:** Anthropic + OpenAI + OpenRouter + Ollama local?

---

## 8. Riscos

| Risco | Mitigação |
|---|---|
| GDK alpha quebra API | Pinar versão; vendorizar peças pequenas; isolar atrás de trait próprio |
| Escopo "controle" virar produto inteiro | Congelar Fase 0 antes de codar o kernel |
| Sandbox cross-platform é caro | Começar Linux (landlock/seccomp) + fallback degradado explícito |
| Performance regredir por persistência | Benchmarks desde a Fase 1 (`criterion`); medir hot path |
| Reimplementar commodity sem querer | Regra "commodity = dependência", revisada a cada fase |
| Segurança de plugin (código não confiável) | Capacidades + isolamento de processo; nunca shell irrestrito |

---

## 9. Síntese da decisão

> **Não forkar o goose nem usá-lo como cérebro.** Usá-lo como **fornecedor de commodity** (via GDK) e construir nós mesmos as camadas que são a tese: **motor de política com bloqueio em runtime, sandbox, plugin host com capacidades e UX de codificação.**
>
> O goose é excelente **referência de arquitetura** (copiar o *padrão* de operações/efeitos e o determinismo reconstruível), mas adotá-lo inteiro contradiz "mínimo" e entrega o controle — nosso diferencial — a terceiros.

---

## 10. O knudge — memória durável por projeto

Análise de [`_REF/knudge/`](../_REF/knudge/) (projeto próprio, `st-all-one/knudge`, MIT OR Apache-2.0, v0.5.1, Rust edição 2024, MSRV 1.97, ~56k linhas, 883 testes).

### O que é

Uma **memória durável por projeto para agentes de IA**, binário único (`kd`), sem servidor/DB/daemon, mais o `knudge-mcp` (MCP sobre stdio). A inversão que define tudo: *"o usuário é o LLM"*, *"o custo é contexto, não disco"*, *"o índice é derivado"*. Ciclo: **`ask → write → task → sync`**.

### Como faz

| Pilar | Implementação |
|---|---|
| **A nota é a verdade** | Markdown + frontmatter **TOON** em `.knudge/notas/`; id **endereçado por conteúdo** (`type + U+001F + normalize(statement)`) |
| **Índice derivado** | BM25 + âncoras + grafo + embeddings opcionais, fundidos por **RRF**; `.idx/` reconstruível |
| **Buscar antes de gravar** | Dedup em duas fases: `<0.75` cria, `0.75–0.92` **merge**, `≥0.92` **rejeita** |
| **Âncoras** | `--anchor PATH` liga a nota ao código; busca por arquivo; drift de âncora |
| **Tarefas** | Hierarquia `epic ⊃ {issue ⊃ task \| task}`; fechar exige **evidência** (`--outcome`) |
| **Ciclo de vida** | shelf-life, decay, confiança **derivada** (Beta-Bernoulli), `forget`/`restore` reversíveis |
| **Propor, nunca agir** | `doctor`/`learn`/`compact`/`prune` só **propõem**; nada muda sem aceite |
| **MCP** | 4 tools (`pre_write`, `pre_edit`, `session_end`, `status`); hints são **ponteiros** (`id+statement+score`) |

### Engenharia (relevante para o katu)

Núcleo puro + **portas** (`Clock/Rng/Env/Fs/Git/HookRunner/Logger`); `#![forbid(unsafe_code)]`; sem `unwrap/expect/panic`; arquivos ≤ 300 linhas; determinismo obrigatório; contrato de bytes com golden/proptest/fuzz. **`knudge-core` tem só 4 dependências** (`thiserror`, `indexmap`, `unicode-normalization`, `sha2`).

Duas decisões do próprio knudge importam aqui:
- **D65** — núcleo puro + adaptadores (feito para reuso/embedding).
- **D68 — Embeddability: "MCP primeiro (já é o caso de uso), FFI/WASM depois".**

O `01_gaps-plan-rs.md` §193 registra que a separação core/adapter existe justamente para viabilizar *"embedding (MCP/agentes)"*.

---

## 11. Sinergia katu × knudge: a tese refinada

O knudge depende, hoje, de **disciplina do modelo**: chamar `kd ask` antes de gravar, ancorar, dar evidência. Na prática, **agentes esquecem**. Os gatilhos MCP mitigam, mas continuam sendo **sugestão** — o modelo pode ignorar.

O instinto original do katu — **controle em runtime, regras rígidas, bloqueio em execução** — é exatamente a peça que falta: um runtime que **torne impossível** violar o protocolo de memória. Não "peça ao modelo para buscar antes de gravar", mas **não deixe a escrita passar sem a busca**.

> **Tese refinada:** o **knudge** define o que é boa memória; o **katu** é a metade que garante que o agente a cumpra.

- Defensável: ninguém entrega "agente que não consegue violar seu protocolo de memória".
- Não duplica o goose nem o knudge.
- Melhora os dois: o knudge ganha um **agente de referência**; o katu ganha um diferencial que não é "coding agent".

Sem esse recorte, o kernel é só um clone. Com ele, é um produto com razão de existir.

---

## 12. Porta `Memory` e adaptadores

**Regra:** profundo na arquitetura e na filosofia, **desacoplado no código**.

```
katu-core        kernel: loop, event stream, sessão
katu-policy      TESE: regras rígidas, bloqueio em runtime, auditoria
katu-memory      PORTA Memory (tipos do katu — NÃO knudge_core::*)
   ├── adapter mcp   → knudge-mcp / kd          (default; respeita D68)
   └── adapter core  → knudge-core in-process   (opcional; 4 deps; latência mínima)
katu-tools / katu-providers / katu-tui
```

Quatro regras que mantêm a dependência **substituível** (não crítica):

1. **A porta é do katu, com tipos do katu.** Se `Memory` for definida em termos de `knudge_core::schema::Note`, nunca mais se troca. Com tipos próprios, o knudge vira **detalhe de implementação**.
2. **Dois adaptadores, feature-gated.** `in-process` (linka `knudge-core`) e `mcp` (sidecar). Se o core quebrar, cai para MCP sem reescrever o kernel.
3. **Core puro é síncrono; o katu pode ser async.** `knudge-core` é bloqueante (embedding HTTP inclusive). Chamar via `spawn_blocking` + timeout. Sem isso, um I/O travado **congela o agente**.
4. **`0.x` e release deliberado.** Cargo trata `0.5 → 0.6` como breaking. Publicar, ficar em `0.x`, bump consciente; ou git-dependency durante a co-evolução.

**Enforcement no `katu-policy`** (traduzindo as regras de ouro do knudge em bloqueios duros): negar `write` quando o `pre_write` aponta duplicata `≥0.92`; exigir `--anchor` em nota sobre código; exigir `--outcome` antes de fechar tarefa; uma afirmação por nota. Mão dupla possível: o knudge já tem hooks (`pre-record` bloqueia/muta) e a porta `HookRunner`.

**Não fundir os repositórios:** o knudge vale por ser **agnóstico** (serve Claude, Cursor, Codex, Pi). O katu é **consumidor de referência**, não jaula.

---

## 13. `knudge-core` como crate vs. binário único

Pergunta: abrir `knudge-core` como crate importável, unificar o código e fazer do katu um **binário único** — vale a pena ou vira dependência crítica?

### Três decisões separadas

| | Vale? | Nota |
|---|---|---|
| **(A)** Abrir/publicar `knudge-core` | ✅ quase sempre | Reforça a reusabilidade; não mata o agnosticismo |
| **(B)** Unificar códigos / binário único | ✅ com o desenho da §12 | Latência, distribuição, enforcement, testes |
| **(C)** "Vira dependência crítica?" | **Depende do acoplamento** | Gerenciável pelas 4 regras da §12 |

### Por que `knudge-core` é caso técnico excepcional

Puro e enxuto (4 deps), sem tokio/reqwest/DB/rede, `forbid(unsafe_code)`, sem panic, determinístico, arquitetura de portas, projetado para embedding (D65/D68). É o oposto de um risco de acoplamento.

### Ganhos do binário único

Sem spawn/framing/IPC (latência e startup melhores); um artefato (fim de "knudge-mcp não instalado"); enforcement no mesmo processo; teste de integração trivial (sem fixtures MCP); sessão/lock compartilhados.

### Custos reais

Ciclo de release compartilhado; MSRV 1.97 / edição 2024 como piso; compilação acoplada; burden de semver se publicado; **perda de isolamento de processo** (um bug derruba o agente — mitigado: core proíbe panic e as notas são a fonte da verdade); risco de **vazamento de tipos** do core para a API do katu.

### Publicar vs. fundir repositórios

| | Publicar `knudge-core` | Fundir repositórios |
|---|---|---|
| Efeito | Reuso por terceiros; contrato semver | CI/lints/versão compartilhados |
| Custo | Assumir estabilidade de API | Governança/issues acoplados |
| Necessário p/ binário único? | Não (path/git dep basta) | Não |
| Risco ao valor do knudge | **Aumenta** reuso (bom) | Neutro, se continuar produto próprio |

### Onde o binário único **não** compensa

CLI fino com MCP barato; necessidade de isolamento de falhas; incapacidade de manter a porta; necessidade de release independente.

### Recomendação

1. **Publicar `knudge-core`** (só ele; `cli`/`mcp` seguem `publish = false`), permanecendo em `0.x`.
2. **katu = binário único** linkando `knudge-core` via adaptador `in-process`, atrás da porta `Memory`.
3. **Manter o adaptador MCP** e **feature-gate** os dois.
4. **`spawn_blocking` + timeout** no caminho async.
5. **Knudge standalone** (repos separados); katu é consumidor de referência.

### Teste de sanidade

> Se amanhã decidirmos voltar ao MCP, **quantos arquivos do katu mudam?** Se for "só o adaptador da porta", o desenho está certo. Se for "meio kernel", a dependência ficou crítica.

### Síntese

> **Não é a dependência que é crítica — é o acoplamento.** Embutir `knudge-core` é tecnicamente fácil e traz ganhos reais. Com porta `Memory` de tipos próprios, dois adaptadores feature-gated e chamadas bloqueantes isoladas, o binário único é um **detalhe de empacotamento** de uma dependência **substituível** — não um ponto único de falha.

---

## 14. Outros projetos próprios analisados

Dois repositórios de autoria (`st-all-one`) que informam o katu: [`_REF/open-mtr-rs/`](../_REF/open-mtr-rs/) e [`_REF/my-task-manager-tui/`](../_REF/my-task-manager-tui/).

### `open-mtr-rs` — shell unificado sobre backends heterogêneos

Wrapper multi-sistema para APIs de MTR (SINIR + estaduais). Rust 1.85+, edição 2024, MPL-2.0, core ~10k linhas.

- **Fachada + adapters:** `OpenMtrClient` (~17 métodos universais) → `trait MtrAdapter` (1 por sistema: SINIR, MTR-PE, SIGOR).
- **Modelo universal com lacunas explícitas:** campos que um sistema não oferece ficam `None` — *"sem perda, sem acoplamento"*.
- **`HttpTransport` trait** (`Arc<dyn>`), plugável via `build_with_transport()`: `reqwest` (nativo) ou `globalThis.fetch` (WASM, sem tokio/reqwest). Um adapter de ~30 linhas troca o mundo.
- **Envelope de resposta:** `Result<Resposta<T>, OpenMTRError>` — sucesso e **erros de negócio parciais** em `Ok(Resposta{dados, erros})`; só catástrofe (rede/auth/timeout) é `Err`. RFC 9457 + **UUID v7** por resposta.
- **Newtypes validados** (`Cnpj`, `Cpf`, `DataMtr`), `#[non_exhaustive]`, retry/backoff/rate-limit no cliente genérico, `tracing` com spans `instance/operation/system`.
- **Multi-binding:** napi-rs (Node), UniFFI (Python/Kotlin/Swift), wasm-bindgen (Deno/Web).
- **Testes:** unit + wiremock + **live contra API real** + **comparativo** (`live_compare_systems` mede preenchimento de campos por backend).

### `my-task-manager-tui` ("taskdeck") — TUI com verdade em Markdown

Rust 2021, ratatui 0.30 + crossterm + rusqlite (bundled), ~4,9k linhas.

- **Markdown + frontmatter JSON é a verdade** (um `.md` por tarefa; pasta = diretório) + **SQLite derivado** (`task_meta`, FTS5, folders, integrations, sync_links); `sync_all_task_meta` **reconstrói** o cache.
- **`Vinculo`** — links externos tipados (`OrigemExterna::{GitLab, Redmine, Local, Url}`) com `display_short()` e ícone.
- **Plugin manager event-driven** (`PluginManager`, `ComandoPlugin`/`EventoPlugin` via `mpsc`), estratégia **cache-first → API → store**.
- **`map_key(KeyEvent, AppMode) -> Action` + `apply_action(&mut App, Action)`** — mapeamento puro, por modo, testável.
- **Decks derivados** (Eisenhower/Checklist/Timeline) = views sobre a mesma tarefa.
- **Lixeira (soft delete)** com `trashed_at` e restauração.
- **Panic hook que restaura o terminal** + `color_eyre`.

---

## 15. Padrões e filosofia de assinatura

Padrões que se repetem entre os projetos próprios (e o knudge):

| Padrão | open-mtr | taskdeck | knudge | Leitura para o katu |
|---|---|---|---|---|
| Fachada + adapters sobre N backends | `MtrAdapter` | `PluginManager` | portas/adapters | Generaliza as portas `Memory`/`Provider` |
| Verdade em arquivo, índice derivado | — | `.md` + SQLite | `notas/` + `.idx/` | Confirma a §12 |
| I/O plugável na borda | `HttpTransport` | canais de plugin | portas | `build_with_memory()` ≈ `build_with_transport()` |
| Erro como valor + sucesso parcial | `Resposta{erros}` | `EventoPlugin::Erro` | `warnings[]` | Contrato de tool-result do katu |
| Enums fechados / determinismo | `System`, `StatusMtr` | `Priority`, `AppMode` | enums + TOON | "invalid states unrepresentable" |
| Soft delete / reversível | — | lixeira | `forget`/`restore` | Nunca destruir; propor e reverter |
| Multi-superfície (core + bindings) | napi/uniffi/wasm | — | core/cli/mcp | Kernel como lib + binário |
| Feature-gate do runtime | `native-http` opcional | — | core sem tokio | Habilita binário único e WASM |
| Teste comparativo/conformance | `live_compare_systems` | — | goldens | Conformance entre adapters |

### A filosofia de assinatura (o que os projetos revelam)

1. **A verdade é um artefato legível e versionável; o índice é descartável** (knudge e taskdeck, independentemente).
2. **Núcleo puro, adaptadores finos na borda** (portas, `HttpTransport`, plugins).
3. **Domínio canônico + lacunas explícitas (`None`).** Nunca falsear dado para preencher contrato.
4. **Erro de negócio não é catástrofe** (`erros[]`/`warnings[]`); o fluxo segue.
5. **Reversibilidade** (soft delete, propose-don't-act).
6. **Determinismo** (enums fechados, ordenação canônica).
7. **Multi-superfície e embeddability.**
8. **O repositório é substrato de colaboração com IA** (`AGENTS.md`, `SKILL.md`, `llms.txt`, `conversations/`, docs com aviso de drift).

---

## 16. Integrações concretas e o que não copiar

### Integrações

1. **`HttpTransport` é o template literal da porta `Memory`** — `build_with_memory(adapter)` nos moldes de `build_with_transport()`; valida os adaptadores da §12.
2. **Envelope + UUID v7 vira o contrato de resultado das tools do katu** — erro-recuperável como dado + correlação para auditoria (casa com intent→efeito→settlement do Pi e errors-as-content do goose).
3. **`Vinculo` como cidadão de primeira classe** — o "onde/como conecta" externo (issue/PR/URL), complementando âncoras/arestas do knudge. Candidato a conceito **compartilhado** katu ↔ taskdeck.
4. **taskdeck é um mini-knudge de tarefas** — mesma tese `.md`-verdade + índice derivado. Cuidado com sobreposição: o knudge já tem `task` + evidência — decidir qual é a fonte.
5. **`PluginManager` (comando/evento + cache-first) é o modelo do plugin host** — plugins por eventos tipados, não chamadas diretas.
6. **Testes comparativos entre adapters** (`live_compare_systems`) → medir paridade entre backends de memória e providers.
7. **Panic-safe do terminal** → requisito da TUI do katu.
8. **Feature-gate do transporte** → confirma a estratégia do binário único (§13).
9. **Bindings além do binário** (napi/uniffi/wasm) → caminho para expor kernel e knudge a outras linguagens.
10. **Docs com aviso de drift + guias de IA no repo** → padronizar.

### O que não copiar

| Item | Onde | Por quê |
|---|---|---|
| `Arc<dyn>` + `async_trait` no caminho quente | open-mtr | Despacho dinâmico; preferir enum/generics onde crítico |
| `expect()` permitido com mensagem | open-mtr | Disciplina diferente do knudge (`unwrap_used` **e** `expect_used` = deny) |
| `#![allow(dead_code)]`, `.lock().unwrap()` | taskdeck | Smell; knudge usa `lock_or_recover` |
| Pasta em dois lugares (dir **e** tabela) | taskdeck | Duas fontes da verdade |
| Logs de sessão de IA volumosos no repo | open-mtr | Fora do versionamento ou resumido |
| Superfície de métodos divergente da doc (16/17/21) | open-mtr | Fixar contagem (knudge resolve com catálogo testado) |

### Síntese

> O **katu não é um ponto fora da curva**: é o **elo que faltava** entre knudge (memória), open-mtr (adapters) e taskdeck (UX/eventos) — o runtime que fiscaliza a memória, com o modelo de adapters do open-mtr e a interação do taskdeck.

---

## 17. `sniff-css` — o blueprint filosófico

Análise de [`_REF/sniff-css/`](../_REF/sniff-css/) (projeto próprio `st-all-one/sniff-css`; v0.4.1, edição 2024, MSRV 1.88, **CC0-1.0**, ~25,5k linhas / 62 arquivos / 8 crates). É o projeto onde nasceu o impulso de criar ferramentas — o **progenitor da linhagem**.

### O que é

Captura o **computed style real** de elementos via **Chrome DevTools Protocol raw** (WebSocket, sem puppeteer/frameworks) — e a **árvore de widgets** de apps Flutter via Dart VM Service — emitindo **JSONL determinístico** otimizado para LLMs. Toolset de 4 binários sobre um core: `sniffCSS` (captura), `sniffCSS-diff` (só o que mudou), `sniffCSS-check` (regras offline), `sniffCSS-mcp` (MCP).

### Arquitetura (camadas limpas)

```
sniff-css (CLI) → SniffConfig → Sniffer (sniff-engine) → CdpSession (sniff-cdp) → Chrome
                                                   └─ 1 Runtime.evaluate → JSONL
Flutter: flutter://device → FlutterMachine → FlutterInspector → mesmo ElementSnapshot → JSONL
```

| Crate | LOC | Papel |
|---|---:|---|
| `sniff-core` | 3.353 | Domínio puro: config, types, snapshot, properties, contrast, storage, error |
| `sniff-cdp` | 1.503 | Protocolo raw: WebSocket, JSON-RPC multiplexado por `sessionId` |
| `sniff-engine` | 6.967 | extractor, sniffer, waiter, action, effects, ax, output |
| `sniff-flutter` | 2.939 | Segundo backend (Dart VM Service → mesmo modelo) |
| `sniff-css` / `-diff` / `-check` / `-mcp` | 2.189 / 2.172 / 3.275 / 3.058 | CLIs + servidor MCP |

### Padrões de engenharia notáveis

| Padrão | Como aparece |
|---|---|
| **Estratégias/ações como dados** | `WaitStrategy` e `Action` são **enums** interpretados — "100% estático", sem trait/dyn |
| **Uma extração em 1 `Runtime.evaluate`** | Zero round-trips (gargalo dominante); memória O(1) |
| **Dois backends, um modelo** | Flutter reusa o `JsonRpcClient` do CDP; normaliza p/ o mesmo `ElementSnapshot`/JSONL — diff/check sem mudança |
| **Pool de browser + semáforo + `RwLock`** | Reuso/relançamento transparente de recurso caro |
| **Regras como motor de política** | `sniff-css-check`: PASS/WARN/FAIL + `evidence` + `TriState` (Pass/Fail/**Unknown**) |
| **Compactação de tokens** | Dedup físico/lógico, supressão de defaults, `css_variables` escopado, hoist p/ `__meta` (~55%); delta (~79%) |
| **Campos derivados pré-computados** | `is_user_noticeable`, `computed_style_hash` (xxHash64), `contrast`, `ax` — "a IA não precisa inferir" |
| **Store com referência** | `SnapshotStore` + guarda anti-path-traversal |
| **MCP com progress + resources** | `notifications/progress`; `sniffCSS://prompts/eval`, `://schemas/eval`, `://guides/golden` |
| **Contrato de saída validável** | `docs/sniffCSS-eval.schema.json` + `eval-prompt.md` |
| **Docs com matriz de propriedade** | `AGENTS.md`: cada fato vive em **um** lugar; não duplicar |
| **Qualidade** | golden tests; integração com auto-skip sem Chrome; teste de regressão por bug |
| **Auditoria retroativa** | CHANGELOG: `contrast-aaa` era documentada mas **nunca emitida** — descoberta e corrigida |

---

## 18. A filosofia do sniff-css: "só o delta"

> **A IA deve receber só o delta.**

```
captura determinística → diff determinístico → checks determinísticos → IA (só interpreta)
   sniffCSS              sniffCSS-diff          sniffCSS-check           eval-prompt
                        └────────── 0 tokens de LLM ──────────┘
```

Cinco princípios que definem a abordagem:

1. **Medir e documentar; a IA só interpreta.** "O snapshot é a fonte de verdade." A IA cita `contrast: 4.5:1` medido, não adivinha pelo screenshot.
2. **Determinismo é um contrato**, não efeito colateral. `golden-run.md` lista os parâmetros que *precisam* ser idênticos (viewport, modo, wait, ações, auth) para o mesmo hash byte a byte.
3. **O modelo nunca vê o corpo inteiro — vê o ponteiro.** No MCP, o snapshot fica no disco e trafega só `{"__sniff": {path, nodes}}`; diff/check rodam **path-first**.
4. **Evidência > palpite.** Checks offline com `TriState` e `evidence` por linha.
5. **Otimizado por padrão.** `compact`, `custom-props`, `stabilize`, `contrast`, `ax` já vêm ON; `--full` desliga os cinco.

### O DNA da linhagem (reaparece em knudge e no katu)

| Princípio | sniffCSS | knudge | katu (a construir) |
|---|---|---|---|
| Otimizar o **contexto injetado**, não o armazenamento | "só o delta" | "o usuário é o LLM" | idem |
| **Ponteiro, não corpo** | `__sniff` reference | hints = ponteiros | artefatos por referência |
| **Medir evidência**, não julgar | checks PASS/WARN/FAIL | `outcomes`/confiança derivada | política com evidência |
| **Determinismo como contrato** | golden-run | TOON/ordem canônica | estado reconstruível |
| **Dois backends, um modelo** | web + Flutter | núcleo + portas | `Memory`/`Provider` |
| **Não deixar a IA inferir o computável** | `contrast`/`ax` | confiança Beta | facets medidos |
| **Arquivo/texto é a verdade** | JSONL snapshot | notas `.md` | eventos append-only |

> O sniffCSS é onde a intuição que depois virou knudge foi codificada pela primeira vez: **dê ao modelo o mínimo de evidência acionável e determinística, e faça o trabalho pesado sem IA.**

---

## 19. Lições e integrações do sniff-css para o katu

1. **`sniff-css-check` é o ancestral direto do `katu-policy`.** Mesma forma: regras data-driven → `Pass/Warn/Fail` + `evidence` + `TriState::Unknown`; adote também a disciplina anti-falso-positivo documentada no CHANGELOG.
2. **sniffCSS como ferramenta de verificação do katu.** Ao editar UI, rodar `capture → diff → check` e citar `contrast`/`oclusão`/`backdrop-over-modal` como evidência — diferencial pronto para integrar (CLI + MCP já existem).
3. **"Só o delta chega ao modelo" é a regra de contexto do katu.** Persistir artefatos e trafegar **referências**, igual ao `__sniff`.
4. **Estratégias como dados → política como dados.** `WaitStrategy`/`Action` enums = molde do motor de política determinístico.
5. **Dois backends, um modelo** valida (com prova em escala) a porta `Memory`/`Provider` e o binário único.
6. **Contrato de determinismo + golden tests** → o katu precisa de um `golden-run.md` próprio para o estado reconstruído.
7. **Contrato de saída validável por schema** → decisões do agente e do policy engine validadas contra JSON schema.
8. **Matriz de propriedade de docs** → adotar em katu/knudge (um fato, um lugar).
9. **Toolset = vários binários sobre um core** → `katu` + `katu-check` + `katu-diff` + `katu-mcp`.
10. **Campos derivados pré-computados no hot path** → pré-calcular o que a política precisa, a custo marginal zero.

### Cautelas

- **Menos estrito que o knudge**: usa `anyhow`/`unwrap` em testes; sem `forbid(unwrap/panic)`. Para um katu rigoroso, seguir a disciplina do knudge.
- **Especialização pesada**: `sniff-engine` (7k) + `sniff-flutter` (2,9k) mostram que o segundo backend muito diferente custa caro — começar o katu com **um** backend de memória + adapter MCP.
- **Dependência de Chrome / `effects`** são inerentes ao domínio; não copiar por copiar.

### Síntese

> **O knudge é o favorito pela filosofia de memória; o sniffCSS é a origem da filosofia de ferramenta — medir sem IA, entregar só o delta, deixar o modelo só interpretar.** O katu deve ser a síntese dos dois.

---

## 20. `agnostic-rag-rlm-tool` (ARAGS) — a ferramenta que cresceu demais

Análise de [`_REF/agnostic-rag-rlm-tool/`](../_REF/agnostic-rag-rlm-tool/) (projeto próprio `st-all-one/agnostic-rag-rlm-tool`; **v0.1.0**, edição 2024, MSRV 1.85, **MIT OR Apache-2.0**). É a **penúltima ferramenta** antes do knudge — o knudge nasceu explicitamente como **contraponto** a ela.

### O que é

Servidor RAG **on-demand**, **agent-agnostic** e **LLM-free** para codebases massivas. Arquitetura **server-first**: o `arags-server` é o processo principal (long-running) e dono de todo o estado; o `arags-cli` é um **cliente gRPC fino**; o LLM é sempre **o do usuário**, usado em apenas 3 pontos (`ask`/digest, `persist`/summarize, `volunteer`/síntese RLM).

### Escala (a medida da complexidade)

| Métrica | Valor |
|---|---|
| Linhas de Rust | **~50.731** |
| Crates | **9** (`cli`, `core`, `storage`, `search`, `embedding`, `memory`, `llm`, `proto`, `server`) |
| RPCs gRPC | **28** (num só `service.proto`) |
| Tabelas SQLite | **30** |
| Migrations | **29** |
| Issues rastreadas (`sd`) | **404** (399 fechadas) |
| Ficheiros `.proto` | 10 |
| Superfície de config | `server.toml` + `arags.toml` (global) + `.arags.toml` (local) |

### A grande ideia: os **quatro espaços de conhecimento**

| | A — chunks | B — qa_cache | C — rlm_nodes | D — explorations |
|---|---|---|---|---|
| Unidade | pedaço de arquivo | pergunta→resposta | sumário L1/L2/L3 | mapa relacional orientado a objetivo |
| Origem | indexação mecânica | alguém perguntou | voluntários (bottom-up) | alguém explorou de verdade |
| Responde | "o que contém" | "já respondi isto" | "o que é este módulo" | "como as peças se conectam para X" |
| Store | `vectors.usearch` + FTS5 | espaço próprio | espaço próprio + FTS | espaço próprio (cosseno) |

**Nunca se misturam na escrita; a unified query funde os quatro na leitura.** Cada dataset carrega proveniência por `content_hash`, staleness por âncoras, confidence composto e gates de review. O modelo de confiança é *assimétrico*: **falso positivo custa mais que falso negativo** (precision > recall em dataset de hit raro) — daí `verify_on_hit` (grounding lazy) além do hash.

### Filosofia (o que o arags acertou — e interessa ao katu)

1. **O servidor é plano de dados puro, LLM-free.** "Nenhum crate de LLM no grafo de dependências do servidor." O modelo é um *cliente*. → É a **mesma firewall** que o katu quer entre kernel determinístico e provider.
2. **Determinismo por padrão, LLM opt-in (`--llm`).** Tiers de busca com latência previsível: `fts` ~5ms → `entity` ~8ms → `vector` ~21ms → `llm rerank` ~200ms.
3. **On-demand, não-recursivo** (no servidor). Rejeição deliberada do loop de agente do lado do data plane.
4. **Agent-agnostic** — CLI/gRPC consumível por OpenCode/Cursor/Pi/Aider; formatos para máquina (`text`/`jsonl`/`full_json`).
5. **Confiança explícita** — proveniência, staleness, decay/saliência, consolidação, review gates, quorum HMAC, time-travel (`--as-of`).
6. **Conhecimento derivado tem ciclo de vida** — salience (recência/frequência/idade), evicção, `stale_reason` granular.

### Por que não pegou: a complexidade operacional

O custo **não foi a ideia, foi a superfície**. Cada capacidade somou um dataset, um store, um RPC, um comando CLI, um knob, uma migration, uma suíte de testes e uma doc — complexidade **multiplicativa** e, pior, **operacional** (é preciso *rodar* um servidor):

1. **Server-first = daemon long-running** — Docker, tokens, roles, TLS/mTLS, `data_dir`, backup. Custo de aquisição para uso diário (knudge: binário único, sem servidor/DB/daemon).
2. **Camada social antes do segundo usuário** — refresh tokens, auth, trust scoring, quorum BFT-leve por HMAC, submissions, exclusions, review gates.
3. **RLM recursivo** (Planner→Solver→Synthesizer) com budget, depth routing, snapshots/replay, observability events, token counter, compaction, result cache — um motor de pesquisa acoplado a uma ferramenta de memória.
4. **4 datasets × 4 espaços vetoriais** = 4× a mesma máquina replicada.
5. **Sinais de peso**: "recuperação pós-incidente" (trabalho perdido em arquivos apagados antes do commit, `CRITICAL_RECUPERATION/`); plano de **code-quality remediation** com 14 arquivos >300 linhas (um de 1116), payload duplicado ×3, complete→store não-transacional, SQL por interpolação, lacunas de cobertura.

> **Metalição:** uma RAG que fica maior que o agente que ela serve é negativa líquida. O arags virou uma **plataforma** (servidor + auth + ML + orquestração + governança) para *operar*, não uma ferramenta para *usar*. O knudge é a correção: mantém a *insight* (conhecimento derivado, ancorado, economicamente injetado) e apaga a *plataforma*.

### Linhagem direta arags → knudge

Os quatro espaços viraram tipos de nota do knudge; proveniência/hash → `--anchor PATH`; staleness por âncora → staleness de nota; "propose, never act" → a postura de que conhecimento derivado precisa de evidência; floats de confiança → confiança derivada Beta-Bernoulli.

---

## 21. Lições do arags para o katu

1. **A firewall LLM-free é a maior lição.** `katu-core`, `katu-policy` e `katu-tools` **não podem** depender de crates de provider — o modelo é cliente. O arags prova que dá para construir todo o plano de dados sem tocar no grafo do LLM.
2. **Tiers de capacidade com latência/custo conhecidos.** Expor níveis (determinístico → modelo barato → modelo caro) e deixar a **política** escolher. Regra: o determinístico resolve primeiro; o LLM é opt-in.
3. **A taxonomia dos quatro espaços como vocabulário da porta `Memory`** — sem copiar os quatro motores. Distinguir notas por origem/ciclo de vida (conteúdo, Q&A, sumário, mapa relacional) cabe no knudge sem 4 HNSW.
4. **Proveniência por hash + staleness por âncora é design provado** (o knudge já usa `--anchor`). Tática: `verify_on_hit` — nunca confiar em derivado cacheado; **re-verificar barato no uso**.
5. **Erro assimétrico (precision > recall) para conhecimento derivado** — melhor re-explorar que agir sobre mapa errado. É um princípio de *política* hardcodável → mapeia nos **bloqueios de runtime** do katu.
6. **Confidence/decay como função pura no core** (`f(similaridade, drift, idade, feedback)`) — testável isoladamente, sem I/O. O veredito do `katu-policy` deve ser função pura e determinística.
7. **Escrita fire-and-forget; validação local antes da rede** — persistência de memória nunca pode bloquear o caminho crítico do agente.
8. **A checklist de "quando persistir"** (≥5 arquivos lidos, conexão não-óbvia, sobrevive à tarefa, explica mecanismo) e os **anti-padrões** (hipótese-como-fato, âncora frouxa, mapa-enciclopédia, re-persistir, confundir datasets) são **regras que o arags só documentou** — o katu deve **enforçá-las** em runtime. É exatamente a tese do katu (§11).
9. **Contrato de mapa em 4 secções** (Mapa / Conexões / Evidências / Limitações): a `## Limitações` é "honestidade barata" — o katu pode **exigir** a declaração do que não foi verificado.
10. **Unificado na leitura, separado na escrita** — tactic para combinar fontes de memória sem acoplar os stores.
11. **Orçamento de contexto com garantias**: mínimo para o cru, teto (`summary_ratio`) para o resumido. Tática para a montagem de contexto do katu.
12. **Deferir o caro**: time-travel (`--as-of`) é valioso mas caro — deixar para depois.

### O que **não** copiar

- Server-first/daemon para ferramenta pessoal; multi-usuário (auth/roles/quorum/review) antes do segundo usuário; 4 motores/espaços vetoriais; RLM recursivo; o padrão "nova capacidade = replicar o molde inteiro" (complexidade multiplicativa); a superfície acumulada de schema/API/config.
- **Disciplina de config**: os 3 ficheiros de config e dezenas de knobs do arags são um alerta — katu deve ter **uma** config, poucos knobs, defaults sãos (cf. sniffCSS "otimizado por padrão").

### Padrões de engenharia do arags que valem reter

- **Lints da casa**: `unsafe_code = deny`, `unwrap_used = deny`, `expect_used = deny`, `panic = deny` (mais estrito que o sniffCSS; alinhado ao knudge).
- Arquivos ≤300 linhas, testes em ficheiros dedicados, SQL 100% parametrizado (`json_each(?)` para listas), tie-break determinístico no RRF, `spawn_blocking` para CPU/DB, audit log, config em dois escopos (global + local gitignored), Docker musl/scratch com modelo assado, consolidação com `dry_run`.

### Síntese

> **O arags é a memória que virou plataforma; o knudge é a plataforma reduzida a memória; o katu deve ser o kernel que *obriga* a memória a ser bem usada.** A maior herança do arags para o katu é negativa e vale ouro: **a fronteira LLM-free do plano de dados.**

---

## 22. Táticas concretas para o katu (destilado do arags)

| Tática | Origem no arags | Aplicação no katu |
|---|---|---|
| Firewall LLM-free | servidor sem crate de LLM | `katu-core`/`policy`/`tools` sem dependência de provider |
| Tiers determinístico→caro | tiers de busca | `katu-policy` escolhe o nível; determinístico primeiro |
| Re-verificação no uso | `verify_on_hit` | validar evidência/âncora antes de agir |
| Precision > recall | score composto | bloquear ação sobre derivado duvidoso |
| Função pura de veredito | decay/confidence puros | policy testável sem I/O |
| Escrita assíncrona | persist fire-and-forget | memória nunca bloqueia o loop |
| Checklists viram regras | "quando persistir"/anti-padrões | bloqueios de runtime, não docs |
| Contrato com `## Limitações` | mapa em 4 secções | exigir declaração de incerteza |
| Orçamento de contexto | mínimo cru + teto de resumo | montagem de contexto com garantias |
| Config única | 3 ficheiros no arags | uma config, defaults sãos |

---

## 23. A matemática do knudge — mapa e filosofia numérica

Revisão da matemática embutida no knudge ([`_REF/knudge/`](../_REF/knudge/)), a partir do spec [`wiki/specs/matematica.md`](../_REF/knudge/wiki/specs/matematica.md) e verificação no código de `knudge-core`. O objetivo do spec é documentar **de onde vêm as fórmulas e as constantes**, não só os valores — um padrão de documentação a copiar.

### Mapa spec → código

| Modelo | Spec | Código (`knudge-core/src/`) | Decisões |
|---|---|---|---|
| BM25/IDF por campo | §1 | `retrieval/bm25.rs` | D35–D38, D173 |
| RRF/pesos | §2 | `retrieval/rrf.rs`, `retrieval/weights.rs` | D81, D123, D179 |
| Beta-Bernoulli + Wilson | §3.1–3.3 | `lifecycle/beta.rs` | D87, D189 |
| Confiança composta | §3.4 | `lifecycle/confidence.rs` | D87, D175, D177, D203 |
| Retenção FSRS-like | §4 | `lifecycle/retention.rs`, `lifecycle/shelf_life.rs` | D44, D154, D190 |
| Drift de âncoras | §5 | `lifecycle/decay.rs`, `lifecycle/drift.rs` | D43, D86, D203 |
| Drift de termos KL/JS | §6 | `lifecycle/term_drift.rs` | D208 |
| PageRank/PPR | §7 | `graph/rank.rs` | D192 |
| Louvain | §8 | `graph/communities.rs` | D193 |
| MinHash/LSH | §9 | `write/dedup/lsh.rs` | D204 |
| PERT/CPM | §10 | `task/flow.rs` | D205 |
| Cosseno/embeddings | — | `embeddings/vector.rs`, `embeddings/semantic.rs` | D79, D102, D123 |
| Stemming PT | — | `retrieval/stem.rs` | D206 |
| TMS/derrota de crenças | — | `graph/tms.rs` | D208 |

### Filosofia numérica (hospedada no core)

- **Tudo `f64` determinístico, nunca RNG.** MinHash e PageRank usam **hash determinístico / semente fixa**. Ordem canônica (`BTreeMap`/`BTreeSet`/sort estável) em toda soma acumulada.
- **`total_cmp`** (ordem total IEEE-754), não `partial_cmp`.
- **Domínios limitados têm `clamp` explícito** (`clamp01`); `NaN` → `0`.
- **Nada derivado é persistido** (D87): confidence, drift e retenção são calculados no read; só índices rebuildáveis (`.idx/*.jsonl`) existem.
- **Funções puras no core**, sem I/O e sem deps pesadas — o cálculo é consumível e testável isoladamente (proptest de monotonicidade/invariância).

---

## 24. Fórmula a fórmula (verificado no código)

**BM25** — `score = type_weight · (1 + 0.1·confirmação) · Σ_f w_f · Σ_t IDF_f(t)·tf·(k1+1)/(tf + k1·norm)`; `k1=1.5`, `b=0.75`; pesos de campo `statement 3.0`/`tags 2.0`/`body 1.0`; `IDF = ln_1p((N−df+0.5)/(df+0.5))` **por campo**; `type_weight` de `decision 1.20` a `epic 0.70`; corte de alta frequência `df/N ≥ 0.9` só se `N ≥ 64`. Código `bm25.rs` confere; `sieve_pays_off` decide entre peneira de postings e varredura.

**RRF** — `RRF(x) = Σ_c peso_c/(k + rank + 1)`, `k=60`; pesos `lexical 1.0`, `anchor 2.0`, `semantic 30.0`, `ppr 0.0`; desempate `(score desc, id asc)`; degrada se um canal falta (R33). O peso 30 é **medido** (Pareto-dominância no corpus PT-BR) e faz o canal vetorial praticamente vencer.

**Beta-Bernoulli + Wilson** — posterior `Beta(1+s, 1+f)`; média `(1+s)/(2+s+f)`; confiança usa o **limite inferior de Wilson 95%** (`z=1.96`), conservador: 1 sucesso ≈ 0.21, 20 ≈ 0.84. Código `beta.rs` implementa a álgebra de Wilson corretamente (`center = p + z²/2n`, `variance = p(1−p)/n + z²/4n²`).

**Confiança composta** — `clamp01((similaridade·age + lower_bound(s,f) + 0.2·feedback + tarefa + 0.05·age·(1−similaridade)) · drift_factor)`; `age = 1/(1+idade/90)` (hiperbólica, meia-vida 90d); `drift_factor = 1 − 0.5·drift ∈ [0.5, 1]`; contradição = `−0.1` no lado perdedor. Monotonicidade garantida por proptest.

**Retenção FSRS-like** — `R(t) = limiar^(t/ttl) = exp(−t/S)`, `S = ttl/ln(1/limiar)`; prazo cresce `base·(1 + 0.5·reviews)`; origem `max(created, último ensaio, último uso se renew)`; `ttl ≤ 0` ⇒ `R=1` (`foundational`).

**Drift de âncoras** — `drift = 1 − válidas/total`; `drift_factor = 1 − 0.5·drift`; demove se `fração < 0.5` e `idade ≥ 30d`; varredura limitada (prof. 32, 20k entradas).

**KL/JS** — `KL(P‖Q)=Σ P·log2(P/Q)` (base 2, smoothing `1e-9`); `JS = ½KL(P‖M)+½KL(Q‖M) ∈ [0,1]`; é o `topic_drift`; propõe revisão se `JS ≥ 0.5` **e** `≥ 4` notas (mediana de `created_at` separa janelas).

**PageRank/PPR** — iteração de potência `r = (1−d)p + d·Σ r/outdeg + d·D·p`; `d=0.85`, ≤32 iter, tol. L1 `1e-8`; massa *dangling* redistribuída por `p`; semente = working set (default desligado).

**Louvain** — *local moving* + agregação, `ΔQ ∝ w_{i,c} − k_i·Σ_tot(c)/2m`; empate fica na comunidade corrente (`EPSILON 1e-9`); auto-laço ×2 na agregação (correto para preservar `2m`); ≤8 níveis, ≤64 passos.

**MinHash/LSH** — 64 permutações (FNV-1a + splitmix64), *banding* 16×4; `P(colisão)=1−(1−s⁴)^16` ≈ 1 para `s=0.92`; candidatos ainda passam pelo Dice exato ⇒ recall ≥ (nunca perde par real); ativa só acima de 256 notas; ganho **−97%** em N=1000.

**PERT/CPM** — `L(id) = dur + max L(deps)`; duração = lead time; memoizado, ciclo ⇒ vazio; desempate `(maior total, caminho mais longo, menor lexicográfico)`.

**Embeddings** — vetores `f32` normalizados (L2), cosseno ou produto interno; `rank_query`/`duplicate_pairs` fazem **varredura completa** (O(N)/O(N²)).

**Stemming PT** — sufixos conservadores, radical mínimo 4 bytes, plural normalizado antes do corte derivacional; `-ões→-ão`; medido **+25% nDCG@5** (`bench/t11_stemming.md`), sem alterar `normalize`/`id`/`body_hash`.

---

## 25. Observações, riscos e lições de engenharia numérica

### Observações (defeitos/latências, não bugs)

1. **Três curvas de decaimento coexistem**: confiança usa **hiperbólica** `1/(1+x/90)`, retenção usa **exponencial** `limiar^(t/ttl)`, salience de arags usava exponencial de meia-vida. Coerente por peça, mas sem um único modelo.
2. **A confiança é soma que satura**: `clamp01` de uma soma de sinais limitados perde resolução no topo; evidência (≤1) domina por construção.
3. **Peso vetorial 30 é load-bearing**: o RRF vira quase-vetorial; só funciona porque o corpus é pequeno e comprime ranks. Risco se o corpus crescer → **re-medir**.
4. **Sem ANN no código**: apesar do `plan/proposals/reranking_ann.md`, `rank_query`/`duplicate_pairs` são força bruta — aceitável no corpus pessoal, não escala.
5. **Spec/código (term drift)**: `matematica.md` §6.1 diz “fold + stem”, mas `term_drift.rs` usa `content_terms` (fold + stopwords, **sem** stem). Divergência de doc menor — a fonte de verdade é o código.
6. **`retention_for` fixa o limiar em 0.5** (`DEFAULT_REVIEW_THRESHOLD`), embora `stability_days` aceite limiar — o limiar não é configurável na prática.
7. **`partial` conta 0.5 em ambos os lados** (sucesso e falha vernacular): cada partial soma 1 a `n`, o que reduz o limite inferior — deliberado e documentado, mas sutil.
8. **BM25 com boost duplo**: a confirmação intrínseca (D38) está em `score_doc`, e o canal lexical ainda aplica o boost de tarefa (D108) — fontes diferentes, mas um leitor pode somar duas vezes.

### Lições de engenharia numérica para o katu

1. **O núcleo de scoring é puro, sem I/O, sem deps, sem RNG** — `knudge-core` tem 4 deps. O katu deve hospedar a matemática de política/scoring num módulo com a mesma disciplina.
2. **“De onde vêm as constantes” é documentação de primeira classe** (`matematica.md`): cada valor tem derivação, medição e arquivo. Adotar em `katu-policy`.
3. **Determinismo não é opcional**: ordem canônica, `total_cmp`, tie-break explícito, hash determinístico no lugar de RNG. Sem isso, o katu não pode ter reconciliação reproduzível.
4. **Derivar no read, nunca persistir** (`confidence`/`drift`/retenção) — mantém os bytes de verdade pequenos e o cálculo auditável.
5. **Degradação graciosa** (R33): canal/sinal ausente nunca aborta; ausência = neutro. Essencial para o katu rodar com memória parcial.
6. **Constantes medidas, não inventadas** (`bench/t09`, `t11`): Pareto-dominância e nDCG@k como gate de tuning.
7. **Off-path o caro**: PageRank, Louvain, MinHash, KL/JS e PERT só rodam em `prune`/`doctor`; o hot path é BM25 + RRF + confiança O(1).
8. **Endurecimento numérico**: `clamp01` com `NaN→0`, `mul_add`, conversões via `try_from` (sem `as`), tolerâncias fixas. Bom modelo para o katu.
9. **Não importar 12 modelos de uma vez**: começar por BM25 + RRF + confiança e deferir PageRank/Louvain/MinHash/KL-JS/PERT — exatamente a lição do arags (complexidade multiplicativa).

---

## 26. `open-keyboard` — o experimento de FFI

Análise de [`_REF/open-keyboard/`](../_REF/open-keyboard/) (projeto próprio `st-all-one/...`; **v0.1.0**, edição 2024, **MPL-2.0**, single commit). É um **experimento**: IME Android (`Norlu OpenKeyboard`) com core em Rust e UI Kotlin/Canvas. O core Rust tem **~1.888 linhas em 22 ficheiros**, distribuídas em `core/` (626), `search/` (574), `ffi/` (417), `storage/` (118), `dictionary/` (109), `trie/` (35).

### Arquitetura

```
FFI (JNI, 417 linhas) → KeyboardEngine (core/) → Dual-Trie (TrieNode BTreeMap)
                              ↓                        ↓
                      search/{fuzzy,scoring,predictors}    storage (sled + ChaCha20Poly1305 + zstd + bincode)
```

O `KeyboardEngine` guarda `static_root` (dicionário embutido), `user_root: RwLock<TrieNode>`, `static_phrases`, `phrase_memory: RwLock<HashMap>`, `current_domain`/`temporal`/`semantic_topics`, `undo` (pilha LIFO cifrada) e `analytics` (atômicos).

### O que o experimento acertou

- **Separação de algoritmos puros**: `search/fuzzy.rs` é stateless e recebe `&TrieNode`; `search/scoring.rs` isola o scoring, `predictors/*` isolam NWP/temporal/tópico/completion.
- **Dual-Trie** (estático imutável + usuário mutável sob `RwLock`) — lê em paralelo, escreve serializado.
- **`new_empty()`** para testes sem o dicionário de 200k palavras (boa higiene).
- **Ideia do “Active Reinforcement Learning”**: `learn_word_with_boost(.., boost = 20)` para o usuário “puni” uma correção equivocada.
- **`unsafe` confinado ao FFI** (50 ocorrências, todas em `ffi/mod.rs`) e `NativePointer` como *value class* no Kotlin.

### Achados concretos (evidência no código)

1. **Reúso de nonce no AEAD (falha criptográfica).** `storage/mod.rs` deriva o nonce de `DefaultHasher` do palavra (determinístico) e grava sob a chave `delta.word`. Regravar a mesma palavra reusa `chave+nonce` com plaintext diferente → **reúso de keystream**, que quebra ChaCha20-Poly1305. O README vende isso como “impede reúso de fluxo” — é exactamente o contrário.
2. **Ranking não-determinístico.** `search_smart` acumula candidatos num `HashMap` (ordem de iteração aleatória) e ordena com `partial_cmp().unwrap_or(Equal)`, sem *tie-break* por `id`. A mesma entrada pode dar ordem diferente entre processes.
3. **Impureza no scoring.** `scoring.rs` chama `SystemTime::now()` por candidato dentro de `calculate_score` — o score depende do relógio e varia entre candidatos do mesmo lote; não é testável.
4. **Benchmark que não compila / claim sem medído.** `benches/fuzzy_search.rs` usa `openkeyboard::engine::KeyboardEngine` (módulo inexistente) e métodos `insert`/`search_fuzzy`; a API real é `keyboard_engine::core` + `insert_static`/`search_smart`. Ou seja, o “~500ns em 100k+” do README **nunca foi medido**. E o fuzzy aloca um `vec![0; columns]` **por nó visitado** e usa `chars().nth()` (O(m)) → custo por nó O(m²).
5. **RecencyCache não é LRU.** Ao encher, remove `usage.keys().next()` (arbitrário no `HashMap`); o próprio comentário admite.
6. **Aprendizado lossy.** `learn_word_internal` faz `base_frequency = frequency` (**sobrescreve**) e `save_delta` persiste `frequency: boost` (não acumulado); ao recarregar, a frequência vira o último boost (normalmente 1).
7. **FFI sem *refcount*.** Todas as chamadas fazem `&*(ptr as *const KeyboardEngine)`, inclusive mutantes; `destroyEngine` faz `Box::from_raw` sem garantia de que não há chamada em voo → **use-after-free/double-free** se o Kotlin destruir enquanto uma predição roda. Com `panic = "abort"`, qualquer panic no JNI **derruba o processo** (sem `catch_unwind`).
8. **Chave por omissão zero.** `initEngine` só copia a chave se `j_bytes.len() == 32`; caso contrário usa `[0u8; 32]` silenciosamente. A “chave de sessão” do undo é `chave XOR 0xAA` — ofuscação, não segredo novo.
9. **“Dynamic Hitboxes (WCAG AAA)” não suportado.** `get_next_char_probabilities` usa `child.base_frequency`, que só é setado em nós **terminais** → nos filhos (não-terminais) é 0, logo `.max(1)` torna as probabilidades praticamente **uniformes**.
10. **Zeroização parcial.** `TrieNode::drop` só faz `children.clear()` (não zeroiza); o `impl Zeroize` existe mas não é chamado no drop. A claim de “zeroização física ao fechar” só vale para a pilha de undo e as chaves.
11. **Pesos mágicos desbalanceados.** `bonus_trigram = 30` / `bonus_bigram = 15` vs `weight_freq = 0.4·ln(F+1)` e `weight_recency = 0.4·e^{−λΔt}` (λ=0.0001/s ⇒ meia-vida ~1,9 h). O “modelo multi-sinal” é, na prática, “contexto vence”.
12. **Dependências mortas.** `dashmap` e `rayon` estão em `Cargo.toml` e **não são usados** em `src/`.

### Lições para o katu

1. **Pureza e Clock injetado** → `SystemTime::now()` espalhado torna scoring não testável. katu: porta `Clock` (como o knudge) e `now` passado para funções puras.
2. **Determinismo de ordenação** → nunca `HashMap` + `partial_cmp().unwrap_or`; usar ordem canônica + `total_cmp` + *tie-break* por id (regra do knudge).
3. **Honestidade de benchmark** → bench no CI, número medído, claim ligado a teste. O katu não pode ter “500ns” sem evidência.
4. **Higiene de deps** → `cargo-machete`/`cargo-udeps` no CI; deps declaradas e não usadas são dívida.
5. **Crypto não se inventa** → nonce determinístico = reúso de keystream. Se o katu tocar em segredos, nonce aleatório/contador por chave com biblioteca auditada.
6. **Doc é contrato verificável** → o README promete 500ns, zeroização física, nonce anti-reúso e hitboxes WCAG AAA; nada disso se sustenta. katu: cada claim com teste/bench (cf. sniffCSS “doc = fonte de verdade”).
7. **FFI com guarda** → se houver fronteira, encapsular com *refcount*/handle, `catch_unwind` e validação de entrada; nunca `&*` cru mutável com destroy concorrente.
8. **Persistência idempotente** → `learn_word_internal` sobrescreve frequência e o reload perde acumulação; katu: estado derivado reconstruível e idempotente (knudge: a nota é a verdade).
9. **Pesos medidos, não mágicos** → `matematica.md` do knudge como modelo.

### Síntese

O `open-keyboard` é o **experimento de FFI/embeddability** da linhagem (Rust core + binding fino), e mostra o outro extremo do sniffCSS: lá o produto amadureceu; aqui ficou no estágio de experimento — módulos limpos e ideias boas, mas **bench quebrado, dep morosa, ranking não-determinista e uma falha criptográfica real**. É o argumento vivo de que **sem disciplina de verificação, as claims viram ficção**. Para o katu, vale como *anti-checklist*: pureza de Clock, ordenação determinística, benchmark medído, deps enxutas, crypto sem invenção, doc como contrato.

---

## 27. Panorama final da linhagem

| Projeto | Papel na linhagem | O que o katu herda |
|---|---|---|
| `sniff-css` | origem da filosofia de ferramenta | “medir sem IA, só o delta, o modelo interpreta” |
| `agnostic-rag-rlm-tool` | a memória que virou plataforma | **fronteira LLM-free do plano de dados**; tiers |
| `knudge` | a plataforma reduzida a memória | nota-verdade, anchors, confiança derivada, matemática pura |
| `open-mtr-rs` / `my-task-manager-tui` | adapters + verdades legíveis | `HttpTransport` → portas; envelope de erro |
| `open-keyboard` | experimento de FFI | **anti-checklist** de pureza/bench/crypto |

---

## 28. `security-audit-skill` — o auditor adversarial

Análise de [`_REF/security-audit-skill/`](../_REF/security-audit-skill/) (Cloudflare, **MIT**; a skill que deu origem ao *vulnerability discovery harness* do blog da Cloudflare). É um **protocolo de auditoria multi-agente** escrito só em markdown + dois validadores CJS *zero-dependency* (~5.276 linhas). Não é uma ferramenta de segurança: é um **sistema de trabalho** que torna uma corrida de auditoria **reproduzível, coberta e verificável**.

### O pipeline de seis fases

1. **Reconnaissance** → `architecture.md` + `coverage-ledger.json`. Quatro agentes de recon em paralelo (produto/stack, principais/autoridade, superfícies/sinks, execução local).
2. **Coverage-led hunting** → caçadores isolados por unidade do ledger; ondas + *coverage critics*.
3. **Candidate validation** → cada candidato vai a um **verificador fresco que tenta refutá-lo**.
4. **Structured output** → `findings.json` validado contra `report-schema.json`.
5. **Independent record verification** → agentes frescos reverificam as afirmações finais.
6. **Target-neutral reporting** → `REPORT.md`, `FINDINGS-DETAIL.md`, `NEEDS-VALIDATION.md` derivados dos registros verificados.

### As ideias estruturais (o que realmente importa)

| Mecanismo | Como funciona |
|---|---|
| **Coverage ledger determinístico** | Uma unidade por combinação material `surface × boundary × subsystem × attack_class`. `coverage_id` = referências canônicas com percent-encoding RFC 3986 unidas por `::`, ordenadas lexicograficamente, sem duplicados. O ledger **é** a claim de cobertura; uma frase “revisei auth” não é. |
| **Tríade de veredictos** | `confirmed` (trace completo + resultado observado), `needs_validation` (bloqueio exato, **sem severidade**), `rejected` (claim refutado, retido para não repetir). Cada um tem um ramo de schema com campos mutuamente exclusivos. |
| **Validação adversarial** | “Quem verifica nunca é quem encontrou.” O verificador recebe o candidato isolado, sem a conclusão de outro verificador, e tenta **refutar** a partir do fonte. |
| **Evidência > opinião** | Severidade só para `confirmed`; severidade geral ≤ impacto demonstrado; *likelihood × impact*, não desvio de checklist. Lacuna de defesa-em-profundidade **não é** vulnerabilidade. |
| **Sandbox obrigatório** | Sem rede externa, ambiente *allowlisted* vazio, alvo/toolchain read-only, escrita só em `scratch/`, limites explícitos de CPU/memória/processos/tamanho/tempo. Se falta um controle, **não executa** — vira `needs_validation` com o bloqueio exato. |
| **Write isolation** | Promoção de artefatos pelo *parent* confiável com descritores retidos, travessia *no-follow*, `fstat` (regular file, link count 1), limites por ficheiro e cumulativos, criação exclusiva no destino. Nunca `cp -r`/globs/symlinks. |
| **Governança de orçamento** | Orçamento = número máximo de invocações de agentes. **Reserva críticos e validação antes de caçar.** Se o orçamento não cobre o mínimo, marca `incomplete` com razão exata e nada roda. Nunca excede o orçamento em silêncio. |
| **Perfis** | `quick` (1 onda + 1 crítico, unidades grosseiras), `standard`, `deep` (split por subsistema e ciclo de vida, segunda passada). Perfis mudam amplitude/redundância, **nunca a régua de evidência**. |
| **Corridas aditivas** | Ledgers e findings anteriores são lidos: `prior_confirmed_same_source` (carregado, excluído da caça por essa raiz), `prior_confirmed_changed_source` (revalidação obrigatória), `prior_needs_validation` (bloqueio preservado). Estado anterior nunca suprime trabalho atual. |
| **Validadores zero-dep** | `validate-findings.cjs` e `validate-coverage-ledger.cjs` interpretam o schema diretamente, impõem limites (5 MiB, 64 níveis, 10.000 unidades), rejeitam Unicode invisível/control, caminhos perigosos, IDs de agente reservados do Windows, colisões de identidade canônica. Validam **formato e consistência do ledger**, não verdade. |
| **Modo** | *Guidance* por defeito (não autoriza o workflow completo); *full audit* só com pedido explícito. Se ambíguo, uma pergunta antes de criar ficheiros. |

### Classes de ataque relevantes ao katu

O catálogo (`ATTACK-CLASSES.md` + companheiros) é um mapa de superfície. Interessam ao katu:

- **`AI-AND-LLM.md`** — injeção indireta via conteúdo recuperado; *context bleed* entre sessões/tenants; **envenenamento de memória persistente**; confusão de papel/proveniência; injeção de argumentos em sinks; *confused deputy*; **action-confirmation e approval binding** (a ação aprovada deve ligar-se ao objeto normalizado, requester, alvo, expiração, lote); *loops* delegados sem orçamento.
- **`MEMORY-SAFETY-AND-BINARY.md`** — bounds/inteiros; **unit/pointer-depth confusion**; use-after-free/FID; **FFI pointer-length e ownership mismatch**, layout/alignment/enum, unwind/panic cross-ABI; TOCTOU; JIT/generated-code consistency; unload/reload.
- **`DESKTOP-MOBILE-AND-LOCAL-IPC.md`**, **`SUPPLY-CHAIN-AND-RELEASE.md`**, **`RESOURCE-EXHAUSTION-AND-AVAILABILITY.md`**, **`DATA-ISOLATION-AND-LIFECYCLE.md`**.

### O que é genial no desenho

- O **ledger como contrato máquina-verificável** de cobertura: transforma “fizemos uma auditoria” em um artefato computável.
- A **separação finder/verifier** com proibição de circular conclusões.
- A **régua de evidência protegida do orçamento**: perfis nunca reduzem o `confirmed` gate nem a disciplina `needs_validation`.
- **`needs_validation` como cidadão de primeira classe** — a honestidade epistémica de dizer “este facto decisivo está fora do fonte” em vez de inventar severidade.
- Os validadores são eles próprios testados (`*.test.cjs`).

---

## 29. Lições e adoções do `security-audit` para o katu

O katu quer um **kernel de política que bloqueia em runtime**. O `security-audit` é a versão madura da mesma ambição aplicada a auditoria. Mapeamento direto:

| Ideia do `security-audit` | Adoção no katu |
|---|---|
| Coverage ledger determinístico | `katu-policy` mantém um **ledger de cobertura de regras**: cada regra tem um id canônico; nenhuma superfície fica sem regra nem sem decisão `not_applicable`/`deferred` explícita. |
| Tríade `confirmed`/`needs_validation`/`rejected` | Os veredictos do kernel: `allowed`, `blocked`, `needs_human`. E todo bloqueio carrega **evidência** (qual regra, que linha, que argumento). |
| Severidade requer impacto | Uma regra só bloqueia se houver **dano concreto** demonstrado (write fora do escopo, exfiltração, perda de dados). Estilo/lint = `warn`, não `block`. |
| Verificador ≠ ator | Um **agente revisor** nunca é a mesma sessão que produziu o diff. O katu deve permitir “reviewer fresh”: contexto limpo, read-only do diff, write só no relatório. |
| Forte evidência por limite | Cada veredicto referencia `file:line` e o resultado observado; nada de prosa. |
| Sandbox com ambiente vazio + read-only + scratch | Base do `katu-exec`/`katu-sandbox`: executar só com controles completos; senão, `needs_human` com o controle em falta. |
| Write isolation com descritores retidos | Modelo para a **porta de saída** de ferramentas: promoção de artefatos com **limites de bytes**, sem symlinks, sem globs. |
| Orçamento com reservas obrigatórias | `katu-policy` impõe **orçamento de turnos/tool-calls/tokens/tempo**; regra de ouro: reservar *verificação* antes de gastar em *produção*. Se não cabe, recusa — não “corta a evidência”. |
| Perfis quick/standard/deep | Modos do katu: `--quick` (1 passe), `--standard`, `--deep` (multi-passe). Perfis nunca baixam a régua de bloqueio. |
| Corridas aditivas | Estado entre sessões (`.katu/state.json` + `coverage-ledger`) carrega o que não mudou e **revalida o que mudou** — base para a retomada multi-sessão. |
| Validadores zero-dependency | `katu` valida os seus artefatos (`state`, `ledger`, verificações) com código **sem deps externas e testado**, para não depender de um runtime de modelo. |
| Doc é contrato | O `report-schema.json` é a prova de que markdown + schema + validador substituem um framework. O katu deve nascer com schema dos seus artefatos. |
| Classes de ataque | O catálogo vira **checklist de regras padrão** do katu (memória envenenada, approval binding, confused deputy, path jail, nonce/paths). |

**Anti-lição**: o `security-audit` mostra que um *harness de multi-agentes* é poderoso — mas também é superfície. O katu deve manter a *régua* e a *verificação independente* sem obrigar uma frota de agentes: o valor está nos **artefatos determinísticos**, não no número de agentes.

---

## 30. `ai-engineering-from-scratch` — o currículo de engenharia de agentes

Análise de [`_REF/ai-engineering-from-scratch/`](../_REF/ai-engineering-from-scratch/) (**MIT**, `rohitg00/ai-engineering-from-scratch`): **523 lições, 20 fases, ~342 h**, Python/TypeScript/Rust/Julia. Não é um produto: é um **currículo**, e “as lições são o produto” (`AGENTS.md`).

### Estrutura e disciplina de engenharia

- Layout por fase/lição: `docs/en.md` (explicador com frontmatter), `code/` (+ `tests/`), `quiz.json` (6 perguntas: 1 pré + 3 check + 2 pós), `outputs/` (artefato reutilizável).
- **Espinha “Build It / Use It”**: escreve o algoritmo à mão antes de importar o framework (backprop, tokenizer, atenção, **agent loop**), depois roda o mesmo com a biblioteca de produção.
- Regras duras: **1 commit por lição**; commits convencionais `feat(phase-NN/MM): <slug>`; **apenas Mermaid/SVG** para diagramas; toda cerca de código com *language tag*; **implementações originais** (cita RFC/spec/papers, não outros currículos); *dependency allowlist* (stdlib-first; Rust = stdlib apenas); nunca commitar gerados (`catalog.json`, `site/data.js`).
- Contrato de qualidade: código roda e sai 0, demo auto-terminante, ≥5 testes, *bias check* dos quizzes, um workflow CI de **invariantes + auto-sync** (o CI regenera `site/data.js` e corrige contagens do README; *drift* é advisory no PR e auto-corrigido no main).

### As fases que interessam ao katu

| Fase | Conteúdo | Relevância |
|---|---|---|
| **13 — Tools and Protocols** (31 lições) | interface de tool, function calling, parallel/streaming, structured output, **tool-schema design**, MCP (fundamentos, server, client, transports, resources, sampling, roots/elicitation, async tasks, apps), **MCP security: tool poisoning, OAuth 2.1**, gateways/registries, A2A, OTel GenAI, **skills, progressive disclosure, invocação/routing, permissões/sandbox/trust, evals/packaging** | contrato de ferramentas e skills do katu |
| **14 — Agent Engineering** (54 lições) | agent loop, ReWOO, Reflexion, ToT/LATS, self-refine, tool use, memória (MemGPT, blocks, mem0), skills (Voyager), workflow patterns, LangGraph, AutoGen, CrewAI, OpenAI/Claude SDK, benchmarks, **failure modes**, **prompt injection/PVE**, orquestração, runtimes, evals — e a mini-trilha **31–42 “workbench”** + **43–54 “shaping the build”** | **o blueprint do katu** |
| **15 — Autonomous Systems** | long-horizon, STAR/AlphaEvolve/DGM/AI-Scientist, auto-alignment, self-improvement, coding-agent landscape, **permission modes**, browser agents, **durable execution**, **cost governors**, **kill switches/canaries**, **propose-then-commit**, **checkpoints/rollback**, constitutional AI, RSP/preparedness | governança de autonomia |
| **19 — Capstones** (87 lições) | inclui **Track A: “terminal-native coding agent”** (20–29): *loop contract*, tool registry, JSON-RPC stdio, dispatcher, **plan/execute**, **verification gates + observation budget**, **sandbox runner com denylist e path jail**, eval harness, OTel traces, demo end-to-end | **validação independente do desenho do katu** |

### As cinco superfícies de falha (fase 14 · 31)

“Um modelo capaz não basta.” As **sete superfícies do workbench**: `Instructions`, `State`, `Scope`, `Feedback`, `Verification`, `Review`, `Handoff`. A falha quando falta: o agente adivinha o que é “pronto”; cada sessão recomeça do zero; edits vazam para código não relacionado; o agente declara sucesso num 400; “parece bom” chega ao main; o construtor corrige a própria prova; a próxima sessão redescobre tudo.

O que o documento chama de *harness engineering* reduz-se a oito primitivas de sistemas distribuídos: **function, worker, trigger, runtime, HTTP/RPC, queue, session persistence, authorization policy**. Traduções: Ralph Loop = *requeue*; PEV = três workers; hooks = triggers; skills = function registry com *progressive disclosure*; sandbox = *compute plane*; MCP = workers sobre RPC com capability lists.

Receipts citados: Terminal Bench 2.0 (mesmo modelo, mudança de harness tirou um agente de fora do top 30 para o 5.º); Vercel apagou 80% das tools e subiu de 80%→100%; Harvey dobrou a precisão só com harness; **88% dos projetos de agentes empresariais não chegam a produção** e as falhas concentram-se no *runtime*, não no raciocínio.

### A tese

O valor não está no modelo nem no prompt: está nas **superfícies de execução**. É exatamente a tese do katu. O currículo dá-lhe vocabulário e desenho.

---

## 31. O workbench de sete superfícies — o blueprint do katu

A mini-trilha 14·31–42 é a especificação mais próxima do que o katu quer ser. Síntese operacional:

| Superfície | Ficheiro/artefato | Regra do katu |
|---|---|---|
| **Instructions** | `AGENTS.md` (router curto, <50 linhas) + `docs/agent-rules.md` | rules curtas, **uma por heading**, diff-friendly; router só aponta (teste de *reachability*: ≤2 saltos) |
| **State** | `agent_state.json` | task ativa, ficheiros tocados, premissas, bloqueios, **próxima ação**; escrita atómica (temp→fsync→rename); `schema_version` |
| **Scope** | `scope_contract.json` + `feature_list.json` | `allowed_files`/`forbidden_files`/`acceptance_criteria`/`rollback_plan`/`approvals_required`; **“uma feature por sessão”** por invariante de startup |
| **Feedback** | `feedback_record.jsonl` | cada comando captura `stdout_tail`/`stderr_tail`/`exit_code`/`duration_ms`/`started_at`/`agent_note`; truncagem **determinística**; `exit_code: null` ⇒ **recusa avançar**; redação no *write*; rotação a 1 MB; `parent_command_id` para retries |
| **Verification** | `verification_report.json` | **função determinística** sobre (regras, scope, feedback, diff); zero LLM; um único caminho de relatório; `block` não é sobreponível pelo agente — só por humano com `override_reason` + `overridden_by`; *coverage floor*; `--strict` promove warns a blocks |
| **Review** | `review_report.json` | revisor **nunca edita o diff**; rubrica de 5 dimensões 0–2 (problem fit, scope discipline, assumptions, verification quality, handoff readiness); <7 soft fail, <5 hard fail; **calibração** com 10–20 casos históricos, ≥80% de concordância |
| **Handoff** | pacote de handoff | o que mudou, porquê, o que falta; fecha o loop no **state file**, não no chat |

### As cinco categorias de regra (14·33)

`Startup` · `Forbidden` · `Definition of done` · `Uncertainty` · `Approval`. Uma regra que não cai numa destas provavelmente quer ser duas. Extras de produção:

- **Severidade `block`/`warn`/`info`** — o runtime só recusa em `block`; overrides vão para `overrides.jsonl` assinado.
- **Expiração** (default 90 dias) — regra sem falha em 60 dias entra em revisão; a Cloudflare mostrou que conjuntos com expiração ficam <30 regras/repo; sem expiração crescem para 80+.
- **Markdown como fonte, JSON como cache** (`agent-rules.lock.json` regenerado por *pre-commit*) — o mesmo padrão `Cargo.toml`/`Cargo.lock`.
- **Progressive disclosure**: router (<50 linhas) → rules (1 ecrã por categoria) → topic docs (só quando a task toca). Link quebrado no router é **violação de startup**.

### Scope contracts — o detalhe que o katu deve copiar inteiro

- **Globs, não paths** (`app/**/*.py`) para sobreviver a refactors entre sessões.
- **Contrato sem `forbidden_files` está incompleto** — o espaço negativo é metade do contrato.
- **Violation budgets**, não falhas binárias: slips menores em `docs/**` = `warn`; `scripts/**`, `migrations/**`, `config/prod/**` = `block` (assimetria vive **no contrato**, não no runtime).
- **`time_budget_minutes` + `network_egress` allowlist** — dimensões de escopo, não só ficheiros.
- **Merge por menor privilégio**: `allowed_files` = interseção; `forbidden_files` = união; tempo = mínimo; approvals acumulam; `None` defere, `[]` nega-tudo.
- **Rollback é parte do escopo**: contrato sem rollback não deve ser aprovado.

### O gate de verificação

Combina (rules + scope + feedback + diff) num único veredicto. Checagens típicas: comandos de aceitação rodaram; saíram 0; sem writes proibidos; sem off-scope; todas as regras `block` passam; sem `exit_code: null`; ficheiros tocados ⊂ `allowed_files`. **Determinístico, em controlo de versão, ligado ao CI; o agente não pode suborná-lo.**

---

## 32. Regras executáveis, contratos de escopo e o gate — decisões para o katu

### Onde isto vive no katu

| Componente | Papel |
|---|---|
| `katu-policy` | motor de regras + scope contracts + gate de verificação + ledger de cobertura |
| `katu-core` | loop/session persistence (`agent_state.json`, `feedback_record.jsonl`), hooks e event stream |
| `katu-tools` | registry com **tool-schema linter** e erros que ensinam o modelo |
| `katu-sandbox` | denylist + path jail + argv inspector + timeout + truncagem + sandbox OS |
| `katu-tui` | mostra `blocked`/`needs_human` com evidência e o `override_reason` |

### Decisões candidatas

1. **Regras são dados + check, não prosa.** `agent-rules.md` (fonte) → `agent-rules.lock.json` (cache). Cada regra: slug, categoria (5), severidade, `check` (função pura), `expires_at`.
2. **Aceitação é um comando com exit 0.** Sem `exit_code` não há `done`; `null` recusa avançar.
3. **Gate determinístico, separado do modelo.** LLM só no *reviewer* (qualitativo), nunca no gate de status.
4. **`scope_contract.json` por task** com globs, forbidden, acceptance, rollback, approvals, `time_budget_minutes`, `network_egress`.
5. **`feature_list.json` com invariante “≤1 `in_progress`”** verificado no startup.
6. **Override assinado** (`overrides.jsonl`) — nunca silencioso; o agente não pode sobrepor `block`.
7. **Observation ledger** (capstone 19·25): o modelo só vê os últimos N turnos; **BudgetGate** recusa quando o teto acumulado de tokens lidos é atingido. Gate chain ordenada por custo crescente: `Whitelist → Regex → Recency → Budget`.
8. **Tool-schema linter**: `snake_case`, ordem verbo-substantivo, sem tense, estável, prefixo de namespace; descrição “Use when X. Do not use for Y.” (<1024 chars); enums para conjuntos fechados; IDs tipados com `pattern`; **erros que ensinam** (`Invalid input: 'city' is required. Example: {...}`); anti-poisoning (rejeitar `<SYSTEM>`, “ignore previous”, markdown oculto).
9. **Sandbox runner** (capstone 19·26): denylist por basename, inspetor de argv (`interpreter -c/-e` = shell com passos extra), metachar de shell quando `shell=False`, path jail via `realpath` (bloqueia symlink escape), truncagem de output, timeout de wall-clock com kill do *process group*, `SandboxResult` estruturado (`denied`/`timed_out`/`truncated` + razão).
10. **PVE (Prompt-Validator-Executor)** para injeção indireta: *source tags* (`user_message`/`tool_output`/`retrieved`), validador barato por tool call, guardrail de escrita de memória (conteúdo com forma de instrução é recusado).

---

## 33. Autonomia de longo horizonte — durabilidade, HITL, checkpoints e custo

### Durable execution (15·12)

- **Workflow** = orquestração determinística; **Activity** = unidade não-determinística (chamada LLM, tool, write); **event log** durável; **replay** re-executa o workflow mas devolve resultados registados das atividades já completas.
- Chamadas LLM **encaixam** neste perfil: não-determinísticas, caras, falíveis, com efeitos.
- Checkpoint por `thread_id`; o backend importa (Postgres durável; SQLite só dev; Redis efémero).
- **Human-input é estado de primeira classe** — o workflow pausa e retoma exatamente ali.
- **Degradação dos ~35 minutos** (METR): a taxa de sucesso cai ~quadraticamente com o horizonte. Durabilidade **não corrige** isso: permite correr mais tempo do que o perfil de confiabilidade suporta — logo, combinar com HITL fresco na reentrada e *kill switches*.
- Quando **não** usar: runs < alguns minutos sem humano; retrivial read-only; tarefas que exigem um único context window.

### Propose-then-commit (15·15)

Estado da máquina: **Propose** (persistido com intenção, *data lineage*, permissões tocadas, *blast radius*, rollback, **idempotency key**) → **Surface** → **Commit** (ack positivo) → **Verify** (reler o efeito).

- **Rubber-stamp approval** é o modo de falha canónico → mitigação por **challenge-and-response** (checklist de perguntas positivas antes de habilitar o botão).
- Consequencial (sempre HITL): writes irreversíveis, transações, comunicação outbound, prod DB, FS destrutivo. Reversível (às vezes): edits locais, staging. Reads: nunca.
- **Post-action verification**: “o commit correu” ≠ “o efeito aconteceu”.
- EU AI Act Art. 14: “oversight efetivo” exclui rubber-stamp.

### Checkpoints e rollback (15·16)

- **Toda transição persiste** (não só pontos de commit).
- **Lease recovery**: worker cai, lease expira, outro retoma do último checkpoint.
- **Idempotência + precondição**: a idempotency key evita duplo-execute; a precondição confirma que o estado ainda corresponde ao aprovado (ex.: saldo > 1000).
- **Verify pós-ação**: `UPDATE ... RETURNING`, reler ficheiro e hashear, `GET` de seguida.
- **Rollback**: in-band (operação inversa), compensatório (SAGA), out-of-band (alerta humano); rollback no-op tem de ser **nomeado** no proposal.
- O incidente afiado: **duplo-execute** quando o crash ocorre entre o efeito e a persistência do estado → persistir “in-flight” antes de executar e marcar “committed” só após o verify.

### Cost governors (15·13)

Camada, não um único teto: `max_tokens` por pedido; orçamento de tokens/dólares por task; **cap por ferramenta**; `max_turns`; janelas rolantes minuto/hora/dia/mês; **limite de velocidade financeira** (ex.: >$50 em 10 min ⇒ cortar); routing por tiers; prompt caching; context windowing (compaction); HITL antes de ações caras; **kill switch** com caminho de re-enable separado. Caso real: agente de e-commerce de $1.200 → $4.800/mês após habilitar uma skill — sem cap por ferramenta e sem alerta de crescimento.

### O que isto significa para o katu

O katu deve ser o **runtime que torna estas superfícies obrigatórias**: estado durável por defeito, HITL como *pull point* do loop (não exceção), atividade/efeito com idempotência e verify, e um stack de orçamento ligado ao gate. É a mesma tese do `security-audit`: **a régua vive no runtime**, não no pedido ao modelo.

---

## 34. Oportunidades de brainstorm destiladas

| # | Oportunidade | Fonte | Esforço | Valor |
|---|---|---|---|---|
| 1 | **`katu-policy` como motor de regras 5-categorias** com severidade, expiração e `check` puro | 14·33 | médio | altíssimo |
| 2 | **Gate de verificação determinístico** + `verification_report.json` + overrides assinados | 14·38, 19·25 | médio | altíssimo |
| 3 | **`scope_contract.json` + `feature_list.json`** com merge por menor privilégio e violation budgets | 14·36 | médio | alto |
| 4 | **Feedback runner** com redação no write, rotação, `parent_command_id`, refuse-on-null | 14·37 | baixo | alto |
| 5 | **Coverage ledger de regras** com `coverage_id` canónico e estados explícitos | security-audit | médio | alto |
| 6 | **Verificador independente** (finder ≠ verifier) como modo do katu | security-audit | médio | alto |
| 7 | **Tríade `allowed`/`blocked`/`needs_human`** com evidência e sem severidade em `needs_human` | security-audit | baixo | alto |
| 8 | **Observation ledger + BudgetGate** na gate chain | 19·25 | médio | alto |
| 9 | **Sandbox runner** (denylist + argv + path jail + timeout + truncagem) | 19·26 | médio | alto |
| 10 | **PVE + source tags** contra injeção indireta e envenenamento de memória | 14·27 + security-audit | médio | alto |
| 11 | **Propose-then-commit + idempotency key + verify + rollback nomeado** | 15·15/16 | alto | alto |
| 12 | **Durable execution** com workflow/activity/event log e **pull points** | 15·12, 19·20 | alto | alto |
| 13 | **Cost governor stack** + kill switch com re-enable separado | 15·13 | médio | alto |
| 14 | **Tool-schema linter** integrado ao registry | 13·05 | baixo | médio-alto |
| 15 | **Validadores zero-dep** dos artefatos do katu (`state`, `rules.lock`, `ledger`, `findings`) | security-audit | médio | alto |
| 16 | **Handoff packet** como artefato de fim de sessão que alimenta o `agent_state.json` | 14·40 | baixo | médio |
| 17 | **Reviewer rubric** 5 dimensões + calibration set | 14·39 | médio | médio |
| 18 | **Eval-driven development**: 3 camadas, cases junto ao código, gate de regressão no CI | 14·30 | alto | alto |
| 19 | **Rules no CI**: falhar o build se regra `block` falhar no último run do agente | 14·33/38 | baixo | alto |
| 20 | **Router `AGENTS.md` <50 linhas** + colisão de link = violação de startup | 14·32/33 | baixo | médio |

### O que **não** copiar

- **A frota de agentes como requisito.** O `security-audit` escala para uma frota, mas o valor do katu está nos artefatos determinísticos; comece com um agente + um verificador.
- **Rubber-stamp HITL** (“Approve?”) — só challenge-and-response conta como oversight.
- **Um único teto de custo** — o stack por escalas de tempo é o que realmente apanha loops.
- **Idempotência sem precondição** — evita duplo-execute mas não evita agir sobre estado obsoleto.
- **Gate probabilístico** — o gate de status nunca chama um LLM.

### Tensões a resolver no katu

1. **Rigidez vs. velocidade**: regras `block` a mais matam o fluxo. Resposta: severidade calibrada + violation budgets + `--strict` opt-in por branch.
2. **Estado durável vs. simplicidade**: event sourcing é poderoso e caro. Resposta: começar com snapshot atómico; event log só quando a retomada multi-sessão for real.
3. **Verificação independente vs. custo**: um verificador dobra o custo. Resposta: perfis — `quick` sem verificador (mas sem claim de `confirmed`), `standard`/`deep` com verificação obrigatória.
4. **katu como runtime vs. katu como harness de multi-agentes**: o `ai-engineering` mostra as duas leituras. O katu deve ser o *kernel* determinístico; multi-agente é um modo, não a fundação.

---

## 35. `docling` — o SDK de documentos (IBM / LF AI & Data)

Análise de [`_REF/docling/`](../_REF/docling/) (`docling-project/docling`, **MIT**, **v2.130.0**, `requires-python >=3.10,<4.0`). Nascido na IBM Research Zurich, hoje sob a **LF AI & Data**. Converte PDF, DOCX, PPTX, XLSX, HTML, EPUB, LaTeX, Markdown, e-mail (EML/MSG), imagens, áudio, vídeo e vários XML (DocLang, USPTO, JATS, XBRL) para uma representação única, o **`DoclingDocument`**, exportável em Markdown/JSON/DocTags/DocLang/HTML. Paper `arXiv:2408.09869`.

**Escala:** **~89.716 linhas de Python** em `docling/` (fora testes/docs), concentradas em `backend/` (22.671 — 30+ formatos), `datamodel/` (10.734), `utils/` (6.645), `pipeline/` (4.442), `models/` (≈4.000) e `service_client/` (4.250). `CHANGELOG.md` de **263 KB**; `uv.lock` de 2 MB.

É o **contraexemplo de escala** da linhagem: um projeto maduro (OpenSSF Best Practices, Discord, docs MkDocs/Zensical, releases semânticos, 12 workflows) que faz bem exatamente o que o katu precisa fazer num domínio muito maior.

### Arquitetura em camadas

```
entrypoints  (cli/, document_extractor)
clients      (datamodel/service, service_client)
pipeline     (BasePipeline → build → assemble → enrich → status → unload)
models       (stages: layout, ocr, table_structure, picture_*, chart, vlm_convert, ...)
core         (backend ↔ datamodel), inference_engines (transformers/vllm/mlx/onnx/kserve/API)
foundation   (chunking, exceptions, utils.ocr_language, backend_options)
```

`DocumentConverter` é a **fachada**: regista `FormatOption` (pipeline + backend + options) por `InputFormat`, com `allowed_formats` explícito, `convert`/`convert_all`/`convert_string`, cache de pipelines e `ThreadPoolExecutor`. **Muitos importadores → um modelo canônico → muitos exportadores.**

### O ciclo de vida do pipeline (padrão a copiar)

`BasePipeline.execute` impõe uma ordem fixa — `_build_document` → `_assemble_document` → `_enrich_document` → `_determine_status` — e um `finally: self._unload(conv_res)` **garantido**. Dois tipos de modelo: `build_pipe` (por página) e `enrichment_pipe` (documento inteiro, em lotes, com geradores que “**Must exhaust!**”). `TimeRecorder` instrumenta cada fase. Há ainda timeout por documento com **PARTIAL_SUCCESS** estruturado, filtragem de páginas não inicializadas e libertação explícita de imagens/backends.

---

## 36. A disciplina de engenharia do `docling` — fitness functions

Esta é a parte mais valiosa para o katu. O `docling` não confia em convenção: **codifica a arquitetura em verificações que o CI executa**.

| Mecanismo | O que faz | Comando |
|---|---|---|
| **`tach.toml`** | Declara **camadas** (`entrypoints > clients > pipeline > models > core > foundation`) e, por módulo, a lista exata de `depends_on` permitidas. Uma importação fora da lista **falha o CI**. | `uv run --no-sync tach check` |
| **`check_tach_module_coverage.py`** | Falha se **qualquer** módulo Python não estiver coberto por um módulo do tach — **não existe módulo sem governança**. | `python3 scripts/check_tach_module_coverage.py` |
| **`check_max_lines.py`** | Limite de **1.000 linhas/ficheiro** (`.py`, `.ts`, `.rs`, `.md`, `.sql`, `.yaml`…), com `.github/max-lines-ignore` dividido em **`[silent]`** (dados gerados/vendored) e **`[warn]`** (dívida que não bloqueia *este* guardrail). | hook `prek` |
| **`prek`** | Runner rápido de hooks pre-commit; `make validate` roda só nos ficheiros alterados; `make check` é read-only. | `uv run prek run --all-files` |
| **Ruff + `ty`** | Lint/format + type checker (`py310`); `[tool.ty.analysis].allowed-unresolved-imports` é uma **lista explícita de imports opcionais** (torch, vllm, mlx, scipy…). | `ty check` |
| **`dprint`** | Formatação de JSON/Markdown/YAML. | — |
| **`dco`** | Workflow `dco-advisor.yml` exige `Signed-off-by`. | — |
| **CI em camadas** | `pr-fast-checks.yml`, `pr-reminders.yml`, `checks.yml` (reutilizável, com `use_tach`/`tach_base_ref`), `ci.yml`, `ci-heavy-examples.yml`, `cd.yml`, `pypi.yml`. | — |
| **Dados de referência** | `DOCLING_GEN_TEST_DATA=1 uv run pytest` regenera goldens; **todo PR que toca dados de referência exige dupla revisão**. | — |

### O que isto ensina ao katu

- **A arquitetura é um teste, não um diagrama.** O katu deve ter um `xtask check-layers` (equivalente ao `tach`) que falhe se `katu-core` importar `katu-providers`, se `katu-policy` importar `katu-tui`, etc. — e um `check_crate_coverage` que garanta que nenhum módulo fica fora das camadas.
- **Dívida com severidade explícita.** O `[silent]`/`[warn]` do `max-lines` é a mesma ideia do `security-audit` (violation budgets) e das regras com expiração do `ai-engineering`. O katu deve ter um ficheiro de exceções versionado, nunca um `#[allow]` espalhado.
- **O comentário do `tach.toml` é exemplar em honestidade**: nomeia os dois ciclos que ainda existem (`datamodel↔backend`, `utils↔stages`) e diz que quebrá-los desbloquearia camadas mais finas. **Documentar a dívida estrutural no sítio onde ela é verificada.**
- **Fast vs heavy.** O katu precisa separar o gate rápido (fmt/clippy/schema/regras) do lento (benchmarks/evals/integração).
- **`-p no:tach`** — o `tach` tem *plugin* de pytest: arquitetura como teste de verdade.

---

## 37. Plugins com controlo de confiança + slim base + extras finos

### O gate de plugins

`BaseFactory.load_from_plugins(plugin_name, allow_external_plugins=False)` carrega entry points (`pluggy`) e **recusa silenciosamente** (com warning) qualquer plugin cujo módulo não comece por `docling.` quando `allow_external_plugins` é falso. As famílias (layout, ocr, table_structure, picture_description) registam classes + metadados `FactoryMeta(kind, plugin_name, module)`; `registered_kind` gera um `Enum` dinâmico; o erro de kind desconhecido **lista os kinds conhecidos** (*error teaches*).

Para o katu (`katu-plugins`) isto é o desenho de base: **as extensões externas são opt-in explícito**, cada plugin tem proveniência (nome + módulo), e o conjunto de plugins carregados é um artefacto observável. Combina com a `authorization policy` do `ai-engineering` e com o `allowlist de ferramentas` do `security-audit`.

### O split slim

O plano [`.plans/active/docling-slim.md`](../_REF/docling/.plans/active/docling-slim.md) (1.000+ linhas) define o split:

- **Base `docling-slim`** = 8 pacotes (~50 MB), *library-first*: `pydantic`, `docling-core`, `pydantic-settings`, `filetype`, `requests`, `certifi`, `pluggy`, `tqdm`, `langcodes`.
- **Extras finos**: `format-pdf-pypdfium2`, `format-pdf-docling`, `format-docx/pptx/xlsx/office/html/markdown/web/latex/xbrl`, `ocr-rapidocr[-onnx]/easyocr/tesserocr/mac`, `models-core`, `models-inference` (~2 GB: torch…), `vlm`, `convert-core`, `extract-core`.
- **Meta-extras de conveniência** compõem os finos (`format-office`, `format-web`, `models`).
- **CLI só no pacote completo**; o slim é biblioteca.
- Imports **lazy** para que uma instalação *ONNX-only* importe `DocumentConverter` sem puxar `torch`.

Regras duras do plano: **nenhum movimento de código**, workspace `uv`, `docling` depende da versão exata de `docling-slim`, CI publica ambos (slim primeiro).

**→ Para o katu:** `katu-core` slim (policy + loop + session, sem providers), e features `provider-*`, `sandbox-*`, `memory-mcp`/`memory-in-process`, `plugin-*`. A regra “o binário mínimo importa sem puxar o mundo” é o teste de sanidade.

---

## 38. Taxonomia de erro e estado — `PARTIAL_SUCCESS` é cidadão de primeira classe

O `docling` modela o resultado como **enum fechado + lista de erros estruturados**, não como exceção:

```
ConversionStatus: PENDING | STARTED | FAILURE | SUCCESS | PARTIAL_SUCCESS | SKIPPED
ErrorItem:         component_type, module_name, error_message, category, page_no
FailureCategory:   POLICY | CAPACITY | SOURCE_UNAVAILABLE | TARGET_UNAVAILABLE
                   | TIMEOUT | INTERNAL | BACKEND_FAILURE | INFERENCE_FAILURE | UNKNOWN
DoclingComponentType: DOCUMENT_BACKEND | MODEL | DOC_ASSEMBLER | USER_INPUT | PIPELINE
```

Duas regras explícitas no código:

1. **“um documento que completou mas registou erros não é um sucesso limpo: nunca reportar SUCCESS com `errors` não vazio”** → promove a `PARTIAL_SUCCESS`.
2. O docstring de `FailureCategory` distingue categorias de **task-scope** (CAPACITY, TARGET_UNAVAILABLE, INTERNAL), **document/page-scope** (BACKEND_FAILURE, INFERENCE_FAILURE) e **partilhadas** (POLICY, SOURCE_UNAVAILABLE, TIMEOUT). `UNKNOWN` é distinto de `INTERNAL` (“defeito conhecido do serviço”).

### Porque é isto central para o katu

É a mesma tese do envelope `Result<Resposta<T>, Erro>` do `open-mtr-rs` (§16) e da tríade do `security-audit` (§28), agora provada em escala:

- **Erro de negócio ≠ catástrofe.** Timeout e *backend failure* são resultados normais com metadados, não panics.
- **Sucesso parcial é um estado próprio.** Num agente, “correu 8 de 10 checks e 2 deram timeout” não é `Ok` nem `Err` — é `Partial` com evidência.
- **Todo erro tem componente + categoria + âmbito.** Sem isso, o retry e o gate não conseguem decidir.
- **Nunca reportar sucesso com avisos pendurados** — o gate de verificação (§32) tem de verificar esta invariante.

**→ Adoção no katu:** `ToolOutcome = Ok | Partial | Denied | Timeout | Unavailable` + `OutcomeError { component, category, scope, message }`; o gate recusa `SUCCESS` com erros; o TUI mostra o parcial com a evidência.

---

## 39. Skills dentro do pacote, `SKILL.md` como contrato, planos como artefactos

### Skills de uso empacotadas

`docling/.agents/skills/docling/SKILL.md` viaja **dentro do wheel/sdist** (descobrível via `uvx`-style library skills). Tem frontmatter com `name`, `description` (rico em gatilhos), `license`, `compatibility`, `metadata`, **`allowed-tools: Bash(docling:*) Bash(docling-tools:*) …`** e uma tabela de decisão “You need to… | Use | Reference”, mais *rules of thumb*. As referências profundas (`cli.md`, `python-sdk.md`, `rag.md`, `service-client.md`, `slim-packaging.md`, `extraction.md`) só são carregadas quando precisas.

**Padrão a copiar pelo katu:** o binário/skill `katu` deve trazer o seu próprio `SKILL.md` (uso) + skills de desenvolvimento (`.agents/skills/`), e a skill de engenharia (`dignified-python` é o modelo: “quando usar”, tabela de alternativas, deteção de versão, carregamento condicional por gatilho).

### Multi-harness por symlink

`.claude/skills/*`, `.codex/skills/*` e `.opencode/skills/*` são **symlinks** para `../../.agents/skills/*`. Uma fonte de verdade, N harnesses — exatamente o padrão do `ai-engineering` (§31), aqui materializado no sistema de ficheiros.

### Planos e notas de release como artefactos versionados

- `.plans/{active,completed,archived}/` — planos de trabalho reais em markdown, incluindo o `docling-slim.md` que é o *design doc* do split.
- `.actor/` — pacote Apify completo (actor.json, input_schema.json, dataset_schema.json, Dockerfile, actor.sh, README, CHANGELOG).
- `.git-blame-ignore-revs` — commits de formatação em massa ignorados no blame.
- `CHANGELOG.md` de 263 KB + `python-semantic-release`.

**→ O katu deve versionar planos e ADRs** (já o faz em `proposal/`), manter um `CHANGELOG` e um `.git-blame-ignore-revs` desde o início.

---

## 40. Oportunidades e adoções destiladas do `docling`

| # | Oportunidade | Fonte no docling | Esforço | Valor |
|---|---|---|---|---|
| 1 | **`xtask check-layers`** — camadas + `depends_on` permitidas por crate, falhando o CI | `tach.toml` | médio | altíssimo |
| 2 | **`check_crate_coverage`** — nenhum módulo fora das camadas | `check_tach_module_coverage.py` | baixo | alto |
| 3 | **Limite de linhas/ficheiro** com `[silent]`/`[warn]` versionados | `check_max_lines.py` + `max-lines-ignore` | baixo | alto |
| 4 | **Gate de plugins opt-in** com proveniência e allowlist | `BaseFactory.load_from_plugins` | médio | alto |
| 5 | **`ToolOutcome` com `Partial`** + `OutcomeError{component,category,scope}` | `ConversionStatus`/`ErrorItem` | baixo | altíssimo |
| 6 | **Regra “nunca SUCCESS com erros”** no gate | `base_pipeline.execute` | baixo | alto |
| 7 | **Ciclo de vida de fase com teardown garantido + profiling** | `BasePipeline.execute` | baixo | alto |
| 8 | **Timeout que produz parcial, não perda total** | `document_timeout` em `PaginatedPipeline` | baixo | alto |
| 9 | **Slim base + extras finos + meta-extras + CLI separada** | `docling-slim.md`, `pyproject.toml` | médio | alto |
| 10 | **Imports lazy** para o binário mínimo importar sem puxar o mundo | `ConvertPipeline` (torch só se `do_chart_extraction`) | baixo | alto |
| 11 | **Skill de uso empacotada** + `allowed-tools` no frontmatter | `docling/.agents/skills/docling` | baixo | médio-alto |
| 12 | **Multi-harness por symlink** a partir de `.agents/skills` | `.claude`/`.codex`/`.opencode` | baixo | médio |
| 13 | **Dados de referência com regeneração explícita + dupla revisão** | `DOCLING_GEN_TEST_DATA=1` | baixo | alto |
| 14 | **Fachada + registo de formatos com allowlist** | `DocumentConverter`/`FormatOption` | médio | alto |
| 15 | **CI em camadas** (fast PR vs heavy vs CD) | `.github/workflows` | baixo | médio |
| 16 | **Erro que ensina** — listar os kinds conhecidos | `_err_msg_on_class_not_found` | baixo | médio |
| 17 | **Timeout por documento + filtragem de artefactos não inicializados** | `PaginatedPipeline` | baixo | médio |
| 18 | **`.git-blame-ignore-revs`** desde o primeiro refactor de formatação | `.git-blame-ignore-revs` | trivial | baixo |

### O que **não** copiar

- **Escala por acumulação.** 30+ backends e ~90k linhas são o resultado de anos e de um caso de uso “todos os formatos”. O katu deve resistir a essa acumulação (a lição do ARAGS, §20).
- **`tach` sobre “core” com ciclos tolerados** — é dívida nomeada; o katu deve **começar** sem ciclos e manter as camadas finas.
- **Dependências de inferência no caminho base** — o próprio docling corrigiu isso com o slim; não repetir o erro.
- **CHANGELOG de 263 KB** — o katu deve automatizar (semantic-release) desde o início, não escrever à mão.

### Síntese

O `docling` é a **prova de maturidade** da mesma filosofia que o katu quer seguir: arquitetura verificada por ferramenta, plugins com confiança explícita, binário mínimo com extras finos, e — sobretudo — **um resultado modelado como estado (`PARTIAL_SUCCESS`) mais erros estruturados**, não como exceção. Onde o `open-keyboard` (§26) mostrou como as claims viram ficção sem verificação, o `docling` mostra o contrário: **quando a arquitetura, o tamanho dos ficheiros, os limites de camadas e os goldens são todos verificados no CI, a claim é o próprio build**.

---

## 41. `deepseek-harness` — a filosofia "tudo é um plugin"

Análise de [`_REF/deepseek-harness/`](../_REF/deepseek-harness/) (`deepseek-ai/deepseek-harness`, **MIT**, *developer preview*, `dsh`). Agent harness da DeepSeek AI construído sobre **Cordis** — um framework de plugins descrito no paper *A Programming Paradigm for Spatiotemporal Composability* (`arXiv:2608.25512`).

**Escala (e este número é uma lição em si):** **~403.480 linhas de TypeScript** em `packages/*/src`, **~316 pacotes** de workspace, `pnpm-lock.yaml` de 1 MB, 15 ficheiros de config de testes (vitest), **282 scripts** (quase todos com o seu `.spec.ts`), **~2.436 notas markdown** em `.agents/notes/`, apps CLI/Desktop/Web + SDK Python + addon nativo, docs em inglês + chinês + sidecar i18n.

### A tese

> **Não existe núcleo privilegiado.** Cada parte do produto é um plugin — o adaptador de modelo, o registo de ferramentas, o log de sessão e **o próprio agent loop**, de modo que cada um é substituível por configuração. Estende-se o `dsh` montando um plugin ao lado dos outros; registos são **efeitos reversíveis** que se desenrolam quando o plugin descarrega.

Cordis em cinco ideias: um **plugin** implementa `Service`; um **contexto** é um repositório de serviços (`ctx.tools`, `ctx.llm`, `ctx.sessions`); a dependência declara-se com `inject` (ordem de carga expressa por requisitos, não por sequência manual); **eventos tipados** por *declaration merging*, despachados em cinco modos (`emit`/`waterfall`/`parallel`/`serial`/`bail`) que são **parte do contrato público**; e **registos são efeitos** com *disposer*.

Um `dsh` em execução é uma **árvore de plugins composta no boot a partir de camadas ordenadas**: um **profile** (composição nomeada: `web`, `headless`, `sdk`, `sdk-minimal`, `acp`) empilha **bundles** (formato de distribuição de linhas de config Cordis) e depois aplica `cordis.patch.yml` do profile, o do *home*, e overlays `--patch`. Qualquer linha pode ser substituída; `dsh --dump-config` mostra a árvore que a máquina arranca.

---

## 42. Como funciona: turno, eventos, sessão e *capability seams*

### A hierarquia do loop

- **step** = um pedido ao modelo mais as ferramentas que ele chama.
- **turn** = zero ou mais steps; abre antes de o primeiro input ser reclamado e fecha quando nada é devido.
- **round** = iteração externa de política (goal round, tentativa Ralph).

O fluxo de turno (`turn/start → agent/pre-step → step/start → agent/request → llm/stream → tool/call* → tools/* → step/end → agent/turn-stopping → turn/end`) separa **eventos de sessão duráveis** de **pontos de extensão vivos**.

### Três domínios de evento

| Domínio | Uso |
|---|---|
| **Session events** | factos duráveis no log (`turn/*`, `step/*`, `user/message`, `assistant/message`, `tool/*`) |
| **Agent events** (`agent/*`) | observar/interceptar trabalho em voo (inbox, step, status, request, validation) |
| **Capability events** (`fs/*`, `tools/*`, `telemetry/*`) | anexar política e adaptadores a um seam sem importar o loop |

Os `waterfall` são *around-middleware*: quem só observa **tem de chamar `next()`**; quem possui a decisão pode curto-circuitar. Essa semântica está documentada e é gate-verificada contra os locais de despacho.

### A invariante central

> **Model-visible ⟺ logged.** Tudo o que chega a um pedido ao modelo tem de ser reconstruível a partir do log de sessão. Um novo input visível ao modelo exige um evento de sessão. Uma invariante de runtime verifica-o.

Daí: o log é a **fonte da verdade**; `deriveMessages()` projeta o histórico do modelo a partir dele; `assistant/attempt` retém tentativas falhadas/retentadas/canceladas **sem acrescentar histórico**; fork/resume/transcripts/telemetria/persistência **derivam** dos mesmos assentamentos duráveis. O formato tem gerações versionadas (`session.vN.jsonl`), migrações adjacentes `vN→vN+1`, e a regra de que **gerações publicadas nunca são renomeadas, substituídas ou apagadas**.

### *Capability seams*

Um **seam** é uma capacidade trocável com **três papéis**: `Service Definition` (a classe abstrata/registo que possui `ctx.<key>` e os tipos), `Service Provider` (implementações), e `Consumer` (quem injeta o serviço). **Uma embalagem pode combinar papéis, mas um papel só não é um seam.** Exemplos: `ctx.shell` (`dsh-shell` + `bash-local`/`bash-sandbox` + `tool-bash`); `ctx.fs`, `ctx.subprocess`, `ctx.sandbox`, `ctx.llm`, `ctx.sessions`, `ctx.approval`.

O efeito prático: "um swap de provider muda o produto inteiro". Filesystem e subprocess **partilham um mundo de execução**, logo apontá-los para um sandbox remoto move o Bash, o PTY e o LSP com eles, sem forks.

### O pipeline de ferramentas

```
tool/call (logado ANTES de executar)
  tools/pre-execute  (waterfall: hooks, permissão, sandbox)
  guards monotónicos  (negam ou abstêm-se; identidade protegida)
  [ask] ctx.approval → ausente/não-respondível = deny
  tools/execute      (waterfall around: timeout, retry, métricas)
    toolBody → fs/write-intent (só tool-fs)
  projectContent     (conteúdo preparado)
  tools/post-execute (aceitar, bloquear, substituir, ADD CONTEXT)
  normalização externa (throws viram isError)
  finalizeContent    (última invariante
  tools/result       (notificação síncrona; resultado congelado)
  tool/result        (evento de sessão; outcome único visível ao modelo)
```

Duas ideias dignas de cópia: a **ordem é por custo e raio de explosão** (allowlist → regex → recência → orçamento, cf. §32), e o guard de repetição (`repeat-tool-reminder`) é **consultivo, nunca veto**: enriquece a decisão de pós-execução com contexto para o modelo, sem bloquear — conta repetições exatas `(nome, argumentos canónicos)` por agente num `WeakMap`, ignora chamadas não-rastreadas (não lavam o loop), **conta as negadas**, e reinicia com nova mensagem do utilizador.

---

## 43. A disciplina de confiança: invariantes, aprovação, sandbox, defesa

### Invariantes de runtime

`ctx.invariants` é um registo configurável; **cada pacote publica um companion `./invariant`** que regista verificações sob o seu nome npm exato. Seleção por *allowlist*/*blocklist* regex; validação **falha alto** no arranque (entrada em branco/duplicada/inválida lança); instalação numa *child fiber*; falha é atribuída ao pacote (`invariant violated by "<package>": …`) sem que o registo importe qualquer pacote de produto.

A regra de ouro: **só publicar `./invariant` quando observações independentes podem divergir.** De resto, omitir a fiação e registar no README porquê — *empty installers* e verificações de presença de serviço são inválidas. `verify-package-invariants` rejeita *markers* gerados e *installers* vazios sem explicação.

### Aprovação — *fail closed*

`ApprovalOutcome` é um fechado: `allowed-once` | `rejected` | `cancelled` | `unavailable`. **`allowed-once` é a única concessão**; quem chama nega em todas as outras. Um *answerer* ausente, não-proprietário, que lança, ou que devolve fora do vocabulário torna-se `unavailable` — **nunca** abre a porta. A política por sessão é `ask`/`never`, e `never` é aplicada **dentro do serviço antes do waterfall**, de modo que um *answerer* registado depois com `prepend` não a pode contornar. Cada pedido produz um par de auditoria `approval/asked`+`approval/decided` **que não entra no transcript** do modelo.

`ApprovalRequest` **omite deliberadamente os argumentos da ferramenta** — a UI liga o prompt à tool call já emitida através do `callId`, para não renderizar uma segunda cópia que poderia divergir.

### Sandbox — a execução como facto relatado

```
SandboxMode:        read-only | workspace-write | danger-full-access
SandboxEnforcement: full | partial
```

- **Só os dois primeiros modos chegam a um provider**; `danger-full-access` nem chama `ctx.sandbox`. **Silent unconfined passthrough nunca é legal para uma política confinada.**
- **A fiscalização é um facto relatado, não uma promessa:** `partial` significa que um backend ativo ou um ABI de kernel antigo governa apenas um subconjunto, e **quem exige a promessa absoluta tem de rejeitar ou expor a distinção**. (Landlock ABI antigo e o backend ACL do Windows são os casos parciais atuais.)
- **A política é por chamada**, não fixada no provider: duas sessões podem confinar sob políticas diferentes no mesmo instante, e um retry escalado aprovado é uma **nova chamada** com política mais larga.
- **`denialSignatures` são o dialeto do backend** (EROFS no bwrap, EACCES no Landlock, EPERM no Seatbelt) — usar a **união** entre backends reivindica negações que um dado backend nunca produz.
- **`RunnerFailureRule`** exige uma **conjunção**: código de saída não-zero (com *gate* opcional de códigos permitidos) **e** uma assinatura fatal dentro de **uma** linha de stderr, depois de remover por igualdade exata de linha inteira as linhas **informativas**. "O status de saída sozinho nunca prova falha do runner."

### Padrões defensivos (cada um é uma classe de bug que efetivamente escapou)

1. **Relatar outcomes ortogonais independentemente** — um processo pode expirar *e* sair 0; expor `timedOut`, `signal`, `exitCode` cada um por si, nunca aninhar um flag no ramo do outro.
2. **Honrar contratos públicos dos dois lados** — normalizar representações antes de devolver pela API pública.
3. **Estado assíncrono ≠ estado síncrono** — `whenIdle()` não é o resultado de um *follow-up*; o *guard* corta para os dois lados (se a transição esperada nunca pode ocorrer, a espera pendura).
4. **Dispose tem de atingir quiescência, não pedi-la** — matar *e esperar*; fechar registos de listeners **antes** de matar.
5. **Conter exceções de callbacks no dispatcher** — um listener que lança não pode rejeitar a *promise* nem esfomear os seguintes.
6. **Nunca dar a uma saída não-confiável o ambiente ambiente nem caminhos previsíveis** — *scrub* de `*KEY*`/`*SECRET*`/`*TOKEN*`/`*PASSWORD*`; ficheiros temporários/spill em diretório privado `0700`, nomes aleatórios, abertura exclusiva `'wx'`/`0o600`.
7. **Desligar caminhos em forma de link** — `lstatSync().isSymbolicLink()` + `unlinkSync`; reservar `rmSync` recursivo para diretórios reais (o Windows lança `ERR_FS_EISDIR` num *junction*).

---

## 44. O sistema de conhecimento: postmortems, agent notes, docs e testes

Esta é a parte mais transferível do projeto — e a que o katu mais precisa. O harness trata **conhecimento e verificação como artefactos de primeira classe, gate-verificados**.

### Postmortems

Um postmortem é escrito quando um bug é **sutil** (mecanismo não óbvio), **sistémico** (a razão de escapar é uma lacuna de testes/ferramentas/convenções) e **caro de redescobrir**. Formato: **Executive summary** (30 s: o que quebrou, causa em linguagem simples, por que escapou, lição durável) → Summary → Impact → Timeline (com sequências do log) → Root cause → **Guardrails added** → **Lessons**. Não é uma Agent Note (que regista uma decisão); é um registo retrospetivo de falha.

Os quatro postmortems são um currículo de "verificação de produto":

| # | Falha | Lição |
|---|---|---|
| 0001 | `export default apply` fez o Loader descartar `inject`; o servidor ACP morreu ao conectar **com 178 testes verdes e 100% de cobertura** | *Hand-mounted tests bypass the real load path.* Testar a **entrada real** (Loader, subprocesso, artefacto publicado). |
| 0002 | `disabled: !!js ...` só é interpolado em `config`, não em metadados de entrada → ferramentas de filesystem **permanentemente desativadas**; e o *snapshot refresh* aceitou `UNKNOWN_TOOL` como saída esperada | *A snapshot refresh is fixture production, not correctness review.* Sintaxe aceita ≠ campo avaliado. |
| 0003 | O agente validou um servidor substituto noutro porto; tratou HTTP 200, build OK e boot manifest como factos intermutáveis | HTTP readiness ≠ build success ≠ boot manifest. **Verificar o origin exato.** |
| 0004 | Um prefixo de stderr partilhado (`landlock-run:`) fez o código confundir aviso benigno com falha do runner, escondendo `SANDBOX_UNAVAILABLE` | **Um prefixo partilhado não é um protocolo.** Atribuição exige conjunção de evidências; exclusões informativas exatas; desconhecido = falha-fechada. |

O 0004 é particularmente valioso por ter **quatro root causes** conceptuais distintas na mesma página: (a) o tipo de resultado público só conseguia expressar um saco de substrings; (b) o consumidor juntou factos de processos diferentes; (c) a matriz de testes espelhava a representação defeituosa; (d) um adaptador substituiu um erro estruturado da camada de baixo por um genérico.

### Agent Notes — decisões como artefactos versionados

Caminho `{lifecycle}/{class}/yyyy-mm-dd-topico.md`, com **ciclo de vida** (`proposed/`, `implemented/`, `rejected/`) e **classe** de conjunto fechado (`feature`, `bug-fix`, `simplification`, `architecture`, `process`, `testing`). Regras notáveis:

- **`## Alternatives considered` é obrigatório** — "uma decisão registada sem aquilo que venceu convida a re-litigar". Alternativas são registadas, nunca inventadas.
- **`implemented/` fala no presente e é mantido atual** com o que realmente foi entregue (ficheiros, nomes, chaves), mas **nunca se edita uma nota para uma decisão diferente** — substitui-se com uma nova e mantêm-se ligadas.
- **Eliminar** quando a implementação é mecânica/local; **arquivar** (congelando permanentemente) apenas quando a decisão está completa e o racional já não guia trabalho futuro; **rejeitar** uma proposta obsoleta.
- Formato verificado por gate (`verify-agent-note-format`), árvore de classes verificada (`agent-note-tree.ts`), arquivo *append-only* com hashes de sidecar.

### Documentação com orçamento

**Um facto, um lar.** Uma taxonomia de tiers explícita (root `AGENTS.md` = ordens permanentes ≤1.950 palavras; `architecture.md` ≤2.400; subtree ≤600; package README = contrato). Regras de escrita: **uma linha física por parágrafo**; blocos `ts` têm de compilar (`doc-typecheck`); `ts type-equiv` para tipos colados; **catálogos gerados** do código; `verify-doc-budgets` rejeita excesso **ou ausência**; e um **slop checklist** (regras duplicadas, história fora do seu tier, anotações de estado que apodrecem, catálogos reescritos à mão, *reasoning transcripts*, *walls* de parágrafo, *emphasis inflation*, spec-speak em notas `implemented/`).

Proíbe explicitamente metáforas: *"antes de escrever `contract`, `boundary` ou `shape`, pergunte se um termo mais exato nomeia o sujeito"*. E proíbe rótulos ambíguos de origem (`prove`, `nance`).

### Testes em camadas

| Camada | Regra |
|---|---|
| **Unit** | vitest; **cada registo tem um teste de segurança de HMR** (dispor o fiber, assertar limpeza) |
| **Coverage** | gate de CI: **100% por ficheiro** em `packages/*/*/src`. *"Uma linha descoberta é muitas vezes código morto que o gate assinala para eliminação, não um teste em falta para aparafusar."* Linha coberta é necessária, **nunca suficiente** |
| **Real-API e2e** | com chave; auto-*skip* sem chave; a política explícita é *"inference is cheap here — não racionar"* |
| **Expected local** | expectativas montadas sem gravação de sessão |
| **Benchmarks** | gate obrigatório de PR em Linux; entradas sintéticas impõem orçamentos de tempo/heap/escala |
| **Snapshot** | **replay de sessão gravada sem chave** através de profiles entregues; o cenário pai fornece input e replay, e serve de resultado persistido esperado; comparação independente da árvore `workspace.expected/` |
| **Web browser snapshot** | Chromium (e WebKit no seletor de modelo); CI em modo `replay` read-only |

Cinco regras que o katu deve adotar literalmente:

1. **Verificar o mundo, não o auto-relato** — "uma asserção e2e re-executa o comando ou relê o ficheiro externamente; uma sonda de palavras-chave na própria saída do agente deixa um agente trapaceiro passar". Assertar que ficheiros não tocados ficaram byte-idênticos.
2. **Testar o caminho de entrada real** — *"um guard só guarda se a regressão o falhar"*. Para provar, **introduzir a regressão, ver vermelho, reverter**.
3. **Preferir a implementação real ao mock** — mockar só a fronteira cara ou não-determinística (adapter LLM, rede, relógio).
4. **Especs correm concorrentemente** — *"uma spec que passa só quando corre sozinha é um defeito na spec"*.
5. **Cada spec possui os seus recursos** até ao teardown, mesmo em falha/retry/timeout.

---

## 45. O que extrair para o katu

### Incorporar (direto)

| # | Ideia | Porque |
|---|---|---|
| 1 | **Capability seam com três papéis** (Definition/Provider/Consumer) como unidade de crate | É a articulação mais nítida das portas do katu: `katu-memory`, `katu-shell`, `katu-fs`, `katu-sandbox`, `katu-llm`. "Um papel só não é um seam" impede meias-abstrações |
| 2 | **Model-visible ⟺ logged** como invariante de runtime | Garante reprodutibilidade e auditoria; o log é a verdade |
| 3 | **Log de sessão como fonte da verdade + projeções** (`deriveMessages`, `stateOf`, `snapshot`) e **tentativas retidas sem histórico** | Resolve fork/resume/telemetria com uma só fonte |
| 4 | **Eventos com modo de despacho no contrato** (`emit`/`waterfall`/`parallel`/`serial`/`bail`) e a regra "waterfall tem de chamar `next()`" | O katu só tem um `EventBus` genérico; isto dá-lhe semântica |
| 5 | **Invariantes de pacote com registo + seleção + falha alta**, publicadas só quando observações independentes podem divergir | Faz do "runtime blocking" verificável e atribuível |
| 6 | **Aprovação fail-closed** (`allowed-once` é a única concessão; `unavailable` nega) + política `ask`/`never` imposta antes do waterfall | O katu quer runtime control; este é o desenho correto |
| 7 | **Presets** que agrupam botões independentes (modo sandbox + política de aprovação) | Uma opção de UI, N invariantes por baixo |
| 8 | **Sandbox: modo + fiscalização relatada (`full`/`partial`) + dialetos de negação por backend + `RunnerFailureRule` conjuntivo** | Substitui o "path jail" ingénuo do capstone por um desenho honesto |
| 9 | **Nunca passthrough não-confinado silencioso** | Uma linha, um invariante |
| 10 | **Os 7 padrões defensivos** (outcomes ortogonais, dispose→quiescência, conter callbacks, scrub de env, unlink de links…) | Cada um é uma classe de bug real |
| 11 | **Postmortems com guardrails e lições** como tier próprio | O `security-audit` provê a epistemologia; isto provê o formato narrativo |
| 12 | **Agent Notes com `Alternatives considered` obrigatório** e ciclo de vida | Os ADRs do katu ganham um contrato verificável |
| 13 | **Um facto, um lar + orçamentos de palavras + catálogos gerados + slop checklist** | Impede a explosão documental que o próprio projeto sofre (2.436 notas) |
| 14 | **E2E que verifica o mundo, não o auto-relato** | A cura direta do fracasso do `open-keyboard` (§26) |
| 15 | **Testar o caminho de entrada real** (binário construído, subprocesso, profile) | O postmortem 0001 é o argumento definitivo |
| 16 | **IDs *branded*; uniões fechadas com `assertNever`; falha alta na configuração** | O katu é Rust — o análogo é *newtypes* + enums `#[non_exhaustive]` + erro de arranque |
| 17 | **Sem *tunables* hardcoded em plugins**; escolhas que variam por deployment são campos `Config` validados | katu: `Config` tipada, `DEFAULT_*` não é configurabilidade |
| 18 | **Explicito > implícito na fronteira**: o default é um passo `resolve(request): Spec` explícito, nunca um `?? default` escondido dentro de `run()` | Aplica-se literalmente ao `katu-shell`, `katu-fs` |
| 19 | **"Requer um dono e necessidade atuais"** — amarrar cada abstração, máquina de estados, opção, cópia defensiva e caminho de compatibilidade a um contrato ou consumidor atual | Anti-YAGNI institucionalizado |
| 20 | **"Impor a decisão na operação que a toma"** — omissão de schema, filtragem de prompt, fachadas, wrappers e ordem de listeners **não são enforcement** quando um chamador direto ou alternativo os contorna; testar a negação **pelo executor** | A regra mais katu-relevante do documento |
| 21 | **"Publicar estado só no ponto de commit"**; derivar caches/prompts/UI/replay de uma fonte autoritativa | |
| 22 | **Aplicar limites ao resultado completo** (bytes, tokens, itens, tempo) onde o valor emitido/re tido é conhecido; testar limites minúsculos, exatos, chunks únicos enormes e limites multibyte | |
| 23 | **Política de dependências**: preferir dependências mantidas a *hand-rolling* quando apagam código **e testes**; *vendor* fixado com SHAs | Complementa a "higiene de deps" do §26 |

### Lapidar (adaptar, não copiar)

- **Seams + eventos sim, contentor de DI geral não.** O Cordis é poderoso, mas o custo é visível: *fiber walk*, *shadow*, **dois postmortems causados pela semântica de carregamento do próprio framework**, 316 pacotes. O katu deve ter **composição estática** (feature flags de compilação + um registo pequeno), não um `ctx` dinâmico.
- **Composição por config tipada, não por DSL.** O postmortem 0002 (`!!js`) é a prova: um YAML com expressões é um risco de segurança e de correção. O katu usa TOML + `serde` com tipos fechados e *fail loud*.
- **Cobertura como sinal de código morto**, não como meta. 100% por ficheiro é ótimo como alerta; perigoso como objetivo (o 0001 tinha 100% e o produto não funcionava).
- **Snapshot/golden como fixture, nunca como prova.** O 0002 mostrou um suite verde a fixar uma regressão.
- **Estrutura de docs em pirâmide só com os gates.** Uma taxonomia de 10 tiers sem `verify-doc-budgets`, `verify-md-links` e `doc-typecheck` degenera; com eles, é sustentável.

---

## 46. O contra-checklist do `dsh` para o katu

> Cada linha é uma pergunta cuja resposta "sim" é um alarme.

1. **O katu está a construir um framework de plugins como *primeiro* produto?** Se sim, parar. O `dsh` tem o plugin framework *e* o produto; o katu deve ter o kernel de política primeiro e o plugin host depois.
2. **Há um "core" privilegiado a remendar?** No `dsh`, isto é tido como o erro de desenho. No katu, `katu-policy` pode ser esse núcleo — mas **deve ser pequeno, determinístico e sem dependências de provider** (a fronteira LLM-free do arags, §20).
3. **Cada abstração tem um dono e uma necessidade *atuais*?** Se não, é dívida (o `dsh` exige consumidor de produção ou evidência).
4. **A decisão é imposta na operação que a toma, ou num wrapper/filtro/ordem de listener que um chamador alternativo contorna?** Testar a negação pelo executor.
5. **Um guard só guarda se a regressão o falhar?** Para cada gate novo: introduzir a regressão, ver vermelho, reverter.
6. **Os testes e2e verificam o mundo ou a auto-descrição do agente?** (Relê o ficheiro, re-executa o comando, asserta ficheiros não tocados byte-idênticos.)
7. **Alguma spec só passa quando corre sozinha?** É defeito da spec.
8. **Um snapshot refresh está a ser tratado como revisão de correção?** Não: é produção de fixture; impossibilidades semânticas precisam de asserções independentes.
9. **A cobertura está a ser usada como prova de que a funcionalidade funciona?** Linha coberta prova que a linha correu, não que o produto funciona.
10. **Um prefixo/assinatura partilhada está a ser usada como protocolo de atribuição?** Exigir conjunção de evidências independentes + exclusões exatas + falha-fechada no desconhecido.
11. **Há passthrough não-confinado silencioso?** Nunca é legal para uma política confinada.
12. **O estado é publicado antes do ponto de commit?** Notificações e estado derivado só depois do sucesso.
13. **O `dispose` pede quiescência ou atinge-a?** Fechar listeners **antes** de matar; esperar pelos filhos.
14. **O ambiente ambiente ou caminhos previsíveis chegam a saída não-confiável?** Scrub de segredos; `0700`; `wx`/`0600`.
15. **A documentação está a escrever um *reasoning transcript*?** Manter o contrato durável, eliminar o caminho.
16. **Há regras duplicadas em dois sítios?** Um lar por facto.
17. **A documentação repete catálogos que o código/gate podia gerar?** Gerar e gate-verificar.
18. **Há *tunables* hardcoded em vez de `Config` validada?** Um `DEFAULT_*` ou *test hook* não é configurabilidade.
19. **A dívida estrutural está documentada onde é verificada?** (cf. o comentário honesto do `tach.toml` no docling, §36.)
20. **O katu está a crescer para 316 pacotes?** A superfície é o custo; a lição do arags (§20) e do `dsh` é a mesma em escalas diferentes.

### A frase que resume o projeto

> *"Não existe núcleo privilegiado para remendar: estende-se o `dsh` montando um plugin ao lado dos outros, e registos são efeitos que se desenrolam quando o plugin descarrega."*

O katu quer ser um **kernel de política** que impõe regras em runtime. O `dsh` propõe o oposto filosófico: **sem kernel**, tudo composto e substituível. As duas teses convergem num ponto que ambos subscrevem — **a fronteira onde a decisão é tomada tem de ser explícita e verificável**. O valor para o katu não está em adotar "tudo é um plugin"; está em adotar a **disciplina de seams, invariantes, verificação da entrada real e postmortems** que tornam qualquer arquitetura — com ou sem kernel — auditável.

---

## 47. `maxima-harness` — o rascunho fundacional do katu

Análise de [`_REF/maxima-harness-proposal-2026-08-18-stallone-main/`](../_REF/maxima-harness-proposal-2026-08-18-stallone-main/). **Este não é um projeto de referência: é o antepassado direto do katu** — construído pelo mesmo autor, sobre o Pi Agent, com a tese que o katu ainda subscreve ("limitar o agente e forçá-lo a seguir um caminho único") e com os resultados que o katu tem de não repetir. É o documento mais importante desta série, porque é **evidência empírica** e não opinião.

**Escala:** 919 ficheiros, 8,8 MB. **19 extensões TypeScript (~4.682 LOC)**, **5 subagentes**, **38 skills** (8 do harness + 28 guias vendorados), **17 tools de seeds**, **3 temas**, **4 prompts**, `bin/pi` de **672 linhas de bash**, um `install.sh` de 26 KB, `Dockerfile` + entrypoint + compose ARAGS, **100 testes** em 12 ficheiros, `.seeds/issues.jsonl` com **127 issues** (126 fechadas), wiki de 11 documentos + `historico.md` de 40 KB com **§12.1–§12.38** datados, e um `plan/` de 43 documentos de design.

### A arquitetura em três camadas

```
Host          bin/pi (bash, 672 linhas)  → deteta container alvo, monta volumes, docker run
Container     maxima-pi (alpine+musl, digest pinado)  → pi + docker-cli + sd + sniffCSS + python3
Sessão        19 extensões TS + 4 skills + AGENTS.md  → guardrails, contexto, tasks, subagentes
```

O isolamento é Docker; o *controlo* é feito por extensões que interceptam eventos do pi. O `bin/pi` injeta `UID/GID` do host (o agente nunca é root), monta `~/.pi/agent` em *named volume* (ou o home inteiro, opt-in), monta o `docker.sock` e **resolve o workdir real do container via `docker inspect --format '{{.Config.WorkingDir}}'`** (§12.33 — antes era `-w /app` cego e quebrava projetos em `/var/www/html`).

### O que o harness inventou (inventário)

| Peça | LOC | Papel |
|---|---|---|
| `maxima-subagents.ts` | 949 | single/parallel(8, 4 conc.)/chain, runtime por agente (provider/model/temperature/maxTokens/thinkingLevel), widget ao vivo, log JSONL, viewer read-only |
| `maxima-seeds.ts` | 571 | 17 tools sobre o CLI `sd` (task antes de codar, anti-batch-close) |
| `maxima-rules.ts` | 493 | `@rule` / deteção de banimento → subagente `guardrail_analyst` → regra JSON persistida |
| `maxima-memory.ts` + `arags.ts` + `knowledge.ts` | 839 | ponte ARAGS (RAG híbrido) + injeção de contexto + sugestão de persistir |
| `maxima-design.ts` | 332 | `DESIGN.md` como fonte única visual, tokens oklch+hex, soft-guardrail de paleta |
| `maxima-context-monitor.ts` | 258 | teto de contexto, âncora, auto-compactação, resumo com modelo barato, `.STAGING.md` |
| `maxima-module-loader.ts` | 230 | `@modulo` → `MODULE.md` (hierarquia de conhecimento) |
| `maxima-guardrails.ts` | 176 | bloqueio determinístico (bash regex + paths), `rm`→`.pi/trash/`, live reload |
| `maxima-tool-loader.ts` | 92 | `search_tools` — mantém o prompt enxuto, ativa tools sob demanda |
| `maxima-efficiency.ts` | 56 | auto-router heurístico anti-overthinking (opt-in) |
| `maxima-global-rules.ts` | 61 | injeta as regras globais no system prompt se o projeto não tem AGENTS.md próprio |
| `maxima-conversation-logger.ts` | 186 | JSONL grep-friendly em `.pi/conversation/`, lock à prova de PID morto |
| `maxima-sniff.ts` | 196 | 3 tools sobre o binário `sniffCSS` (cópia embarcada do §17) |
| `maxima-container-router.ts` | 132 | roteia comandos build/test → `docker exec` no container do projeto |
| `maxima-thinking.ts` | 82 | `set_thinking_level` em runtime |
| `maxima-host-warning.ts` | 29 | banner "EXTREMA CAUTELA" no system prompt no modo host |

### O fluxo único que o harness quis impor

`AGENTS.md` abre com a frase que define a tese:

> "Este documento **É o contrato de comportamento**. Obedeça **como regra, não como sugestão**. Onde diz SEMPRE / NUNCA / FAÇA / PADRÃO, é obrigatório."

E define-o linearmente: **1. abrir task → 2. consultar conhecimento → 3. decidir escopo e delegar → 4. implementar → 5. testar → 6. persistir conhecimento → 7. fechar task**, com "Compactação/perda de memória → escreva `.STAGING.md` ANTES". Complementado por: **delegue por padrão** (roteamento categoria→agente), "NUNCA codar sem task", "persistir conhecimento tem prioridade sobre modificar código", "o loop principal fica fino (`defaultThinkingLevel: low`)".

---

## 48. A tese e o mecanismo: "regra, não sugestão" contra o teto do substrato

O documento `plan/implementation/14-guardrails.md` contém, escrito em 2026-08, **o núcleo da tese do katu**:

> "Princípio fundamental: **segurança via extensões, não via instruções ao modelo**. O modelo não decide se um comando é seguro — a extensão decide antes dele ver."

Com a tabela comparativa — e é aritmética de confiabilidade, não filosofia:

| Abordagem | Confiabilidade | Porquê |
|---|---|---|
| Instrução no `AGENTS.md` | **~70%** | O modelo "esquece" ou "interpreta differently" |
| Guardrail via extensão | **100%** | Executa **ANTES** do modelo ver o comando |

E o mesmo documento afirma que o `return { block: true }` significa que "o modelo **nunca vê o resultado** — ele **não pode** tentar de novo ou contornar".

### O problema: só duas categorias de regra chegaram ao lado dos 100%

O substrato (o Pi) expõe exatamente isto — e nada mais:

```
pi.on('session_start') ×8   pi.on('input') ×7    pi.on('before_agent_start') ×6
pi.on('tool_call') ×5       pi.on('turn_end') ×2 pi.on('session_shutdown') ×2
pi.on('session_before_compact') ×2 · session_compact · session_compact_failed
tool_result · context
pi.registerTool ×44 · pi.registerCommand ×4 · pi.setThinkingLevel ×3
pi.setActiveTools/getActiveTools · pi.exec ×3 · ctx.ui ×40 · ctx.hasModel/compact/sessionManager
```

Consequência direta: **o único ponto síncrono e sancionável é o `tool_call`, e só para `bash` (regex sobre a string) e `write`/`edit` (substring do caminho)**. Logo:

| Superfície do caminho único | Como foi "imposta" | Categoria |
|---|---|---|
| `rm` → `.pi/trash/` | regex em `tool_call` + live reload | ✅ determinístico |
| `sudo`, `git push/commit/reset --hard`, `chmod 777`, `docker rm/rmi/stop/kill` | regex em `tool_call` | ✅ determinístico (com buracos, §49) |
| paths protegidos (`.env`, `.git/`, `node_modules/`) | substring em `write`/`edit` | ✅ determinístico (frágil, §49) |
| **task antes de codar** | 3 camadas de *prosa* (skill + hook que injeta protocolo + AGENTS.md) | ❌ instrução |
| **delegar por padrão** | *prosa* + `promptGuidelines` em tom imperativo | ❌ instrução |
| **consultar conhecimento primeiro** | *prosa* ("EXPLORER: exija que ele devolva `exploration_id`") | ❌ instrução |
| **persistir conhecimento tem prioridade** | *prosa* | ❌ instrução |
| **código: ≤30 linhas, ≤3 params, DI, logs estruturados** | *prosa* | ❌ instrução |
| **respostas ≤5 linhas, sem preâmbulo** | *prosa* (a extensão `maxima-brevity.ts` planeada **nunca foi implementada**) | ❌ instrução |
| **design: só tokens do `DESIGN.md`** | *prosa* + soft-warn ("nunca bloqueia") | ⚠️ aviso |
| **`.STAGING.md` antes de compactar** | *prosa* + instrução inserida no prompt de resumo | ❌ instrução |
| **fechar task só depois de testado** | *prosa* | ❌ instrução |
| **Raciocínio enxuto / Chain of Draft** | *prosa* + heurística de thinkingLevel | ⚠️ heurística |

**O harness contradiz o seu próprio documento de arquitetura.** Diz que instruções valem 70%, e depois entrega ~85% das suas regras como instruções, com um `AGENTS.md` que ordena "obedeça como regra". O `historico.md` é o registo de pagar esse preço: §12.34 ("Agente ignorava seeds → reforço em 3 níveis"), §12.38 ("o agente 'Min' raramente evocava subagents → AGENTS.md reescrito em tom determinístico"), #36 ("o agente criava todas as seeds no início e fechava todas de uma vez"). **A resposta ao incumprimento foi sempre mais prosa.**

### As duas provas de que o teto era do substrato

1. **O `rm` que escapava.** O regex original só casava `rm` no *início* da linha; `cd x && rm arquivo` passava. A correção (§10) não foi estrutural — foi alargar o regex para "qualquer posição". Sintoma de um motor de política que opera sobre *texto* em vez de sobre *factos*.
2. **O `node_modules` remendado.** Para renomear o rodapé da TUI para `🤖 Min` (§29), o `Dockerfile` **faz patch de `dist/bundle/chunks/chunk-*.js`** — o doc ainda nota que "o ficheiro solto em `modes/interactive/components/footer.js` é artefacto não carregado". Quando se patcheia o bundle de um `node_modules` para mudar uma string de UI, o substrato já disse o que tinha a dizer.

> **Conclusão que o katu tem de inscrever no seu ADR nº 1:** o conjunto de regras *imponíveis* é determinado pelo substrato. O katu não é um "maxima v2 sobre o pi"; é **o loop, possuído**. É a única forma de o lado dos 100% deixar de ser uma exceção de duas categorias.

---

## 49. Anatomia da instabilidade — oito causas nomeadas

A descrição do autor ("até funcionou, porém com muita instabilidade e limitação") é correta e diagnosticável. As causas, por ordem de alavancagem:

### 1. O substrato interceta, não governa

O pi expõe hooks; não expõe uma máquina de estados. Logo **o estado do caminho único vive na adesão do LLM a prosa** — variância por natureza. Nenhum hook consegue *negar* uma transição que não foi modelada.

### 2. Só duas famílias de regra estão no lado determinístico

Sobram ~50 regras em prosa (tabela do §48). A "garantia" do §12.34 é "reforço em 3 níveis de prosa" — três camadas de probabilidade não multiplicam para 100%.

### 3. Estado em *module scope* de extensões, não em estado de sessão tipado

`anchorActive`, `compacting`, `savedCtx`, `projectTrusted`, `projectLoaded`, `blockPatterns`, `protectedPaths`, `liveBlocks`, `liveRunning`, `lastInput`, `lastRunLogPath`, `SUBAGENT_LOGS` — *singletons mutáveis* por processo, reconstruídos (quando reconstruídos) em `session_start`. Daí a necessidade de `pi.appendEntry()` + reconstrução + códigos de *handoff*; e daí os vazamentos em fork/steer/switch. **Um caminho único exige que o estado do caminho seja uma entidade explícita, serializável e reconstruível — não 12 variáveis soltas.**

### 4. Controladores sobrepostos para o mesmo ciclo de vida

A compactação tem **seis** donos: o auto-compact nativo do pi (`reserveTokens`), o `maxima-context-monitor` (aviso 80% / compacta 90% / hard cap), a `force_compact` tool, a âncora (`/anchor`, `/compact-now`), o evento `seeds:task_completed` do `maxima-seeds`, o guard `session_before_compact` do `maxima-subagents`, e o `thinkingBudgets`. Resultado documentado: **compactação dupla** (resolvida com uma trava `compacting`) e uma "guarda anti-compactação re-entrante" que **cancela** uma compactação concorrente — admitida pelo próprio changelog como *"Heurística (…) Mitigação, não fix do core"*.

### 5. Dois (ou mais) provedores para cada capacidade

| Capacidade | Backends simultâneos |
|---|---|
| Memória | **ARAGS** (`arags search`) **e** o legado HTTP `MAXIMA_MEMORY_API_URL` (mantido "como fallback" para sempre) |
| Skills | vendoradas na imagem **e** `/host-skills` montado **e** `.agents/skills` do projeto (merge no entrypoint) |
| Subagentes | `agents/*.md` globais **e** `.pi/agents/*.md` do projeto (sobreposição por nome) |
| Regras | `blockPatterns`/`protectedPaths` (determinística) **e** `rules[]` (semântica, só prosa) |
| Config | flag > env > `.pi/config.json` > `~/.pi/maximapi/config.json` > default (**5 camadas**) |
| AGENTS | do projeto **e** `maxima-global-rules` injetado **e** a cópia do entrypoint |
| Motor de memória | Ollama **e** candle/MiniLM assado ("/models") |
| Chrome | Chromium interno **e** Chrome do host via CDP |

Cada dualidade dobra o espaço de estados e torna o teste combinatório impossível. O próprio `HISTORICO.md` existe **porque** os documentos deixaram de corresponder à realidade.

### 6. Sem testes no caminho de enforcement

Os 100 testes concentram-se nos *extratores puros*: `parseRuleJson`, `isViolation`, `classifyInput`, `transformInput`, `mergeRule`, `renderRules`, `normalizeThinkingLevel`, `eventDisplayLine`, `subagentLogName`, oklch↔sRGB, o parser de frontmatter. **Nenhum teste conduz o loop real do pi e assere que uma negação ocorreu.** É exatamente o postmortem 0001 do `deepseek-harness` (§44): *hand-mounted tests bypass the real load path*. Daí as regressões reaparecerem (rm encadeado, steering a terminar a âncora, subagentes ignorados, batch-close de seeds).

### 7. A fronteira real (Docker) é entregue ao agente

O `docker.sock` é montado no container do agente. A documentação lista "Docker Socket: acesso controlado via `docker exec` (não acesso direto)" — **factualmente falso**: montar o socket *é* acesso ao *daemon API*; `docker exec` é uma convenção, e o guardrail tem um **`early return` explícito para `docker exec`**. O que está bloqueado é `docker rm/rmi/stop/kill`, mas **`docker run` (incl. `--privileged`), `docker cp`, `docker build`, `docker exec` em qualquer container, `docker network`, `docker volume` não estão** — isto é, o denylist tem buracos precisamente onde o poder está. E existe o escape de primeira classe `pi --host`: *"EXTREMA CAUTELA — sem isolamento"*, um banner + prosa. É o **oposto** de um desenho *fail-closed* — cf. o `SandboxMode` do `deepseek-harness` (§43), que **nunca** faz passthrough não-confinado silencioso.

### 8. O motor de política opera sobre texto, não sobre factos

- **Regex sobre a string do comando** — vulnerável a `r''m`, `\rm`, `R=rm; $R -rf x`, `find -delete`, `> ficheiro`, `dd`, `git clean -fdx`, `python -c "shutil.rmtree(...)"`, `base64 -d | sh`, `npm run <script arbitrário>`, e a **escrita do script primeiro e execução depois** (que passa pelo `bash` uma vez).
- **Substring sobre o caminho** — `protectedPaths.some(pp => path.includes(pp))` gera falsos positivos (`.env.bak`, `x/.enviar`) e **não resolve** `../`, não canonicaliza symlinks, e não vê escritas via `bash` (`echo > .env`) nem via `exec_in_project` (dentro do container do projeto). TOCTOU por construção.
- A afirmação "o modelo nunca vê o resultado, não pode contornar" é **falsa** na parte que importa: pode tentar de novo com outra string.

### Divergências documento↔código (o próprio `HISTORICO.md` as reconhece)

| Documento diz | Código faz |
|---|---|
| "Orquestrador **≤100k** (garantia absoluta, inegociável)" (docs 16 e 19) | `DEFAULT_HARD_CAP_TOKENS = 150_000`, e o `HISTORICO.md` registra "Teto 150k, reserveTokens 50000 \| §12.6" |
| `reserveTokens: 100000` (docs 02/16) | `settings.json`: `reserveTokens: 50000` |
| `keepRecentTokens: 15000` (doc 19) | `settings.json`: `keepRecentTokens: 20000` |
| `defaultTools: []` — "nenhuma tool built-in, apenas extensões" (docs 00/01/02/24) | `settings.json`: `defaultTools: ["read","bash","edit","write","grep","find","ls"]` — **as sete built-ins ativas** (e o doc 09 nota que "`defaultTools: []` não remove as tools built-in") |
| "11 extensões TS" (`wiki/panorama.md`) | **19** ficheiros em `extensions/` |
| `maxima-brevity.ts` como camada 3 da mitigação (doc 19) | não existe |
| Instalação via `apt`/`curl`, `.maxima-pi.yml` como config primária, `registerProvider` legacy | removidos (HISTORICO) |

### Métricas sem medição

`plan/implementation/22-overview-comparison.md` apresenta uma tabela completa — "−77% tokens", "−60% turnos", "−70% tempo", "Consistência +40%", **"Assertividade ~70% → ~95%"** — para um cenário **escrito à mão**. São números ilustrativos apresentados como resultados. É o mesmo pecado do `open-keyboard` (§26: o benchmark que nunca compilou) e do ARAGS. O doc 19 ainda acrescenta "Economia: ~50 tokens (find+grep) vs ~500 tokens", como se medido.

---

## 50. Os padrões que funcionaram — o que adotar do `maxima`

O rascunho falhou na arquitetura e **acertou em padrões que o katu deve herdar sem hesitar**.

| # | Padrão | Evidência no harness | Adoção no katu |
|---|---|---|---|
| 1 | **`.STAGING.md` — protocolo de externalização de estado antes de perder memória**, com 5 secções canónicas (Objetivo / Estado / Pendências / Próximo / Descobertas) | `AGENTS.md` + injeção no prompt de resumo + mensagem do hard cap aponta para `.STAGING.md` e `.pi/conversation/` como fontes de reconstrução | **Schema do artefacto de checkpoint do katu** (§33, §44). O katu deve promovê-lo de prosa a *campo tipado de estado* |
| 2 | **Trust gate para config de projeto** que pode *adicionar* bloqueios e agentes | `.pi/maxima.json` e `.pi/agents/*.md` só se `ctx.isProjectTrusted()`; caso contrário pergunta com UI; **headless não-confiável = ignorado (fail-closed)** | Adotar verbatim. É corretíssimo: policy local é vetor de escalada de privilégio |
| 3 | **Separação `live` ↔ `durable` da observação** | widget ao vivo (últimas ~7 linhas, throttle 80 ms) que **não entra no chat**; transcrição completa **sempre** em `.pi/conversation/subagent/<ts>-<agente>.jsonl`; viewer read-only (`/subagentlog`, ESC/Ctrl+D/q) | Adotar. Mapeia no split *live/durable* do `deepseek-harness` (§42). Regra: **o resultado devolvido ao pai é um resumo; a transcrição é um artefacto** |
| 4 | **Resultado do subagente = status + resposta final (nunca a transcrição)** | `buildSubagentSummary` cortou o *bloat* que causava compactações em cascata | Adotar como contrato de `ToolOutcome` (§38) |
| 5 | **Extratores puros exportados para teste** | `eventDisplayLine`, `parseRuleJson`, `isViolation`, `classifyInput` — funções puras exportadas *precisamente* para teste | Adotar **e ir mais longe**: testar também o caminho de enforcement (§49.6) |
| 6 | **Regra de runtime registada pelo utilizador, com efeito imediato** | `@rule add` → `rule_add` → subagente `guardrail_analyst` (canonicalização por agente **distinto** de quem vai obedecer) → merge com dedupe por `id` → **live reload** a cada `tool_call` | Adotar a *ideia*: veto do utilizador vira **regra estruturada, versionada, com efeito na sessão corrente**. Refinar em §51.7 |
| 7 | **Precondição tipada "registar antes de agir"** | `VIOLAÇÃO` → a extensão reescreve o input com uma diretiva que obriga a chamar `rule_add` **antes de qualquer outra ação** | Adotar o **conceito** (transição tipada com pré-requisito), rejeitar o **mecanismo** (prosa) |
| 8 | **Escapes de saída estruturados para agentes analistas** | contrato JSON com `{"skip": true, "reason": "..."}` — o analista pode *recusar* a tarefa | Adotar: todo agente auxiliar do katu devolve sucesso | skip | erro tipado |
| 9 | **Heurística consultiva que nunca veta** | o soft-guardrail de paleta ("avisa ao usar cor fora da paleta — **nunca bloqueia** (falso-positivo em SVG/testes é alto)") | Adotar o princípio: **avisar sem bloquear é uma categoria legítima**, distinta de negar (cf. `allowed`/`blocked`/`needs_human` do `security-audit`, §28) |
| 10 | **Design tokens como contrato verificável** | `DESIGN.md` = fonte única; `oklch(L C H / A)` **com alpha sempre** + par sRGB hex em `:root` dentro de `@supports`; `design_tokens` devolve `hexFallback` computado; hex equivalente a um token é válido | Adotar; e a regra "mudança de design = editar `DESIGN.md` primeiro, código depois" é *"um facto, um lar"* (§44) aplicado ao visual |
| 11 | **Perceção determinística sem tokens** | aliases via `BASH_ENV`: `cat`→bat (números de linha), `ls/ll/la/tree`→eza, `du`→dust, `diff`→difftastic, `git diff`→delta, `bench`→hyperfine, `rm`→wrapper para `.pi/trash` com log JSON | Adotar o *princípio* (o delta, §18). **Cuidado:** cria divergência entre a shell que o agente vê e a real (a confusão `sd` vs `sed-rs` é um perigo documentado) |
| 12 | **Execução no ambiente do projeto, não do agente** | `exec_in_project` + `docker inspect` para resolver o `WorkingDir` real (§12.33) | Adotar o *conceito* (`katu-shell` com *cwd* canónico); rejeitar o *mecanismo* (o agente com o `docker.sock`) |
| 13 | **`.pi/` 100% fora do git, com o trade-off explicitado** | `.pi/.gitignore` = `*`; "Tradeoff aceito: recursos por projeto passam a ser locais por máquina — **compartilhamento vira cópia manual**" | **Adotar pela metade, e corrigir a metade má** em §51.6 |
| 14 | **Logger grep-friendly** | JSONL com roles distintos (`tool_call`/`tool_result`) e campos achatados; "`grep '"role":"tool_call"'`" direto; lock à prova de PID morto | Adotar para o `katu` event log |
| 15 | **Política de não-regressão global** | FIX = teste vermelho primeiro; FEATURE = testes estruturais (integração/E2E) durante a implementação; UI = sistema E2E autocontido com `historia.md` + `teste.spec.ts` 1:1 sob `cenario/` | Adotar verbatim nos ADRs do katu |
| 16 | **Progressive disclosure com gatilhos explícitos** | `AGENTS.md` (sempre) → `MODULE.md` (`@modulo`, sob demanda) → `DESIGN.md` (`@design`, sob demanda) | Adotar, com gatilhos **tipados** e medição do contexto poupado |
| 17 | **Thinking fora do contexto durável e do ecrã** | `hideThinkingBlock: true` + `Ctrl+T` para alternar em runtime | Adotar (cf. `dsh-trim-cot-leakage`) |
| 18 | **Runtime por subagente (modelo/temperatura/maxTokens/thinking) com precedência por campo** | `agents.<nome>` em `.pi/config.json` > global > env > sessão | Adotar: **um perfil por papel**, não um modelo global |
| 19 | **`search_tools` — tools secundárias desativadas até serem pedidas** | 20 tools no `SEARCHABLE`, reativadas por consulta | Adotar (contexto é o recurso escasso) |
| 20 | **Recusar vazar segredos no ambiente dos filhos** | `childEnv()` com allowlist explícita (`PATH, HOME, SHELL, …` + chaves de provider), `PI_OFFLINE=0` | Adotar (cf. "scrub de env", §43.6) |

---

## 51. O que o `maxima` prova sobre o katu — decisões

> Cada linha é uma decisão que o rascunho justifica *por experiência* e que o `katu` deve herdar (ou executar ao contrário).

### 51.1 O katu é o loop, possuído — não uma camada de regras

**Evidência:** o §48 (só `tool_call` + `bash`/`write` sancionáveis) + o §49.1 (sem máquina de estados) + o patch do `node_modules` (§49.5).
**Decisão:** o `katu-core` **é** a máquina de estados do agente. As regras são transições, não interceptações de texto. O `katu-policy` **avalia factos tipados** (`ToolUse{name, args, resolved_paths, argv}`, `Phase`, `Budget`) devolvendo `Allow | Deny{reason, evidence} | RequireApproval | NeedsHuman` — nunca um `Vec<Regex>` sobre strings.

### 51.2 O caminho único é um grafo tipado com pré-condições, não um texto numerado

**Evidência:** o `FLUXO OBRIGATÓRIO` de 7 passos teve de ser reescrito "em tom determinístico" e continuou a ser ignorado (§12.34, §12.38, #36).
**Decisão:** o estado do katu é `Task → KnowledgeConsulted → Planned → Implemented → Verified → Persisted → Closed`, e cada transição tem **pré-condição verificável** (ex.: `close` exige um `verification_report` válido; `implement` exige um `knowledge_query_id` ou um `waiver` explícito). Um `TaskClose` sem verificação é uma **recusa**, não um aviso.

### 51.3 Prioridade é ordenação imposta, não prosa

**Evidência:** "persistir conhecimento tem prioridade sobre modificar código" — uma prioridade entre duas ações, sem qualquer mecanismo. "Delegue por padrão" — uma inversão de default, feita por prosa, e que falhou (§12.38).
**Decisão:** se uma regra afirma prioridade, o motor tem de a poder *impor* — por ordem de fase, por bloqueio, por quê-orçamento. Se não pode, a regra não é escrita como prioridade; é escrita como **recomendação consultiva** (categoria do §50.9).

### 51.4 O denylist não é política

**Evidência:** `BASE_BLOCK_PATTERNS` (7 regex) + `docker exec` no *early return* + `docker run`/`cp`/`build` ausentes + substring de caminho não resolve `../` nem symlinks nem escritas via `bash` (§49.8).
**Decisão:** o `katu` trabalha com **capacidades**, não com proibições: `Capability::{ReadPath(root), WritePath(root), Exec(argv, cwd), Net(host), SpawnPty, McpSession}`. A negação é a ausência de capacidade; e o caminho é **resolvido e canonicalizado** (`canonicalize` + verificação de prefixo após resolução, `openat2`-style) antes de qualquer decisão.

### 51.5 O sandbox é uma capacidade exigida, nunca um contentor que dele escapa

**Evidência:** `docker.sock` montado + `pi --host` como flag de primeira classe + rede por env (§49.7).
**Decisão (adota o `deepseek-harness`):** `SandboxMode = ReadOnly | WorkspaceWrite | FullAccess`, com **fiscalização relatada** (`full`/`partial`) e a regra "**nunca passthrough não-confinado silencioso**". Se um controlo está ausente (ex.: `Landlock` indisponível), o resultado é `SandboxUnavailable` — e a execução **não** acontece sem uma autorização explícita e registada. O agente **nunca** recebe o socket do orquestrador de contentores.

### 51.6 A política tem de ser versionável e revisível

**Evidência:** o §50.13 — `.pi/` todo gitignored, com o trade-off aceite de que **as próprias regras do projeto (`maxima.json`, `.pi/agents/*.md`) deixam de estar em revisão**. Uma política que não se pode *code-review* não é política.
**Decisão:** separar em três: **`policy/` versionado** (regras, revisíveis em PR), **`.katu/` runtime** (sessões, logs, trash — nunca versionado), **`secrets`** (fora de ambos, só no keyring). Os secrets do rascunho (chmod 600 no `~/.pi/maximapi/config.json`) e o `auth.json` read-only já apontam para a direção certa.

### 51.7 Regra de runtime: adotar a ideia, endurecer o mecanismo

**Evidência:** o `@rule` acerta na *forma* (veto → regra canónica persistida → live reload) e erra na *substância* (trigger por prosa, armazenamento gitignored, enforcement por regex).
**Decisão — `Rule` do katu:**

```
Rule { id, statement, scope: Path|Command|Phase|Budget,
       enforcement: DenyCommand | DenyWrite | RequireBefore(Phase) |
                    RequireAfter(Tool) | Budget(cap) | Advisory,
       severity: Critical|Warn, expires: Option<Instant>, waiver: Option<Waiver> }
```

com **canonicalização por um verificador distinto** de quem vai obedecer (§29: finder ≠ verifier), trust-gated à entrada, e **versionada**. `negative`/`positive` exemplos obrigatórios: a regra só é aceite se o motor conseguir demonstrar *um comando que ela nega* — senão é `Advisory` e é rotulada como tal.

### 51.8 Checkpoint é protocolo de fase, não limiar heurístico

**Evidência:** seis donos da compactação (§49.4); trigger por palavras-chave do utilizador (`'próximo'`, `'feito'`) que a própria #30 documenta ter disparado compactação a meio da tarefa; e a heurística admitida como mitigação.
**Decisão:** o katu faz checkpoint **no limite de uma fase** (transição tipada) e escreve um artefacto durável (o `.STAGING.md` de §50.1 elevado a tipo). O teto de contexto é **um** número, com **uma** autoridade (não cinco camadas de precedência).

### 51.9 O motor de política tem de ser testado pelo caminho real

**Evidência:** os 100 testes testam extratores; as regressões voltaram (§49.6) = postmortem 0001 do `dsh`.
**Decisão:** para cada regra do katu, um teste de integração que **conduz o loop real** e assere a negação com `evidence`. E a regra de ouro do `dsh`: *"um guard só guarda se a regressão o falhar — introduzir a regressão, ver vermelho, reverter"*.

### 51.10 Instrução é uma categoria declarada, não um fallback vergonhoso

**Evidência:** o `AGENTS.md` ordena obediência a ~50 regras em prosa, enquanto o documento de arquitetura as avalia em 70%.
**Decisão:** todo texto injetado no katu é classificado — `Enforced{rule_id}` (tem motor), `Advisory{rationale}` (só conselho), ou `Perception` (contexto factual). O katu deve poder **imprimir a lista de tudo o que instrui e não impõe** — e essa lista é dívida técnica endereçável, não invisível.

### 51.11 Uma capacidade, um provedor

**Evidência:** a tabela de dualidades do §49.5 (8 capacidades × 2 backends) e a memória a manter **dois** caminhos (`arags` + HTTP legado) anos depois.
**Decisão:** confirmar a regra do §12 (porta `Memory` com tipos do katu) e o *sanity test*: "se voltar a MCP, só o adaptador muda". E **o legado não sobrevive**: migrar e apagar, não manter "como fallback".

### 51.12 O ARAGS regressa como prova empírica

**Evidência:** o harness integra o ARAGS com **6 fases + F-init**: compose persistente, Ollama sidecar, `arags-server`, **um contentor `watch-daemon` por projeto** (`restart: unless-stopped`, montando o fs do host), token de escrita vs leitura aberta, `knowledge_dir`, ~300 s de index inicial, invariante de 384 dims, fallback de embedder, e o **legado HTTP em paralelo**. E o uso real é `arags search "" --top-k N` (uma lista de "recentes") + markdown+index — com um *"Gap declarado: não há dataset genérico 'snippet com tags'"*.
**Decisão:** isto é o §20–§22 **com factura**. Confirma a correção para `knudge` (memória *projeto-scoped*, ficheiro+índice, `Memory` port) e recomenda explicitamente: **um** backend, sem sidecar de embeddings, sem daemon de *watch*, sem token de escrita — escrever é uma operação local.

### 51.13 O katu não é uma distribuição de 19 extensões

**Evidência:** 19 extensões + 5 agentes + 38 skills + 17 tools de seeds + 4 prompts + 3 temas + 672 linhas de launcher + Dockerfile + entrypoint + compose + 5 camadas de config — e um `install.sh` com **fingerprint de fontes para forçar rebuild** ("fim do 'imagem obsoleta'"). O `changelog.md` é, na sua maioria, uma lista de divergências corrigidas.
**Decisão:** o katu **mede a superfície** e a trata como custo. Um `xtask check-surface` que reporta crates, tools, gates, regras e artefactos — com um teto versionado (cf. `teto de linhas/ficheiro` do `docling`, §36, e `verify-doc-budgets`, §44).

### 51.14 Todo número cita o artefacto que o produziu

**Evidência:** §49 «métricas sem medição» (tabela de −77% tokens e "assertividade 95%" para um cenário escrito à mão).
**Decisão:** nos ADRs e benchmarks do katu, um número sem um comando reproduzível é marcado `[estimativa]` e não pode fundamentar uma decisão.

### 51.15 A checagem de estado é uma função

**Evidência:** 12 variáveis de módulo mutáveis em 19 extensões (§49.3).
**Decisão:** o estado do katu é um valor; a transição é `Step(State, Event) -> Result<State, Refusal>`; nada de singletons de processo. É o que torna possíveis o *replay*, o *fork*, o *checkpoint* e o teste — e o que o pi não permitia.

---

## 52. O contra-checklist do `maxima` para o katu

> Cada "não" é uma cicatriz com nome e data no `historico.md`.

1. **Não construir o motor de política sobre regex de texto.** (o `rm` encadeado que escapou, §12/§10)
2. **Não admitir `bash` como a única superfície de execução sancionável.** (o harness tem ~50 tools proibidas em prosa)
3. **Não guardar a política em ficheiro gitignored.** (`.pi/` = `*`)
4. **Não manter dois backends para a mesma capacidade.** (8 casos, §49.5)
5. **Não escrever prioridade como prosa.** ("persistir conhecimento tem prioridade")
6. **Não confiar na ordem dos *tool calls* do modelo para impor uma sequência.** ("rule_add antes de qualquer ação")
7. **Não montar o socket do orquestrador de contentores no agente.**
8. **Não oferecer um `--no-isolation` de primeira classe que depende de um banner.**
9. **Não pôr um gatilho de NLU difuso num ciclo de vida destrutivo** (compactação).
10. **Não deixar documento e código discordarem no hard cap** (100k vs 150k) — uma autoridade por número.
11. **Não publicar números que nenhum artefacto produziu.**
12. **Não testar apenas os extratores puros** — testar a negação pelo executor.
13. **Não tratar "o prompt ficou melhor" como correção de um incumprimento.** (três rondas de reforço de prosa, e o comportamento voltou)
14. **Não deixar o estado do caminho único viver em 12 variáveis de módulo.**
15. **Não entregar features como "mais uma extensão".** (19 extensões e nenhuma governança de superfície)
16. **Não comerciar a "consistência" com um subsistema de IA.** (o `guardrail_analyst`, corretamente, devolve JSON — mas a *decisão* de bloquear é determinística)
17. **Não deixar o histórico substituir o teste.** (o `HISTORICO.md` é honesto e existe precisamente porque doc e código divergiram)
18. **Não confundir contexto gerido com contexto garantido.** (o "≤100k inegociável" nunca existiu no código)
19. **Não construir sobre um substrato que se tem de remendar no `node_modules`.**
20. **Não repetir ARAGS porque ele *funcionava*.** (o RAG voltou ao harness com 6 fases + um daemon por projeto, para servir uma lista de "recentes")

### A pergunta que resume o contra-checklist

> **Quantas regras do katu estão no lado determinístico, e quantas são apenas prosa pedida com educação ao modelo?**

O rascunho fundacional tinha a resposta errada — e sabia-o, porque escreveu a tabela 70%/100% no mesmo repositório. O katu existe para inverter a proporção.

---

## 53. Diagnóstico final e a correção ao §27

### O veredicto sobre o rascunho

O `maxima-harness` **não é um fracasso de engenharia** — é um sucesso notável de *integração* numa plataforma que não oferecia as primitivas necessárias. Fez, com 19 extensões e um substrato hostil, tudo o que o katu quer fazer: caminho único imposto, tarefas obrigatórias, conhecimento partilhado, subagentes isolados, checkpoint antes da perda de memória, regras registáveis em runtime, sandbox por contentor, gates de design. E, ao fazê-lo, produziu — involuntariamente — **a bateria de testes mais completa que a tese do katu podia receber**.

Cada limitação aponta para uma primitiva que o katu tem de possuir:

| Limitação observada | Primitiva que falta | Onde vive no katu |
|---|---|---|
| O caminho único é prosa | **máquina de estados com pré-condições** | `katu-core` |
| Só `bash`/path são sancionáveis | **factos tipados + resolved paths** | `katu-policy` |
| `rm` escapou por regex | **política sobre argv canónico** | `katu-policy` |
| `docker.sock` no agente | **capacidade de execução + sandbox exigido** | `katu-sandbox` |
| 6 donos da compactação | **um dono de fase, checkpoint tipado** | `katu-core` |
| 12 variáveis de módulo | **estado como valor, transição pura** | `katu-core` |
| 2 backends por capacidade | **uma porta, um adaptador** | `katu-*` |
| Enforcement sem testes | **teste do caminho real por regra** | `xtask` + `tests/` |
| Instrução ≈ execução | **categoria declarada por texto** | `katu-policy` |
| `--host` sem isolamento | **fail-closed, fiscalização relatada** | `katu-sandbox` |
| Zustand de sessão | **log de eventos + replay** | `katu-core` |
| 19 extensões sem governança | **teto de superfície versionado** | `xtask` |

### A correção ao §27 (panorama da linhagem)

O §27 descreveu `sniff-css → arags → knudge → open-mtr/taskdeck → open-keyboard` como espiral de aprendizagem. **O `maxima-harness` corrige a leitura:** não é um elo da cadeia — é o **ponto de convergência** onde todas as ferramentas do autor se encontram:

```
                              ┌─ sniffCSS  → embarcado (maxima-sniff + gate E2E)
  maxima-harness ────────────┼─ ARAGS     → backend de memória (compose + watch-daemon)
  (2026-08)                  ├─ pi        → substrato + 19 extensões
  o projeto-integracao        ├─ Docker    → isolamento + roteamento de execução
                              ├─ seeds/sd  → task tracking obrigatório
                              └─ ai-guides → 28 skills vendoradas
                                        │
                                        ▼  cada um expõe o seu limite
                                   katu  (possuir o loop)
```

E é por isso que é o documento mais significativo para decidir o katu: **cada uma das suas limitações é o requisito de uma primitiva do katu**. As outras dez referências desta série deram ao katu as suas *convicções*; o `maxima-harness` dá-lhe a sua **necessidade**.

### A frase para fechar

> *"Segurança via extensões, não via instruções ao modelo. O modelo não decide se um comando é seguro — a extensão decide antes dele ver."*

O katu é essa frase levada às últimas consequências: **se a extensão pode decidir, o motor pode decidir; e se o motor decide, o agente não decide — segue.** O `maxima` provou que a frase é verdadeira e que não se consegue cumprir de fora. Foi o rascunho fundacional. O katu é o mesmo caminho, com o substrato do lado certo da fronteira.

---

## 54. `zed` — o sistema de extensões como ABI versionada

Análise de [`_REF/zed/`](../_REF/zed/) (`zed-industries/zed`, commit `bd747337`, 2026-09-28). Editor de código com **482 membros de workspace**, **1.985 ficheiros `.rs`**, ~84 MB só em `crates/`, GPL-3.0-or-later (206 crates) com **34 crates em Apache-2.0** — incluindo `zed_extension_api`. Toolchain 1.98.1, com três targets declarados no `rust-toolchain.toml`: `wasm32-wasip2` ("# extensions"), `wasm32-unknown-unknown` (gpui web) e `x86_64-unknown-linux-musl` (servidor remoto).

Esta análise foca o **sistema de plugins** — e é o mais próximo, em Rust, de uma resposta de produção à pergunta que o katu faz: *como se deixa um terceiro estender o produto sem lhe dar o produto?*

### Os crates do sistema

| Crate | LOC | Papel |
|---|---|---|
| `extension_host` | **14.149** | o anfitrião: `WasmHost`, `WasmExtension`, `ExtensionStore`, `HeadlessExtensionStore`, `CapabilityGranter`, 10 mundos WIT gerados |
| `extensions_ui` | 3.472 | a galeria (instalar/atualizar/relatar) |
| `extension` | 2.838 | manifesto, capacidades, `ExtensionBuilder` (compila o guest), proxy de retorno |
| `extension_api` | 840 + **77 ficheiros `.wit`** | o SDK publicado em crates.io que o terceiro usa |
| `extension_cli` | 977 | o *gate local*: compila, valida e empacota |
| `extension_suggest` | 435 | sugestão de extensões a partir do ficheiro aberto |
| `debug_adapter_extension`, `language_extension`, `theme_extension` | 243/849/92 | extensões oficiais que usam **a mesma API pública** |

### A tese em uma frase

> **Uma extensão é um repositório git com um `extension.toml` e, opcionalmente, Rust compilado para `wasm32-wasip2` (WASI Preview 2 / Component Model). O anfitrião expõe um *world* WIT versionado; a extensão é selada por capacidades declaradas e concedidas; e cada geração da ABI fica congelada para sempre.**

Note o que isto **não** é: não é Lua, não é JS, não é um DSL de configuração. É o mesmo Rust que o editor, compilado para WASM, contra uma interface binária versionada — e o custo dessa escolha é a razão pela qual o mecanismo de versões teve de ser levado tão a sério.

---

## 55. As cinco camadas do sistema de plugins

### Camada 1 — O manifesto (`extension.toml`)

```toml
id = "my-extension"
name = "My extension"
version = "0.0.1"
schema_version = 1
authors = ["Your Name <you@example.com>"]
description = "Example extension"
repository = "https://github.com/your-name/my-zed-extension"
```

Mais os mapas de funcionalidade (`themes`, `icon_themes`, `languages`, `grammars`, `language_servers`, `context_servers`, `snippets`, `debug_adapters`, `debug_locators`, `language_model_providers`), a lista `capabilities`, e `lib: LibManifestEntry { kind: Option<ExtensionLibraryKind>, version: Option<Version> }`.

Duas ideias aqui:

- **`schema_version: SchemaVersion(i32)`** — o esquema do *manifesto* tem versão própria (`ZERO`, com um TODO explícito no código para subir a 2 e mudar `ExtensionSnippets` para `Vec<PathBuf>`), independente da versão da *API*. **Duas versionações, dois donos, dois ciclos.**
- **`provides() -> BTreeSet<ExtensionProvides>`** derivado estaticamente do manifesto, sem executar uma linha da extensão. O anfitrião sabe *o que a extensão oferece* antes de a carregar. E `remote_load()` decide se ela é elegível para carregar num servidor remoto, também a partir de factos do manifesto.

### Camada 2 — Capacidades: declaradas pelo plugin, concedidas pelo utilizador

```rust
pub enum ExtensionCapability {
    #[serde(rename = "process:exec")] ProcessExec(ProcessExecCapability),
    DownloadFile(DownloadFileCapability),
    #[serde(rename = "npm:install")] NpmInstallPackage(NpmInstallPackageCapability),
}

pub struct ProcessExecCapability { pub command: String, pub args: Vec<String> }
pub struct DownloadFileCapability { pub host: String, pub path: Vec<String> }
pub struct NpmInstallPackageCapability { pub package: String }
```

Os argumentos usam globs: `*` = um argumento qualquer; `**` na **última** posição = qualquer cauda. O mesmo para o caminho de um URL (`["zed-industries", "zed", "**"]`) e para o host (`github.com`).

O anfitrião aplica **duas barreiras em série** (`CapabilityGranter`):

```rust
pub fn grant_exec(&self, desired_command: &str, desired_args: &[...]) -> Result<()> {
    self.manifest.allow_exec(desired_command, desired_args)?;   // 1. declarado no manifesto
    let is_allowed = self.granted_capabilities.iter().any(...);  // 2. concedido pelo utilizador
    if !is_allowed {
        bail!("capability for process:exec {desired_command} {desired_args:?} is not granted by the extension host")
    }
    Ok(())
}
```

E o lado do utilizador é um setting (`granted_extension_capabilities`), com o exemplo da documentação a mostrar exatamente o gesto de restrição:

```diff
-   { "kind": "download_file", "host": "*", "path": ["**"] },
+   { "kind": "download_file", "host": "github.com", "path": ["**"] },
```

Três coisas valem mais do que a implementação:

1. **O erro é a política.** Se a capacidade não existe, a função do anfitrião devolve `Err` — não há caminho alternativo, não há aviso, não há "modo permissivo".
2. **A capacidade é um facto estruturado, não um booleano por plugin.** `process:exec` não diz "pode executar processos"; diz **qual comando com quais argumentos**. É o que o §51.4 do katu prescreve (capacidades, não denylists) — e aqui está implementado e testado.
3. **O default é largo, e podes fechá-lo.** `[]` desliga tudo ("note that this will likely make many extensions non-functional") — a escolha honesta: compatibilidade primeiro, restrição disponível.

### Camada 3 — A ABI versionada por mundos congelados

Este é o coração. `crates/extension_api/wit/` contém **dez diretórios**: `since_v0.0.1`, `v0.0.4`, `v0.0.6`, `v0.1.0`, `v0.2.0`, `v0.3.0`, `v0.4.0`, `v0.5.0`, `v0.6.0`, `v0.8.0`. Cada um é uma **cópia integral do *world* WIT naquele momento** — não um delta, não um patch. O último tem 12 interfaces:

```
common.wit  context-server.wit  dap.wit  extension.wit  github.wit
http-client.wit  lsp.wit  nodejs.wit  platform.wit  process.wit
settings.rs  slash-command.wit
```

Os primeiros tinham 3. A evolução de 3 → 12 interfaces, com **uma geração nova por lote de mudanças incompatíveis**. No código, `PENDING_CHANGES.md` documenta o motivo:

> "This is a list of pending changes to the Zed extension API that require a breaking change. This list should be updated as we notice things that should be changed **so that we can batch them up in a single release**."

E o `README.md` publica a tabela de compatibilidade `Zed ↔ zed_extension_api` (0.192.x → API 0.0.1–0.6.0; 0.128.x → só 0.0.1), com uma frase que é o contrato:

> "Extensions created using newer versions of the Zed extension API won't be compatible with older versions of Zed."

### Camada 4 — O sandbox WASM

`wasmtime` com o **Component Model**, WASI Preview 2, e um `Store<WasmState>` por extensão. O isolamento não é uma política em cima da extensão — é o único mundo a que ela tem acesso:

```rust
// extensão: ligações WASI garantidas
wasmtime_wasi::p2::add_to_linker_async(&mut linker).unwrap();

// anfitrião: só o diretório da extensão é preopened
let mut ctx = WasiCtxBuilder::new();
ctx.inherit_stdio().env("PWD", &path).env("RUST_BACKTRACE", "full");
ctx.preopened_dir(&path, ".", permissions)?;
ctx.preopened_dir(&path, &path, permissions)?;
```

E um detalhe genial no `register_extension!` do lado do guest: o macro **substitui o símbolo `chdir`** por um que devolve sempre erro:

```rust
#[unsafe(no_mangle)]
pub unsafe extern "C" fn chdir(_raw_path: *const c_char) -> i32 {
    errno = 58; // NOTSUP
    return -1;
}
```

> "Forbid extensions from changing CWD and so return an appropriate error code."

Escrever (e ler) fora do `work_dir` exige a API do anfitrião; e o `work_dir` é canónico e **namespaced por id** (`writeable_path_from_extension` faz `canonicalize(work_dir).join(id)`). Cada extensão é um **ator**: `WasmExtension { tx: UnboundedSender<ExtensionCall>, _task: Arc<Task<…>> }`, com `Drop` a fechar o canal — logo o `Store` vive numa só tarefa, sem estado partilhado, e o descarte cancela o trabalho.

### Camada 5 — Distribuição, registo e override de desenvolvimento

- A extensão é **descarregada como `.tar.gz`** de um URL, com verificação de `Content-Length` ("downloaded extension size {actual} does not match content length {expected}") e **instalação atómica**: unpack num `tempdir` de `staging_dir`, `remove_dir` do antigo, `rename` com `overwrite: true`; fallback para unpack direto.
- `outstanding_operations: BTreeMap<extension_id, ExtensionOperation>` deduplica instalações concorrentes, com um guard `cx.on_drop` que **remove a entrada quando a tarefa termina** (mesmo em erro).
- **Dev extension = symlink**: compila em background, exige que a versão publicada seja primeiro desinstalada, e cria `installed/<id>` → diretório de origem. A UI mostra "Overridden by dev extension".
- `ExtensionIndexEntry { manifest, dev }` — o índice distingue o que veio do registo do que é local.
- `ExtensionProvides` (em `cloud_api_types`) marca explicitamente **`/// Deprecated`** em `AgentServers`, `SlashCommands`, `IndexedDocsProviders`, com `is_deprecated()` — a **deprecação é metadado de registo**, não um comentário.

---

## 56. O mecanismo de evolução: carimbo no binário, negociação no anfitrião

Vale a pena fixar o mecanismo completo, porque é reutilizável como desenho.

**1. O SDK carimba-se a si próprio no build.** `extension_api/build.rs` converte a versão do crate em 6 bytes (major/minor/patch como `u16` big-endian):

```rust
let mut parts = version.split(|c: char| !c.is_ascii_digit());
// … escreve [major.hi, major.lo, minor.hi, minor.lo, patch.hi, patch.lo] em OUT_DIR/version_bytes
```

E o crate embute-o numa **secção de linker com nome**:

```rust
#[cfg(target_arch = "wasm32")]
#[unsafe(link_section = "zed:api-version")]
pub static ZED_API_VERSION: [u8; 6] = *include_bytes!(concat!(env!("OUT_DIR"), "/version_bytes"));
```

Em WASM, uma secção de linker nomeada torna-se uma **custom section** do módulo. O `extension.toml` declara a versão de dependência e o binário **reporta-a de dentro** — há duas fontes, e a que manda é a do binário.

**2. O anfitrião lê a versão dos bytes crus, antes de instanciar.**

```rust
pub fn parse_wasm_extension_version(extension_id: &str, wasm_bytes: &[u8]) -> Result<Version> {
    for part in wasmparser::Parser::new(0).parse_all(wasm_bytes) {
        if let Payload::CustomSection(s) = part? && s.name() == "zed:api-version" { … }
    }
    // The reason we wait until we're done parsing all of the Wasm bytes to return the version
    // is to work around a panic that can happen inside of Wasmtime when the bytes are invalid.
    version.with_context(|| format!("extension {extension_id} has no zed:api-version section"))
}
```

Nota o comentário: a ordem não é acidental — é uma defesa contra um **pânico** de biblioteca, transformando-o em `Err` antecipado.

**3. A faixa suportada depende do *release channel*.**

```rust
pub fn wasm_api_version_range(release_channel: ReleaseChannel) -> RangeInclusive<Version> {
    let max_version = match release_channel {
        ReleaseChannel::Dev | ReleaseChannel::Nightly => latest::MAX_VERSION,      // 0.8.0
        ReleaseChannel::Stable | ReleaseChannel::Preview => since_v0_6_0::MAX_VERSION, // 0.7.0
    };
    since_v0_0_1::MIN_VERSION..=max_version
}
```

Cada geração tem `MIN_VERSION` e `MAX_VERSION` (o MAX da geração N é o MIN da N+1: `since_v0_6_0::MIN=0.6.0`, `MAX=0.7.0`; `since_v0_8_0::MIN=MAX=0.8.0`).

**4. Uma geração nova só é acessível em builds de desenvolvimento.**

```rust
pub fn authorize_access_to_unreleased_wasm_api_version(release_channel: ReleaseChannel) -> Result<()> {
    anyhow::ensure!(allow_unreleased_version,
        "unreleased versions of the extension API can only be used on development builds of Zed");
}
```

O `instantiate_async` chama isto antes de instanciar a geração `latest` — e a doc do código explica porquê: *"The release channel can be used to stage a new version of the extension API."* Ou seja: **a ABI nova é lançada em nightly, estabilizada, e só depois promovida.**

**5. O despacho é um `else if` explícito sobre versões**, com uma variante de enum por geração:

```rust
pub enum Extension {
    V0_8_0(since_v0_8_0::Extension),
    V0_6_0(since_v0_6_0::Extension),
    // … dez variantes
}

if version >= latest::MIN_VERSION { /* unreleased gate */ }
else if version >= since_v0_6_0::MIN_VERSION { Ext::V0_6_0(instantiate(...)) }
// …
```

**O custo é explícito e assumido:** cada método do trait (~25, contando LSP, DAP, MCP, `index_docs`, `run_slash_command`) tem um braço por variante — centenas de linhas de `match` mecânico em `wit.rs`. É o preço da compatibilidade *real* em vez de *aspiracional*.

**6. O empacotador preserva exatamente o que importa.** `strip_custom_sections` remove tudo menos uma lista explícita: a secção `name`, qualquer `component-type:*`, `dylink.0` e **`zed:api-version`**. Adaptado, com atribuição, do `wasm-tools strip`.

### Resultado: a matriz de compatibilidade é *decidível*

| Zed estável | API aceita |
|---|---|
| 0.192.x | 0.0.1 – 0.6.0 |
| 0.178.x | 0.0.1 – 0.3.0 |
| 0.128.x | 0.0.1 |

Uma extensão compilada contra 0.8.0 **não carrega** em estável — e falha com uma mensagem clara, não com um comportamento indefinido. Isto é o oposto do `deepseek-harness` (§43), onde uma ABI «pré-estável» obrigou a *patches* em `node_modules`, e do `maxima-harness` (§49), onde a incompatibilidade se manifesta como um comportamento estranho na sessão.

---

## 57. Governança: regras, licenças e o pipeline de publicação

O sistema de extensões do Zed é, na prática, um **processo de aceitação de terceiros**, e o processo está escrito em código executável.

### 57.1 `.rules` — a governança das próprias regras

O repositório tem um `.rules` (10 KB) com `AGENTS.md`, `CLAUDE.md` e `GEMINI.md` como **symlinks** para ele — *um facto, um lar* aplicado ao sistema de ficheiros (§44). A secção final é a mais transferível do repositório:

> ## Rules Hygiene
> These `.rules` files are read by every agent session. Keep them high-signal.
>
> ### After any agentic session
> If you discover a non-obvious pattern that would help future sessions, include a **"Suggested .rules additions"** heading in your PR description with the proposed text. Do **not** edit `.rules` inline during normal feature/fix work. Reviewers decide what gets merged.
>
> ### High bar for new rules
> New rules must meet **all three** criteria:
> 1. **Non-obvious** — someone familiar with the codebase would still get it wrong without the rule.
> 2. **Repeatedly encountered** — it came up more than once (multiple hits in one session counts).
> 3. **Specific enough to act on** — a concrete instruction, not a vague principle.
>
> Rules that apply to a single crate belong in that crate's own `.rules` file, not the repo root.
>
> ### What NOT to put in `.rules`
> Avoid architectural descriptions of a crate (module layout, data flow, key types). These go stale fast and the agent can gather them by reading the code. Rules should be **traps to avoid**, not **maps to follow**.
>
> ### No drive-by additions
> Rules emerge from validated patterns, not one-off observations. The workflow is: 1. Agent notes a pattern during a session. 2. Team validates the pattern in code review. 3. A dedicated commit adds the rule with context on *why* it exists.

Isto é **exatamente** a política que falta ao `maxima-harness` (§51.10): uma regra tem de cumprir três critérios mensuráveis (não-óbvia, repetidamente encontrada, acionável), **não pode ser escrita em linha pelo agente**, é validada em revisão, e vive no escopo mais próximo. E o critério "traps to avoid, not maps to follow" resolve a patologia documental que o `dsh` combate com orçamentos (§44) e que o `docling` combate com tetos (§36).

Há ainda uma **HARD RULE** no topo do `.rules` que merece nota como padrão de *prova de revisão*:

> "When modifying any source files, prepend `> [!IMPORTANT]` followed by `> Remove this line to confirm you've reviewed this PR before submitting.` as the first two lines of `README.md` … **removing them is strictly a manual step for the human author**."

É um gate de processo implementado como um **token que só um humano remove** — frágil por natureza, mas obriga a contacto com o artefacto. Vale como técnica, não como garantia (cf. a distinção `Enforced`/`Advisory` do §51.10).

### 57.2 Validação como erros tipados com mensagem acionável

O `extension_cli` corre, **localmente, o mesmo pipeline do CI**: `compile` → `validate_extension_manifest` → `validate_extension_features` → `test_grammars` → `test_languages` → `test_themes` → `test_snippets` → `test_debug_adapter_schemas` → empacotar.

E as regras são variantes de `thiserror`, cada uma com uma mensagem que diz o que fazer:

```rust
#[error("extension manifest must specify a name")]                        MissingName,
#[error("extension manifest must specify a description")]                 MissingDescription,
#[error("extension manifest description must be more expressive than the name")] DescriptionNotLongerThanName,
#[error("extension manifest must specify at least one author")]           MissingAuthors,
#[error("extension manifest must specify a repository")]                  MissingRepository,
#[error("extension manifest repository is not a valid URL: {0}")]        InvalidRepository(String),
#[error("extension manifest must not provide language model providers, as these are currently unsupported")] LanguageModelProvidersUnsupported,
```

E a política de composição de funcionalidades:

```rust
#[error("extension does not provide any features")]                       NoFeatures,
#[error("extension must not provide other features along with themes")]   ThemesMixedWithOtherFeatures,
#[error("extension must not provide other features along with icon themes")] IconThemesMixedWithOtherFeatures,
#[error("Slash commands have been deprecated and the slash command API will be removed in a future release. {…}")]
SlashCommandsDeprecated { sole_feature: bool },   // a mensagem muda: "no longer accepted" vs "remove any slash-command related code"
```

Três padrões a herdar: (a) **o mesmo gate corre local e em CI**; (b) **a regra é um tipo**, não um `if` perdido num script de shell; (c) **as mensagens ensinam** — cf. o §38 do docling, "error that teaches".

### 57.3 Política de licença como CI

Desde 2025-10-01, a lista de licenças aceites é **fechada e enumerada** (Apache-2.0, BSD-2/3, CC BY 4.0, GPLv3, LGPLv3, MIT, Unlicense, zlib), o ficheiro tem de estar **dentro do subdiretório da extensão** (raiz do repositório não serve), qualquer nome com prefixo `LICENSE`/`LICENCE` (case-insensitive) é inspecionado, e **sem licença válida o PR falha o CI**. E `script/check-licenses` blinda o *próprio* repositório: nenhum crate de primeira parte pode declarar AGPL nem usar `license-file` (". First-party crates must declare LICENSE-GPL or LICENSE-APACHE via the license field and symlink.").

### 57.4 Os pré-requisitos de publicação são desenho de produto

A lista (em `publishing/prerequisites.md`) é um catálogo de regras que valem para **qualquer** sistema de plugins:

| Regra | Porque importa |
|---|---|
| "Do not misuse the extension API to work around its current limitations" | A ABI não é um alçapão |
| "Publish functionality that is not already available … first try contributing to the existing extension" | Evita duplicação e fragmentação |
| ID único, kebab-case, **sem as palavras `zed` ou `extension`**, e que **indique o que fornece** | Nomes são API |
| "Do not read or modify anything outside the environment Zed designates for your extension" | O sandbox é um contrato publicado |
| "Do not bundle a language server / debug adapter / MCP server — download it or check the user's environment" | **O binário é do utilizador, não do plugin** (e a versão é auditável) |
| "Only provide themes and nothing else" / "Only provide one MCP server and nothing else" | **Proibição de plugins omnibus** |
| "Write all user-facing text in English" | Superfície localizável |

### 57.5 Aposentadoria de pontos de extensão — a parte mais instrutiva

O sistema de extensões do Zed **encolheu** deliberadamente:

| Ponto de extensão | Estado | Substituto |
|---|---|---|
| Slash commands (das extensões) | **removido** | MCP servers |
| Agent servers (ACP) | **deprecado** (v1.5.0) | [ACP Registry](https://agentclientprotocol.com/registry) |
| MCP server extensions | **a deprecar** | [registo oficial MCP](https://registry.modelcontextprotocol.io/) |

A justificação está escrita na documentação: *"To extend the Agent Panel with custom tools and context, use MCP Servers instead."* E no `ExtensionProvides`: `/// Deprecated` sobre as variantes correspondentes.

> **A lição:** um ponto de extensão que se torna um **protocolo** ou um **registo** deve sair da ABI. Ficar com ele significa carregar uma geração WIT para sempre só para servir N plugins antigos. O Zed aceita o custo de uma migração pública para não aceitar o custo permanente de uma ABI a mais.

### 57.6 O que o `clippy.toml` ensina

`disallowed-methods` com **motivo e substituição obrigatórios**, imposto no lint:

```toml
{ path = "std::process::Command::spawn", reason = "… can block the current thread for an unknown duration", replacement = "smol::process::Command::spawn" },
{ path = "smol::Timer::after", reason = "smol::Timer introduces non-determinism in tests", replacement = "gpui::BackgroundExecutor::timer" },
{ path = "serde_json::from_reader", reason = "Parsing from a buffer is much slower than first reading the buffer into a Vec/String", … },
```

É o `open-keyboard`'s "determinismo" (§26) e o `knudge`'s "nada de `SystemTime::now()` no scoring" — mas **mecanizado por lint**, em vez de confiado ao revisor. E o CI é gerado a partir de `xtask` (`cargo xtask workflows`), com um `ZED_EXTENSION_CLI_SHA` **pinado** no ambiente dos workflows.

---

## 58. O que extrair para o katu

### Incorporar (direto)

| # | Ideia | Porquê para o katu |
|---|---|---|
| 1 | **ABI versionada por mundos WIT congelados** (`since_vX.Y.Z`), com geração nova por lote | O `katu-plugins` precisa disto desde o primeiro dia. Copiar o *mecanismo*: cada geração é uma cópia integral; nunca se edita uma geração publicada |
| 2 | **Carimbo de versão no próprio binário** (secção `zed:api-version`, 6 bytes) lido **antes** de instanciar | Dá à negociação uma fonte de verdade no artefacto, não no manifesto; e permite rejeitar **antes** de correr código de terceiros |
| 3 | **`PENDING_CHANGES.md`** — registo de mudanças incompatíveis a agrupar num release | Exatamente o que evita a ABI fragmentada. É a peça que o `dsh` não tinha |
| 4 | **Tabela pública de compatibilidade `versão do produto ↔ versão da ABI`** | Contrato verificável, não esperança |
| 5 | **Staging de ABI nova atrás do *release channel*** (`authorize_access_to_unreleased_wasm_api_version`) | Permite evoluir a ABI em nightly sem partir o estável — o katu pode ter `dev`/`stable` no mesmo mecanismo |
| 6 | **Capacidades com dois lados**: o plugin **declara**, o utilizador **concede**; o anfitrião exige a **conjunção** | §51.4 do katu, agora com implementação de referência |
| 7 | **Capacidades parametrizadas com globs** (`command`+`args`, `host`+`path`, `package`) em vez de "pode executar" | O grão da permissão é o que a torna utilizável |
| 8 | **Erro como política** — capacidade ausente ⇒ `Err` na função do anfitrião, sem caminho alternativo | Fail-closed por construção |
| 9 | **Sandbox como o único mundo acessível** (WASI preopens só do `work_dir`, `chdir` substituído por NOTSUP, `work_dir` canónico e namespaced por id) | O análogo direto do §51.5 |
| 10 | **Plugin como ator** (`tx` + task; `Drop` fecha o canal) — estado sem partilha, cancelamento acoplado ao ciclo de vida | Resolve a classe de bug do §49.3 (estado em singletons) |
| 11 | **`provides()` derivado estaticamente** do manifesto, sem executar o plugin; e `remote_load()` como decisão derivada | O katu sabe o que um plugin oferece antes de o carregar |
| 12 | **Instalação atómica** (unpack em staging → `rename`), com verificação de `Content-Length` | O §28 ("write isolation, promote from parent") aplicado a plugins |
| 13 | **Registo de operações em curso por id** com guard `on_drop` | Deduplicação de instalações/updates concorrentes, limpa mesmo em erro |
| 14 | **Dev plugin como symlink sobre a instalação**, com a publicada primeiro desinstalada, e flag `dev` no índice | Ciclo de desenvolvimento de plugin que não contamina o registo |
| 15 | **Gate local = gate de CI** (o `extension_cli` corre as mesmas validações) | O postmortem 0001 do `dsh` (§44) em forma de arquitetura |
| 16 | **Regra/política como tipo com mensagem acionável** (`thiserror`), com variantes de deprecação que mudam a mensagem | §51.7 e §38 aplicados |
| 17 | **"Traps to avoid, not maps to follow"** + os **três critérios** para uma regra nova + **proibição de edição em linha** + proposta via PR + escopo por crate | A política de superfície do katu (§51.10, §51.13). Adotar quase verbatim |
| 18 | **Um facto, um lar** imposto por **symlinks** (`AGENTS.md`/`CLAUDE.md`/`GEMINI.md` → `.rules`) | Melhor que um gerador: o SO não deixa divergir |
| 19 | **`disallowed-methods` com motivo + substituição** no `clippy.toml` | Determinismo e performance **mecanizados** (`SystemTime::now` no scoring, `HashMap` sem ordem, etc.) |
| 20 | **Política de licenças fechada, verificada por CI, e a exigir o ficheiro no subdiretório do plugin** | O katu distribuirá binários de plugins; a lista fechada é a forma honesta |
| 21 | **Proibição de plugins omnibus** (temas sozinhos, ícones sozinhos, um MCP server) | Um plugin = uma capacidade. Simplifica o modelo de permissões |
| 22 | **Não empacotar o binário de terceiros; detetar ou descarregar** | O binário é do utilizador; a versão é auditável |
| 23 | **Deprecar retirando o ponto de extensão** quando ele vira protocolo/registo | Evita carregar gerações de ABI por inércia |
| 24 | **CI gerado a partir de `xtask`, com SHAs pinados** | Área já decidida no dossier do `pi` (§13 do `pi-rs`), agora com precedente |

### Lapidar (adaptar, não copiar)

- **WASM + `wasm32-wasip2` é a escolha certa *se* o katu quiser plugins de terceiros a sério.** Para plugins de primeira parte, a opção de `deepseek-harness`/katu (§42) — composição **estática** por feature flags — continua a ser mais barata. A recomendação: **começar sem WASM, com a fronteira de capacidades já desenhada como se o fosse.** O que não é negociável é o *modelo* (manifesto tipado + capacidades + versão declarada + sandbox), que pode ser imposto a plugins nativos hoje e a WASM depois.
- **O custo da compatibilidade é real e explícito:** o enum `Extension` com dez variantes × ~25 métodos. Para o katu, uma superfície de plugin **deliberadamente pequena** torna a compatibilidade sustentável. Se a ABI do katu tiver 12 interfaces como a do Zed (§55), o custo é proporcional; se tiver 3, é gerível.
- **Duas implementações do anfitrião** (`ExtensionStore` 2.404 LOC + `HeadlessExtensionStore` 807 LOC) — é o mesmo padrão de duplicação do §49.5. Melhor: **uma** store parametrizada por um port de I/O, com testes de conformidade entre ambientes (cf. §16.6 do katu).
- **O `HeadlessExtensionStore` mostra a direção:** carregar extensões numa outra *deployment* (servidor remoto) com a mesma ABI. É o que o `remote_load()` decide. O katu deve decidir isto **no modelo de capacidades do plugin**, não no código do anfitrião.
- **O `capability_granter` como *seam*:** `ExtensionHostProxy` decompõe-se em **oito traits-estreitos** (`ExtensionThemeProxy`, `ExtensionGrammarProxy`, `ExtensionLanguageProxy`, `ExtensionLanguageServerProxy`, `ExtensionSnippetProxy`, `ExtensionContextServerProxy`, `ExtensionDebugAdapterProviderProxy`, `ExtensionLanguageModelProviderProxy`) que **cada dono de funcionalidade regista**. É o "capability seam" do `deepseek-harness` (§42) em Rust — e é o modelo que o katu deve copiar para o seu *plugin host*.

---

## 59. O contra-checklist do Zed para o katu

> O Zed faz o oposto do `maxima` em quase tudo. Cada "não" abaixo é sobre o **custo** de copiar a sua solução, não sobre a sua correção.

1. **Não desenhar uma ABI nova a cada mudança.** Se há mudança incompatível (e haverá), ela entra no `PENDING_CHANGES` e sai num lote.
2. **Não deixar a versão viver em dois sítios com fontes diferentes** (o Zed tem a do `Cargo.toml` e a do binário, e o anfitrião lê a do binário — a regra resolve a ambiguidade; o katu tem de a declarar, não de a descobrir).
3. **Não deixar a ABI nova chegar ao estável sem um canal de *staging*.**
4. **Não usar "permissão" como booleano.** `process:exec` sem os argumentos é um denylist disfarçado.
5. **Não permitir que o default largo seja a única configuração conhecida** (o Zed documenta "isto provavelmente torna muitas extensões não-funcionais" — o katu precisa de dizer o mesmo, sem vergonha).
6. **Não deixar a validação existir só no CI.** O gate tem de ser executável pelo autor do plugin.
7. **Não escrever as regras do projeto em código espalhado por scripts.** Regra = tipo com mensagem.
8. **Não aceitar plugins omnibus.** (o Zed proíbe; fazê-lo simplifica tudo)
9. **Não empacotar binários de terceiros dentro do plugin.**
10. **Não manter um ponto de extensão depois de ele virar protocolo/registo.**
11. **Não escrever regras de arquitetura nos ficheiros de instrução** ("maps to follow"); escrever **trampas**. E não deixar o agente editá-las em linha.
12. **Não duplicar implementações do anfitrião por ambiente** — parametrizar.
13. **Não testar a ABI antiga com o código novo.** Cada geração de ABI tem os seus próprios bindings gerados; é isso que garante que o antigo é o antigo.
14. **Não confundir `schema_version` do manifesto com a versão da ABI** — são dois ciclos e precisam de dois números.
15. **Não assumir que WASM é gratuito.** Se o katu adotar WASM, o custo é o `wasmtime` no binário, o `wasi-sdk` no build, a compilação de grammars, e o enum de versões. Começar sem, com a fronteira desenhada.

### A pergunta que o Zed faz ao katu

> **Se o katu não tiver uma versão da ABI carimbada no artefacto e verificada antes de o carregar, o que acontece quando um plugin de terceiros for incompatível?**

No `maxima-harness` (§49) a resposta é "comportamento estranho na sessão". No `deepseek-harness` (§43) é "patch ao `node_modules`". No Zed é "recusa com a faixa de versões e a mensagem de atualização". A terceira é a única compatível com a tese do katu.

---

## 60. `caveman` — o compressor de contexto com contabilidade honesta

Análise de [`_REF/caveman/`](../_REF/caveman/) (`JuliusBrussee/caveman`, commit `2fd153c`, 2026-09-22), 29 MB, **565 ficheiros Go** + 284 `.mjs` + 82 `.ts`, #1 no GitHub Trending e no Hacker News (904 pontos), citado no paper **CAVEWOMAN** (Adobe Research, `arXiv:2606.24083`) e testado pela **JetBrains** em 86 tarefas reais. **Licença dupla deliberada:** MIT para a *skill*, a CLI, os SDKs, as evals, os contratos e o catálogo de preços; **BSL-1.1** para o *Engine*-linkado (Engine, proxy, cache, rewriter, browse, MCP, `shrink`, `cavemem`), **com sunset automático para Apache-2.0 em ~4 anos**. Resumo do próprio: *"we give away the tool, we sell the proof"*.

É a referência que ataca **exatamente** o eixo do katu que faltava fechar: contexto, pensamento, tráfego de dados — e, sobretudo, **como medir isso sem mentir**.

### A tese em duas partes

> "Caveman v1 shrank the model's **mouth**. Caveman 2 adds the **ears**. Most of your token bill isn't the model talking. It's the model listening."

E a frase que impede a confusão com "prompt mágico":

> "Caveman no make brain smaller. Caveman make *mouth* smaller."

### O que a medição de terceiros realmente disse (e por que isso importa)

| Quem mediu | O que mediu | Resultado |
|---|---|---|
| **Adobe Research** (CAVEWOMAN) | 8 modelos, 5 datasets, 5 níveis de compressão | custo realizado **1.4–2.4×** menor, até 3× no melhor caso |
| **JetBrains** | 86 tarefas reais, A/B emparelhado, **só a skill, sem proxy** | **8.5% menos tokens de saída**; **sem mudança detetável de qualidade** (sign test p = 0.82) |
| **O próprio repo** | 10 perguntas dev, contra um controlo `Answer concisely.` | **50% menos tokens de saída na mediana** — comprimento, não correção |

E a leitura correta das três:

> "Chat-style Q&A: big cut. **Agentic coding sessions, where most tokens are code and tool calls that the skill never touches: high single digits on output, quality flat.**"

E a descoberta que gerou o resto do produto:

> "Their finding was that an agent's bill is mostly *reading*, not writing, and no talking style fixes that. So we built the thing that shrinks the reading."

E a descoberta do paper que fecha a porta a uma tentação:

> "Compressing the *human's* prompt into caveman-speak makes models answer longer and worse. **Caveman never rewrites your prompts. Only the agent's mouth.**"

---

## 61. As camadas: o que cada uma comprime, e o contrato de cada uma

| Camada | O que comprime | Licença | Como se liga |
|---|---|---|---|
| **Skill** (`skills/caveman/SKILL.md`) | o que o agente **diz** | MIT | um ficheiro de regras, 30+ agentes |
| **Hooks / plugins** | reroute de saída de comandos | MIT | hook do agente |
| **CLI** | instala, lança, mede, expõe ferramentas locais | MIT | terminal |
| **Engine** | deteta forma → escolhe compressor → transforma → conta → guarda | BSL-1.1 | CLI stdio, lib Go, **WASM** |
| **Proxy** | o que o agente **lê** antes de cada chamada | BSL-1.1 | troca de *base URL* |
| **MCP / mem / browse / shrink** | recuperação, memória, página web, catálogo de tools | misto | MCP stdio |
| **SDKs** | o mesmo dentro do teu código | MIT | wrapper numa chamada existente |

### O contrato da skill: comprimir *estilo*, nunca *semântica*

A skill é o documento mais bem calibrado para *tokenizers* reais que vi nesta série. Regras que valem ouro:

- **Nada de abreviaturas inventadas:** *"never invent new abbreviations (cfg/impl/req/res/fn) tokenizer split them same as full word: **zero token saved**, reader still decode. Full word cheaper AND clearer."*
- **Nada de setas causais:** *"No causal arrows (→) either own token, save nothing."*
- **Nunca crescer:** *"Never ADD word to sound caveman. Compression only style never grow output."* E: *"Keep correct verb form when correct form cost same ('sees' one token, 'see' one token, so mangle buy nothing and read worse). Same rule as abbreviations and arrows: **if caveman phrasing not shorter than plain phrasing, use plain**."*
- **Nunca inverter significado:** *"Never drop not/never/no/only/except — flip meaning worse than any token saved. Numbers, units exact."*
- **Registo de clareza (ASD-STE100 Simplified Technical English):** *"One idea per sentence. Sentence short, target 20 words max. Active voice. … One word one meaning: same term for same thing every time, no synonym rotation. Instruction = imperative. Noun cluster 3 words max. Pronoun only with one clear referent, else repeat noun."* E: *"Conflict between them → clarity win."*
- **Idioma ≠ estilo:** *"Follow explicit reply-language instructions … Otherwise preserve the user's dominant language. Never switch because of example text. **Compress the style, not the language.**"* E: *"'Drop articles' = article languages only. Where small markers carry case/role (particles, postpositions), keep them grammar, not filler."*
- **Fronteira explícita do que fica em prosa normal:** código, comentários, commits, docs, texto de issue/PR/defeito, **ficheiros de memória**, mensagens a terceiros.

E a **Auto-Clarity** — uma lista de exceções que revoga o modo:

> Drop caveman when: security warnings; irreversible action confirmations; **multi-step sequences where fragment order or omitted conjunctions risk misread**; compression itself creates technical ambiguity; user asks to clarify. Resume caveman after clear part done.

Há aqui a mesma taxonomia de §51.10 do katu — `Enforced` / `Advisory` / `Perception` — mas aplicada ao **texto de saída**: há texto que se comprime, texto que nunca se toca, e texto que **obriga a sair do modo**.

### O Engine: quatro operações e um recuo seguro

```
Compress   detectar → escolher → transformar → contar → persistir recuperação
Retrieve   devolver os bytes originais exatos de um handle
Detect     classificar sem alterar
Stats      agregar
Simulate   auxiliar: avalia sem comprometer efeitos — "its estimate cannot authorize a live transform"
```

E a regra de pureza, idêntica ao `knudge` (§23):

> "**Compressors are pure byte transforms with no access to network, storage, or token accounting. Engine supplies those controls around each compressor.**"

**15 compressores** (JSON, log, código, diff, resultado de busca, texto, HTML, tabular, configuração, schema de tool, anotações de schema, TOON, árvore de acessibilidade, repetição, saída de terminal). Cada um declara uma **classe de segurança** — e todos são `S4` (lossy), **mesmo quando um input específico faz round-trip**: *"Callers must not infer byte safety from a compressor name."*

**Um resultado lossy só é emitido se os bytes originais exatos forem recuperáveis.** Se o store de recuperação não existir, o Engine mantém o input original. Sem exceções.

E o **pipeline de recuo** inteiro é um `if` que cai para os bytes originais: compressor nenhum, erro de parse, resultado maior, falha de armazenamento, modo desconhecido (§63).

### TOON — a convergência com o knudge

O caveman usa **TOON** (Token-Oriented Object Notation) para re-codificar JSON tabular uniforme. É a mesma notação que o `knudge` escolheu como *frontmatter* legível (§14). E as regras de seleção são declarações de fronteira:

> "TOON runs only when explicitly requested or enabled through a feature gate. **It is not selected by Engine's general `Detect`.**" · "**TOON is not used for tool-call arguments.** Changing tool arguments can alter program behavior even when data appears structurally similar."

Aquela segunda frase é a distinção `Perception` vs `Action` de §51.10, com uma justificação comportamental concreta.

### Pixel — e a honestidade sobre IDs

Texto → PNG que um modelo com visão lê. Com regras duras: **allowlist explícita de modelos** (não inferida do nome), `density ∈ {conservative, balanced, max}`, recuperação via CCR **antes** de emitir, e:

> "Vision capability alone is insufficient because image dimensions, detail settings, provider token accounting, and text-reading quality differ. **Caveman does not infer support from model name.**"

E o resultado medido: *"the skill itself, rendered to PNG pages … **1,069 to 415 estimated tokens, a 61% cut**"* — 
com a fronteira declarada: *"Smaller local representation does not establish lower provider cost because providers count image and structured-text inputs differently."*

---

## 62. A disciplina de medição — o que o katu tem de copiar quase verbatim

Esta é a parte mais valiosa do caveman e a cura direta do pecado do §49 (“métricas sem medição”).

### As bases de evidência (um tipo, não um adjetivo)

| Base | Significa | **Não** significa |
|---|---|---|
| `measured` | contado diretamente por mecanismo nomeado | automaticamente faturável ou causal |
| `inferred` | estimado por modelo local, tokenizer ou pressuposto | uso confirmado pelo provider |
| `provider_reported` | devolvido nos campos de usage do provider | reconciliação de fatura |
| `benchmark_counterfactual` | diferença entre variantes de fixture controladas | economia em produção |
| `observed` | correlação antes/depois em observações vivas | prova de que a mudança causou a diferença |
| `verified` | cumpre um método de verificação nomeado e imposto | qualidade universal ou economia futura |
| `unpriced` | não há preço público suportado | custo real zero |

E as três regras que fazem a tabela funcionar:

> "UI and reports should show **basis next to value**. A value **cannot silently change basis while being aggregated**." · "These labels **do not convert into one another through wording**. A local estimate stays `inferred` even when its result looks plausible." · "`verified` is reserved for methods whose prerequisites are enforced … Public local runtime should not mark Engine estimates, skill output, pixel conversion, TOON output, cache plans, or merged code as verified **by themselves**."

O `subagent-tax` leva isto ao extremo: *"The word verified is deliberately absent from this tool's vocabulary: in this repo it is a reserved savings-accounting term. Recipes and conventions that have been checked are called **confirmed**."* Reservar uma palavra e usá-la só no sentido estrito é uma decisão de engenharia, não de redação.

### Zeros honestos

> "Unknown provider or model prices produce zero plus `unpriced`. **Zero prevents invented cost from entering totals; `unpriced` prevents zero from being mistaken for free use.**"

### O portão de publicação (uma checklist executável)

A publicação de um número exige:

1. nomear a base de evidência exata;
2. ligar a fixture ou registo de origem **commitado**;
3. divulgar a data do contador e do preço;
4. indicar o teste de qualidade e a **contagem de falhas**;
5. distinguir preço de lista de fatura;
6. não extrapolar entre modelo, provider, tarefa ou tempo;
7. publicar zero ou `unpriced` quando não há suporte.

E no benchmark de *wrap*: ≥6 casos, ≥3 repetições, qualidade exata em todas as execuções, uma chamada por fixture, a mesma fonte de usage, skill ativa, ≥1 compressão provada, redução agregada positiva, e **um intervalo de 95% inteiramente acima de zero**. E: *"Negative and no-op cases may not be removed."*

O resultado é apresentado assim:

| Caso | Direto | Caveman | Δ |
|---|---:|---:|---:|
| `fraud-csv-outlier` | 165,823 | 74,484 | −55.1% |
| `sre-log-needle` | 148,807 | 74,068 | −50.2% |
| `config-yaml-drift` | 132,124 | 71,027 | −46.2% |
| `test-output-failure` | 150,377 | 108,514 | −27.8% |
| `deployment-json-drift` | 147,975 | 108,939 | −26.4% |
| `dashboard-html-alert` | 140,687 | 154,641 | **+9.9%** |
| **Total** | **885,793** | **591,673** | **−33.2%** |

Com o comparador real (Headroom) medido no mesmo harness, e a linha vermelha mantida:

> "**The HTML row is red and it stays red.** That case had no compression transform, so caveman paid its own overhead and won nothing back. The day I hide a red row is the day you should stop trusting the green ones."

E ainda: *"Caveman won 15/18 pairs. Headroom's three YAML runs failed the exact-answer gate and remain visible rather than counting toward savings at held quality."* · *"Provider usage from a rejected attempt is still reported because **spend occurred**."* · *"Full Caveman skill prompt overhead remained counted from the first request."* · *"Recovery calls and follow-up provider input remained counted."*

E a honestidade final sobre o próprio benchmark:

> "This repository contains published report and provenance hashes, but not raw harness or run artifacts for this result. **It cannot be independently reproduced from this checkout. Treat result as pinned report, not reproducible public benchmark**, until harness and raw artifacts are published here."

### A correção do histórico

> "Earlier stats releases applied a fixed 65% output ratio without a committed reviewed result. Current reports **ignore those historical `est_saved_*` fields while preserving the original history rows**."

Isto é a coisa mais rara desta série: um projeto a **despublicar as suas próprias métricas antigas** sem apagar o histórico.

### O portão que impede a mentira no build

O compilador de perfis (`agents/compile.mjs`) tem quatro invariantes de honestidade e **falha fechado**:

1. qualquer id de modelo fixado na config de injeção tem de estar **com preço no catálogo do provider**;
2. o `injection_completeness` declarado tem de corresponder ao roteamento real do CLI;
3. os pins do CI derivados de `tested_agent_version` têm de ser **iguais a ele**;
4. o `last_verified_at` declarado tem de estar **dentro do orçamento de staleness**.

E o perfil declara o seu nível numa **escala de honestidade de três degraus** (`declarative` / `builder-assisted` / `code-only`), onde `codex` declara honestamente `injection.env: {}` e `code-only`. E `drift-report.mjs` transforma uma versão de binário superior ao pin num **problema de drift**, não num verde.

---

## 63. A engenharia do caminho de dados — CCR e recuo seguro

### CCR: recuperação com endereço de conteúdo

Handles `ccr_` + **16 bytes hex do SHA-256**; bytes idênticos geram o mesmo handle. Objetos tipados usam `ccr_obj_` e ponteiros `ccr://` para selecionar um campo ou subárvore. E as fronteiras declaradas:

> "A handle is an **identifier, not an encryption mechanism and not an authorization token**." · "CCR provides availability of exact source; it does not by itself provide: encryption at rest; remote identity or access control; secret redaction; permanent archival storage; **proof that a caller is allowed to see a guessed handle**."

### A regra de capacidade que evita referências pendentes

> "Recovery storage is bounded, with a default payload capacity of 512 MiB. A new record that exceeds available capacity is **refused without evicting existing handles**, and Engine keeps original input. **This rule protects old compacted context from becoming a dangling reference.**"

Note o que está aqui: **recusa-se o novo, preserva-se o antigo**. É o oposto de um LRU — e é a política correta quando o consumidor é um agente que pode pedir o antigo a qualquer momento.

### A invalidação terminal do *store*

A análise do SQLite é de nível forense e vale como modelo:

> "The native store checks database and journal file identities **before and after each operation**. … Any observed removal or replacement is **terminal for that Store**, including a complete, valid database replacement. Reads, writes, typed-object operations, and `Store.Close` then return `recovery storage changed`. **No replacement connection is opened**, and a change observed during a write **suppresses its returned handle** so the Engine keeps the original input. **Restoring the previous files does not revive an invalidated Store.**"

E depois a fronteira da garantia, declarada em vez de escondida:

> "These checks are **not an atomic lock** against external file replacement. … The supported guarantee is **terminal failure after an observed identity change**, not safety under arbitrary concurrent filesystem manipulation."

E a ligação explícita a um incidente real: `issue #1008`, o *orphaned-descriptor* — *"an existing process must report a storage failure instead of acknowledging writes through retired files. Recovery is explicit process restart, not transparent replacement adoption."* E mantêm a conexão aberta até ao fim do processo para que um `close` não faça checkpoint de um WAL obsoleto para a base de dados atual.

Isto é o `security-audit-skill` (§28) aplicado a um produto: **a evidência determina o veredicto, o desconhecido falha fechado, e o limite é publicado**.

### A tabela de recuo (fail-safe), que é a peça central

| Falha | Comportamento |
|---|---|
| modo de runtime desconhecido | usar `record` |
| rota desconhecida | devolver 404 |
| input de transformação malformado | **encaminhar o corpo original** |
| saída da transformação não é menor | **encaminhar o corpo original** |
| CCR indisponível ou cheio | **encaminhar o original; não publicar handle** |
| transformação não suportada para o provider/modelo | **encaminhar o original** |
| classe de segurança desconhecida | **não executar a transformação** |
| preço do provider ausente | marcar `unpriced`; **não adivinhar** |
| falta o MCP de recuperação para um caminho que o exige | **deixar esse caminho sem compressão** |
| processo estranho na porta do proxy | **não reiniciar nem confiar nele** |

E a frase que fecha:

> "Provider errors still reach the caller as provider errors. **A transform failure does not become a synthetic success** or a client-side parse error."

É o oposto exato do §49.7 do maxima (regex que "protegia" com buracos) e do §38 do docling (`PARTIAL_SUCCESS` é cidadão de primeira classe em vez de um sucesso mentiroso).

### O que o rewriter de trajetória recusa a aceitar

Seis pré-condições antes de aceitar: limiar de tamanho ultrapassado **antes** de qualquer chamada ao provider; modelo e credencial explicitamente configurados; passar as validações estruturais; **sinais de falha, contagens, códigos de saída e referências permanecem presentes**; saída melhor que o limiar; original exato guardado atrás de um ponteiro. E o porquê:

> "Past failures often explain why an agent chose its current approach. Removing an exit code, error class, or failed command can make a later step appear unmotivated."

E as fronteiras: *"Rejected rewrites are never inserted into agent context."* · *"Accepted rewrite is model-generated text, **not a lossless encoding**."* · *"Systems can coexist, but **evidence must remain separate**."*

### O planejador de cache — não quebrar o prefixo

Duas regras de engenharia que o katu precisa:

- **Determinismo do prefixo:** *"A transformed block becomes part of later request prefixes. Caveman stores a **deterministic original-to-replacement mapping so the same source block produces the same replacement bytes on later turns**. A replacement cache miss or write failure returns original bytes."* Sem isto, uma compressão "melhor" a cada turno **destrói o cache de prefixo do provider** e custa mais do que poupa.
- **O cache hint não prova o hit:** *"A cache hint does not prove a cache hit; provider response usage determines whether a cache read or write occurred. Local planning records remain `inferred`, and provider observations retain their own evidence basis."*

E os casos de recuo do planejador: JSON malformado ou ambíguo, **chaves JSON duplicadas**, **conteúdo volátil numa fronteira candidata**, e **semântica do provider que divergiu dos dados de capacidade registados**.

### O `subagent-tax` — medir o imposto que nem se vê

Uma ferramenta que **impersona o endpoint do provider num sink loopback local** e deixa cada harness instalado enviar **um pedido real**, para medir o prefixo. O resultado numa máquina real (2026-08-07):

| harness | wire | tools | mcp | system | schemas | body | input (est) | variante |
|---|---|---:|---:|---:|---:|---:|---:|---|
| claude | anthropic-messages | 91 | 63 | 42k | **219k** | 267k | ~43k | config real |
| opencode | openai-responses | 10 | – | 68k | 20k | 87k | ~14k | config isolado |
| codex | openai-responses | 11 | – | 40k | 10k | 52k | ~8.3k | home mínimo (piso) |
| gemini | gemini-generatecontent | 8 | – | 30k | 8.3k | 39k | ~6.3k | home isolado |
| pi | anthropic-messages | **4** | – | 23k | **2.8k** | 26k | ~4.1k | home isolado (4 tools) |
| cursor-agent | – | – | – | – | – | – | – | **impossível medir** |

E a recusa explícita da inferência fácil:

> "What it does **not** say: that claude is '10x pi'. That row is one person's real installed setup with 63 MCP tools; the pi row is a floor with 4 built-in tools. **Comparing them measures a plugin loadout, not a harness.**"

Mais: **toda a execução escreve um *repro pack*** (capturas cruas, logs, configs, `report.json`, `manifest.sha256`), com **redação no momento da escrita** (credenciais **e** identificadores de conta/dispositivo/sessão, `redacted:sha256:<12>` para a igualdade continuar verificável); a calibração é divulgada (`~6.4 chars/token`, banda ±8%, derivada do tokenizer Anthropic e aplicada a todas as protocols — logo as comparações cross-harness carregam um erro adicional, declarado); e a contagem `exact` existe só para Anthropic, porque *"other providers' tokenizers are never approximated as exact."*

E a regra de honestidade estrutural: **`est` e `exact` são dois degraus que nunca se misturam.**

---

## 64. O que extrair para o katu

### Incorporar (direto)

| # | Ideia | Porquê para o katu |
|---|---|---|
| 1 | **Bases de evidência tipadas** (`inferred`/`provider_reported`/`benchmark_counterfactual`/`observed`/`verified`/`unpriced`) mostradas **ao lado do valor**, sem conversão por redação, sem mudança de base em agregação | É a materialização do §51.14. Deve ser um **tipo** no katu, não um campo de texto |
| 2 | **`unpriced` e zero honesto** em vez de preço adivinhado | Elimina o custo inventado **e** a confusão "zero = grátis" |
| 3 | **`verified` como termo reservado** e `confirmed` para o resto | Vocabulário com semântica imposta |
| 4 | **Checklist de publicação** de 7 itens + **portão** (≥6 casos, ≥3 repetições, intervalo 95% todo acima de zero, casos negativos e no-op **não removíveis**) | O `katu` deve ter isto como `xtask gate:bench` |
| 5 | **Insumos contados mesmo quando o resultado é rejeitado** ("spend occurred"), e **overhead da própria ferramenta contado** | Contabilidade que inclui o custo de medir |
| 6 | **Despublicar o passado sem apagar o histórico** (`est_saved_*` ignorados, linhas preservadas) | Correção de rumo auditável |
| 7 | **Tabela de recuo fail-safe** com 10 linhas explícitas, terminando em "encaminhar o original" | É o §51.5 aplicado ao caminho de dados |
| 8 | **"A transform failure does not become a synthetic success"** | Complementa `PARTIAL_SUCCESS` do docling (§38) |
| 9 | **Recuperação obrigatória para qualquer transformação lossy**; sem store → sem transformação | Um invariante, não uma convenção |
| 10 | **Compressores como transformações puras de bytes**, sem network/storage/accounting; o Engine fornece o contexto | O "núcleo puro + portas" do knudge (§23/§12) |
| 11 | **Classe de segurança declarada por transformação**, e todos lossy mesmo quando o round-trip funciona | "Callers must not infer byte safety from a compressor name" |
| 12 | **Refusar o novo para preservar o antigo** na capacidade do store de recuperação | Evita handles pendentes; o oposto do LRU |
| 13 | **Identidade de ficheiro verificada antes e depois de cada operação; invalidação terminal; o handle suprimido em caso de mudança** | O padrão para qualquer store do katu (memória, sessão, checkpoint) |
| 14 | **Fronteira de garantia declarada** ("terminal failure after an observed identity change, not safety under arbitrary concurrent filesystem manipulation") | Honestidade de especificação |
| 15 | **Mapeamento determinístico original→substituto** para não quebrar o cache de prefixo | **Crítico**: sem ele, comprimir o contexto custa mais do que poupa |
| 16 | **`Simulate` que não pode autorizar uma transformação real** — separar medição de ação | O `--dry-run` do `learn` e o `candidate` do docling (§39) na mesma família |
| 17 | **`record` mode** (passagem byte-a-byte) para quando todo o byte tem de ficar visível | Um "modo de confiança" de primeira classe |
| 18 | **TOON só explícito, nunca na deteção geral, e nunca em argumentos de tool** | A distinção `Perception` vs `Action` |
| 19 | **Pixel com allowlist de modelos**, nunca inferida do nome | Não generalizar capability |
| 20 | **Perfis de agente como dados** (`agents/profiles/*.json` + `schema.json`), com `compile.mjs` zero-dep e **falha-fechada nas quatro invariantes de honestidade** | O "plugin como dados" + "o build impõe a honestidade" |
| 21 | **Escala de honestidade de três degraus** para a pureza de dados de um perfil (`declarative`/`builder-assisted`/`code-only`), verificada contra o código real | Declarar o que é dados e o que é código |
| 22 | **`reserved-verbs.json`** — tokens que nenhum id/binary pode ofuscar, compilados no CLI e testados contra a realidade do dispatcher | Governança de namespace |
| 23 | **Orçamento de tokens do prompt núcleo** (`core_prompt_token_budget: 560`) | O teto do `docling` (§36) aplicado ao próprio prompt |
| 24 | **Sink de provider local** para medir o prefixo real, com **labels de variante** por linha, redação na escrita, e a recusa de comparar pisos com configs reais | A ferramenta que responde "onde vão os meus tokens" — e o katu deve poder correr isto |
| 25 | **Regra de recusa de escopo:** o `builder` recusa **três ou mais ficheiros**; o `explorer` só tem Read/Glob/Grep e devolve **uma citação verificada por linha** (`path:START-END reason`), ou literalmente `no relevant locations found` em vez de uma citação adivinhada | §51.2 (pré-condições) + antidoto à alucinação de citações |
| 26 | **"Isolation is mechanism, not outcome guarantee"** — e a recusa de reivindicar benefício líquido sem comparar o uso total | Complementa o §51.9 |
| 27 | **Uma funcionalidade retida até haver prova** ("Codex integration is withheld until transcript isolation has dedicated proof") | Prontidão como pré-condição de integração |
| 28 | **O candidato carrega só o LOCATOR, nunca o corpo** — "Do not trust any body from the candidate; there is none" | Defesa contra injeção via artefacto de proposta (cf. `security-audit` §28) |
| 29 | **Portão de token-líquido-negativo:** *"if after is not below before, revert and report. Never keep an edit that does not reduce tokens/turn."* | Uma otimização que não mede uma melhoria é revertida automaticamente |
| 30 | **Sinks comportamentais são observações** — *"present their numbers as fact and their suggestion softly. Do not turn a behavioral finding into an imperative."* | A fronteira entre facto e conselho, na própria linguagem do produto |
| 31 | **"Spend is what the window COST. It is never what a fix would return"**; nunca multiplicar uma janela por um mês | Extrapolação proibida por desenho |
| 32 | **Licença dupla com função explícita** (MIT = funil, BSL = runtime, **sunset para Apache**) e a explicação honesta do porquê | Modelo de licenciamento para o katu se tiver um componente comercial |
| 33 | **Argumentar contra si próprio no README** ("When caveman loses (net-negative)" com issues reais: #145, #506, #550, incluindo um caso que **piorou** — 4.3M vs 1M tokens) | A prova de que a honestidade é praticável |

### Lapidar (adaptar, não copiar)

- **A compressão de *input* não pertence ao núcleo do katu.** O katu é um agente: o seu núcleo é a máquina de estados e a política (§51). O caveman ensina o katu a **medir e a contabilizar**, não a transformar tokens de terceiros. O lugar natural é um **porta de contexto** (`katu-context`) que o núcleo **não** abre por defeito e que devolve sempre o original em caso de dúvida.
- **A arquitetura de 7 processos** (CLI, proxy, engine, mcp, mem, browse, shrink) é a resposta correta para *um produto de conveniência que se instala em 30 agentes*. O katu tem um binário: o mesmo resultado vem de **crates + feature flags**, não de processos ligados por stdio.
- **A compressão de *output* é a lição mais transferível e a mais barata.** O núcleo léxico da skill — não inventar abreviaturas, não usar setas, não crescer, nunca dropar negação, STE para clareza, idioma ≠ estilo, Auto-Clarity — deve entrar no katu como **regras de estilo tipadas** (§51.10), com a Auto-Clarity como um **estado de saída que revoga a compressão**, não como uma exceção de prosa.
- **`before → after` por edição com consentimento** (`learn apply --dry-run`, mostrar o diff, pedir sim/não, recontar, reverter se não melhorar) é o fluxo exato do `propose-then-commit` do §33.
- **A frase "no output-reduction percentage is published until a reproducible run and raw outputs are committed"** deve virar uma regra do katu: nenhum número do katu sem o artefacto commitado que o produziu.

---

## 65. O contra-checklist do caveman para o katu

1. **Não publicar percentagens sem a execução crua commitada.** O caveman **removeu as suas próprias**.
2. **Não deixar uma base de evidência mudar durante uma agregação.**
3. **Não marcar nada como `verified` sem um método de verificação imposto e nomeado.**
4. **Não remover casos negativos nem no-op de um benchmark.**
5. **Não estimar o custo de uma transformação a partir da representação local mais pequena** — o provider conta imagens e texto estruturado de outra forma.
6. **Não comprimir argumentos de tool.** Mudar argumentos pode mudar o comportamento.
7. **Não inferir suporte de modelo a partir do nome** (visão, tamanho de janela, tokenizer).
8. **Não inferir que um input é seguro a partir do nome do compressor.**
9. **Não emitir um resultado lossy sem recuperação garantida.**
10. **Não despejar o antigo para aceitar o novo** num store do qual um agente pode pedir handles antigos.
11. **Não adivinhar preços.** Zero + `unpriced`.
12. **Não transformar um erro do provider num sucesso sintético.**
13. **Não deixar a compressão invalidar o cache de prefixo** — mapeamento determinístico, ou não comprimir.
14. **Não invocar uma transformação real a partir de uma simulação.** `Simulate` não autoriza.
15. **Não tratar "isolamento" como garantia de resultado.**
16. **Não deixar um artefacto de proposta carregar o payload** — só o locator.
17. **Não manter uma edição que não mediu uma redução.** Reverter.
18. **Não transformar uma observação comportamental num imperativo.**
19. **Não fingir que um prefixo de 219k chars de tool schemas é uma propriedade de harness.** É um *loadout* de plugins.
20. **Não pôr um budget de tokens no prompt que ninguém verifica** (o caveman tem 560 e um compilador atrás; o `maxima` tinha "≤100k" e o código dizia 150k — §49).

### A frase que o caveman entrega ao katu

> "**Measurement first, claims never ahead of evidence.**"

O katu quer ser um agente em que se **confia**. Uma propriedade necessária de confiança é que o agente não possa fabricar os números que descrevem o seu próprio desempenho. O caveman mostra como construir isso: o tipo da evidência viaja com o valor, a base não se converte por redação, o desconhecido é `unpriced` e não zero, o negativo permanece visível, e **o build falha se a honestidade declarada não corresponder à realidade**.

---

## 66. Síntese final — o mapa de decisões do `katu`

Com o caveman fechamos **onze referências** mais o documento original. Esta é a síntese que alinha o brainstorm.

### 66.1 O que cada referência deu ao katu

| # | Referência | O que **provou** | O que o katu **leva** | O que o katu **rejeita** |
|---|---|---|---|---|
| 1 | `agentes-de-ia.md` | o objeto de estudo | o vocabulário | — |
| 2 | **`pi`** | um harness mínimo, legível e extensível funciona | o split *core/extensões*, o log como verdade, o TUI próprio | a ABI não versionada, o `node_modules` como dependência de build |
| 3 | **`goose`** | como se industrializa um harness (15 crates, ~278k LOC) | a disciplina de providers/declarativos, o *state machine*, os testes | a superfície (forkar é caro; não é um kernel de política) |
| 4 | **`sniff-css`** | "só o delta": medir sem IA é possível | o firewall determinístico antes do LLM | — |
| 5 | **`arags`** | uma plataforma maior que o agente é negativa líquida | as lições negativas: server-first, multi-user prematuro | 4 espaços de armazenamento, orquestrador recursivo |
| 6 | **`knudge`** | memória **pura** + *ports* + matemática medida | o núcleo puro, `Clock`/`Rng` injetados, `matematica.md` com a origem das constantes | o acoplamento ao MCP como única interface |
| 7 | **`open-mtr-rs` / `taskdeck`** | *facade* + *trait* de transporte + verdade legível | a porta `HttpTransport` como molde da porta `Memory`; a verdade em markdown + índice descartável | `Arc<dyn>` + `async_trait` no hot path, `#![allow(dead_code)]` |
| 8 | **`open-keyboard`** | como as claims viram ficção sem verificação | o **anti**-checklist (nonce determinístico, benchmark que não compila, FFI sem refcount) | criptografia caseira, pesos mágicos desbalanceados |
| 9 | **`security-audit-skill`** | a **epistemologia da verificação** | a tríade `allowed`/`blocked`/`needs_human` com evidência, a alocação de cobertura, o orçamento com reserva obrigatória, finder ≠ verifier | a ideia de que severidade se aplica sem demonstração |
| 10 | **`ai-engineering-from-scratch`** | o desenho das **superfícies** de um agente | as 7 superfícies, as 5 categorias de regra, o gate de verificação determinístico, *durable execution*, `propose-then-commit`, *cost governors* | tratar o *curriculum* como produto |
| 11 | **`docling`** | arquitetura **verificada por CI** | `xtask check-layers`, teto de linhas, `ToolOutcome` com `Partial`, `SKILL.md` com `allowed-tools`, plugins com gate de confiança | 30+ backends por acumulação, dependências de inferência no caminho base |
| 12 | **`deepseek-harness`** | o limite **oposto** (tudo plugin) + a disciplina de conhecimento | *capability seams*, `Model-visible ⟺ logged`, invariantes de pacote, **postmortems**, *Agent Notes* com alternativas obrigatórias, "impor a decisão na operação que a toma", **"testar a entrada real"** | o contentor de DI geral, `100%` de cobertura como meta, 316 pacotes |
| 13 | **`maxima-harness`** | o **rascunho fundacional** da tese do katu, e o seu teto | a tabela 70%/100%, `.STAGING.md`, o trust gate, o split live/durable, `@rule` com efeito imediato | regex como política, `docker.sock` no agente, política em ficheiro gitignored, 19 extensões sem governança |
| 14 | **`zed`** | a **prova de produção** da fronteira de capacidades em Rust | ABI por mundos WIT congelados, carimbo de versão no artefacto, capacidades globadas de dois lados, `PENDING_CHANGES`, a governança do `.rules`, `disallowed-methods` com motivo | WASM como pré-requisito, plugins omnibus, duplicar o anfitrião por ambiente |
| 15 | **`caveman`** | como **medir sem mentir** (e o eixo do input/output) | as bases de evidência tipadas, o portão de publicação, a tabela de recuo, CCR com invalidação terminal, determinismo de prefixo, a skill como taxonomia de texto | os 7 processos, transformar contexto no núcleo do agente |

### 66.2 As sete decisões fundacionais do `katu`

Depois de quinze referências, o brainstorm converge em sete decisões. Cada uma tem evidência positiva **e** negativa.

| # | Decisão | Evidência a favor | Evidência contra (o que acontece se não) |
|---|---|---|---|
| **1** | **O katu possui o loop.** É uma máquina de estados com pré-condições tipadas, não uma camada de regras sobre um substrato de terceiros | `zed` (capacidades), `docling` (lifecycle com `finally`), `dsh` (invariantes) | `maxima` (§48–§49): "a extensão decide" mas 85% das regras são prosa a 70%; o `rm` escapa; o `node_modules` é remendado |
| **2** | **A política avalia factos, não texto.** `Capability` explícitas, caminhos canonicalizados, argv resolvido | `zed` (`process:exec {command, args}`, `download_file {host, path}`), `security-audit` (evidência estruturada) | `maxima` com 7 regex: `r''m`, `find -delete`, `git clean -fdx`, `docker run` fora da lista |
| **3** | **Toda a regra declara a sua categoria.** `Enforced{rule_id}` ou `Advisory{rationale}` ou `Perception`; e o katu imprime a lista do que instrui e não impõe | `zed` ("traps to avoid, not maps to follow"), `caveman` (Auto-Clarity, `est` vs `exact`, `verified` reservado) | `maxima` (§51.10): ~50 regras em prosa avaliadas em 70% pelo próprio documento |
| **4** | **Fail-closed em todas as fronteiras.** Sandbox com fiscalização relatada, nunca passthrough silencioso; recuo para o original; `unavailable` nega; desconhecido não executa | `caveman` (tabela de 10 recuos), `dsh` (`partial`, `allowed-once`, `unavailable`) | `maxima`: `--host` de primeira classe + `docker.sock`; `open-keyboard`: chave zero no `init` |
| **5** | **A evidência viaja com o número.** Bases tipadas, `unpriced` ≠ zero, negativos visíveis, gate executável de publicação | `caveman` (§62 inteiro), `security-audit` (§29) | `maxima` (§49): "−77% tokens" e "assertividade 95%" para um cenário escrito à mão; `open-keyboard`: benchmark que nunca compilou |
| **6** | **Uma capacidade, um provedor.** A porta `Memory` com tipos do katu; adaptadores atrás de *features*; sem legado em paralelo | `knudge` (4 deps, núcleo puro), `open-mtr-rs` (porta de transporte) | `maxima` (§49.5): 8 dualidades, incluindo o ARAGS **e** o HTTP legado anos depois |
| **7** | **O conhecimento e a política são artefactos de primeira classe, versionados e gate-verificados.** ADRs com alternativas obrigatórias, postmortems, regras de estilo, catálogos gerados | `dsh` (postmortems, Agent Notes), `zed` (Rules Hygiene, licenças em CI), `docling` (fitness functions), `caveman` (compile.mjs falha-fechado) | `maxima` (`.pi/` todo gitignored: as próprias regras do projeto fora de revisão) |

### 66.3 O que o katu é, em uma frase

> **O `katu` é o loop possuído de um agente de código, com a política como kernel determinístico que avalia factos em vez de texto, capacidades em vez de denylists, e evidência tipada em vez de números afirmados — de modo que a única coisa que o agente não pode fazer é decidir por si próprio o que lhe é permitido.**

Cada metade dessa frase vem de uma referência diferente, e as sete decisões de 66.2 são o contrato entre elas.

### 66.4 O que fica **fora** do katu (e porquê)

Para o brainstorm não continuar a crescer indefinidamente, é tão importante fixar o que não entra:

| Fora | Porquê |
|---|---|
| Um framework de plugins geral (contentor de DI, `ctx` dinâmico) | `dsh` provou que é poderoso e pagou-o com dois postmortems do próprio carregador e 316 pacotes |
| WASM obrigatório para plugins no MVP | `zed` mostra o custo (enum de 10 versões × ~25 métodos, `wasi-sdk` no build). O **modelo** de capacidades entra já; o **runtime** pode esperar |
| Compressão de contexto como função do núcleo | É uma *porta* (`katu-context`), desligada por defeito, com recuperação obrigatória — não um caminho do núcleo |
| Um motor de memória no katu | `knudge` é o motor; o katu é o **consumidor de referência**, atrás da porta `Memory` (não fundir os repos) |
| Um servidor, multi-utilizador, ou daemon | `arags` (§20) é a prova de que a plataforma maior que o agente é negativa líquida |
| Um DSL de configuração ou expressões avaliadas | postmortem 0002 do `dsh` (`!!js`): um YAML com expressões é um risco de correção e de segurança |
| Cobertura de 100% como meta | postmortem 0001 do `dsh`: 178 testes verdes e 100% de cobertura com o produto quebrado |
| Métricas de *output* como prova de capacidade | JetBrains mediu 8.5% menos tokens com qualidade igual — bom, mas **não é** prova de que o agente é melhor |
| Um "modo sem isolamento" de primeira classe | `maxima --host`. Se o katu precisar dele, é `needs_human` com autorização registada, nunca um banner |

### 66.5 Estado do brainstorm

O documento tem agora **§0–§66**, com **15 referências** analisadas em profundidade, e cobre: tese e evidência (§1–§9), memória e as ferramentas próprias (§10–§27), verificação e desenho de superfícies (§28–§34), engenharia madura (§35–§40), o limite oposto (§41–§46), o rascunho fundacional (§47–§53), a prova de produção (§54–§59) e a medição honesta (§60–§65). **A fase de reconhecimento está fechada.**

---

## 67. Próximo artefato sugerido

A fase de reconhecimento fechou (§66.5). A ordem abaixo segue as **sete decisões fundacionais** de §66.2 — cada artefacto materializa uma, e cada um cita a evidência que o sustenta.

| # | Artefacto | Materializa | Evidência principal |
|---|---|---|---|
| **01** | `proposal/katu/01-adr-o-loop-possuido.md` | decisão 1 (o loop possuído) | §48–§53 (`maxima`), §42 (`dsh`) |
| **02** | `proposal/katu/02-adr-motor-de-politica.md` | decisão 2 (factos, não texto) | §51.4/§51.7, §55/§56 (`zed`), §28–§29 (`security-audit`) |
| **03** | `proposal/katu/03-adr-categorias-de-regra.md` | decisão 3 (categoria declarada) | §51.10, §57.1 (`zed` Rules Hygiene), §61 (`caveman` Auto-Clarity) |
| **04** | `proposal/katu/04-adr-sandbox-e-capacidades.md` | decisão 4 (fail-closed) | §51.5, §43 (`dsh`), §63 (`caveman` tabela de recuo) |
| **05** | `proposal/katu/05-adr-evidencia-e-contabilidade.md` | decisão 5 (a evidência viaja com o número) | **§62 inteiro** (`caveman`), §29 (`security-audit`), §49 (`maxima`) |
| **06** | `proposal/katu/06-adr-memoria-e-porta.md` | decisão 6 (uma capacidade, um provedor) | §12–§13 (`knudge`), §51.12 |
| **07** | `proposal/katu/07-adr-conhecimento-e-governanca.md` | decisão 7 (conhecimento como artefacto) | §44 (`dsh`), §36 (`docling`), §57 (`zed`), §62 (`caveman`) |
| 08 | `proposal/katu/08-arquitetura-crates.md` | o recorte físico | §4, §54–§58 (`zed` seams), §27 |
| 09 | `proposal/katu/09-arquitetura-plugins-e-abi.md` | o sistema de plugins | §54–§59 (`zed`) |
| 10 | `proposal/katu/10-performance-e-benchmarks.md` | os tetos e o portão de medição | §62 (`caveman`), §36 (`docling`), §26 (`open-keyboard`) |
| 11 | `proposal/katu/11-modelo-de-contexto.md` | contexto, ficheiros, *compaction*, checkpoint | §19 (`maxima` `.STAGING.md`), §42 (`dsh` live/durable), §61 (`caveman`) |

**Ordem de execução recomendada:** 01 → 02 → 03 → 04 → 05 (o kernel de confiança completo) → depois 06 e 07 (as duas fronteiras de dados) → e só então 08–11 (o recorte físico e os tetos), que são consequências e não premissas.

O artefacto **01** é o mais importante do projeto: é onde se inscreve, com o `maxima-harness` citado como evidência empírica, a razão pela qual o katu tem de possuir o loop em vez de o pedir emprestado.
