# 01 — Decisões fundacionais (congeladas)

> **Contrato de projeto.** A partir daqui, código e documentação derivam. Cada decisão tem:
> enunciado, evidência a favor, evidência contra, consequência arquitetural e **o teste que a
> trava**. Alterar uma `DFxx` exige commit próprio e uma nova `DF` que a substitua ligada a ela
> (§44, "Agent Notes: nunca editar uma nota para uma decisão diferente").
>
> Base: §66.2 da brainstorm. Status: ✅ congelada.

---

## DF1 — O katu possui o loop ✅

**Enunciado.** O `katu-core` **é** a máquina de estados do agente. As regras são **transições**,
não intercepções de texto sobre um substrato de terceiros.

**Evidência a favor.** `zed` prova a fronteira de capacidades em Rust (§54–§58); o `docling`
impõe ciclo de vida com teardown garantido (§35); o `dsh` exige invariantes de pacote (§43).

**Evidência contra (o que acontece sem isto).** O `maxima` (§48–§49): a extensão "decide" mas
85% das regras ficaram em prosa a ~70%; o `rm` escapou por regex; o `node_modules` teve de ser
remendado; 12 variáveis de módulo faziam o estado do caminho único (§49.3).

**Consequência.** O estado é um **valor**; a transição é `Step(State, Event) -> Result<State,
Refusal>`. Nada de singletons de processo. Habilita replay, fork, checkpoint e teste.

**Teste que trava.** `E04-T04`: um teste conduz o loop real e verifica que uma transição ilegal
(ex.: fechar tarefa sem verificação) é **recusada**; um replay do event log reproduz o mesmo
estado final byte a byte.

---

## DF2 — A política avalia factos, não texto ✅

**Enunciado.** `katu-policy` avalia `ToolUse{name, args, resolved_paths, argv, cwd}`, `Phase` e
`Budget`, devolvendo `Allow | Deny{reason, evidence} | RequireApproval | NeedsHuman`. Nunca um
`Vec<Regex>` sobre a string do comando.

**Evidência a favor.** `zed` (`process:exec {command, args}`, `download_file {host, path}`) é
implementação de referência e testada (§55); `security-audit` exige evidência estruturada (§28).

**Evidência contra.** O `maxima` com 7 regex: `r''m`, `\rm`, `find -delete`,
`python -c "shutil.rmtree(...)"`, `docker run` fora do denylist, substring de caminho que não
resolve `../` nem symlinks (§49.8).

**Consequência.** Caminhos **canonicalizados** antes de decidir; `argv` resolvido; capacidades
declaradas. O veredicto é **função pura e determinística** (testável sem I/O).

**Teste que trava.** `E02-T05`: golden de veredictos para uma matriz de factos (incluindo
`../`, symlink, `bash -c`, encadeamento `&&`); proptest de determinismo (mesmo input → mesmo
veredicto) e de monotonicidade (mais capacidades nunca nega o que já permitia).

---

## DF3 — Toda regra declara a sua categoria ✅

**Enunciado.** Todo texto injetado e toda regra é classificada: `Enforced{rule_id}` (tem motor),
`Advisory{rationale}` (só conselho) ou `Perception` (contexto factual). O katu imprime a lista
de tudo o que **instrui e não impõe**.

**Evidência a favor.** `zed` ("traps to avoid, not maps to follow", §57.1); `caveman`
(Auto-Clarity, `est` vs `exact`, `verified` reservado, §61).

**Evidência contra.** O `maxima` (§51.10): ~50 regras em prosa, avaliadas em ~70% pelo próprio
documento; três rondas de reforço de prosa sem corrigir o incumprimento (§49, §52.13).

**Consequência.** A lista de `Advisory` é **dívida técnica endereçável**, não invisível. Uma
regra que afirma prioridade tem de ser imponível, senão é reescrita como recomendação.

**Teste que trava.** `E02-T07`: `xtask policy:audit` falha se existir texto de instrução sem
categoria; a lista `Enforced` vs `Advisory` é publicada e comparada com a esperada.

---

## DF4 — Fail-closed em todas as fronteiras ✅

**Enunciado.** Contenção **determinística (soft)** no MVP: política imposta na operação, caminhos
canonicalizados, capacidades tipadas, `unavailable` nega, desconhecido não executa. **Nunca
vendida como fronteira de segurança** — o katu corre global, como o utilizador. Recuo para o
original em dúvida. A **jail de SO real** (bwrap + Landlock) é feature **futura** (E17).

**Evidência a favor.** `caveman` (tabela de 10 recuos, §63); `dsh` (`partial`, `allowed-once`
única concessão, `unavailable` nega, §43); o **`ai-jail`** (pesquisa externa, fora do projeto)
como referência da jail
**futura** (bwrap + Landlock + seccomp + rlimits, projeto monotónico, egress filtrado, escapes).

**Evidência contra.** O `maxima`: `--host` de primeira classe com um banner + `docker.sock` no
agente (§49.7); o `open-keyboard`: chave zero silenciosa no `init` (§26).

**Consequência.** Um controlo em falta vira `Denied`/`Unavailable` (contenção soft); a execução
**não** acontece sem autorização explícita e registada. A garantia é declarada, não escondida.

**Teste que trava.** `E07-T03`: com o controlo ausente (path não canonicalizável, `argv`
desconhecido, `exit_code: null`, autorização em falta), a operação **não** corre
(`Denied`/`Unavailable`); um teste afirma que a contenção é **soft**; `RunnerFailureRule` exige a
conjunção (exit ≠ 0 **e** assinatura fatal).

---

## DF5 — A evidência viaja com o número ✅

**Enunciado.** Todo número que descreve o desempenho carrega a sua **base de evidência tipada**
(`measured` | `inferred` | `provider_reported` | `benchmark_counterfactual` | `observed` |
`verified` | `unpriced`). Bases não se convertem por redação; não mudam durante uma agregação;
negativos e no-op permanecem visíveis; zero + `unpriced` em vez de preço adivinhado.

**Evidência a favor.** `caveman` (§62 inteiro): linhas vermelhas mantidas, despublicação honesta,
`verified` reservado; `security-audit` (tríade com evidência, §28).

**Evidência contra.** O `maxima` (§49): "−77% tokens" e "assertividade 95%" para um cenário
escrito à mão; o `open-keyboard`: um benchmark que nunca compilou (§26).

**Consequência.** Um número sem artefacto reproduzível é marcado `[estimativa]` e **não pode
fundamentar decisão** (§51.14). É um **tipo**, não um campo de texto.

**Teste que trava.** `E15-T02`: `xtask gate:bench` exige artefacto commitado por número; o build
falha se um valor publicado não tiver a sua base; a linha negativa do benchmark não pode ser
removida.

---

## DF6 — Uma capacidade, um provedor ✅

**Enunciado.** A porta `Memory` é definida com **tipos do katu** (não `knudge_core::*`). O
adaptador **in-process** (`knudge-core`) é o **primário e único no escopo atual**; o adaptador
MCP é **futuro, opcional** (fora do escopo). Sem legado em paralelo, sem dualidade ativa.

**Evidência a favor.** O knudge tem núcleo puro com 4 dependências e foi desenhado para embedding
(D65/D68, §10); `open-mtr-rs` mostra a porta de transporte como molde (§14); G4 exige o knudge
como memória, não como plugin.

**Evidência contra.** O `maxima` (§49.5): 8 dualidades, incluindo o ARAGS **e** o HTTP legado
mantido "como fallback" anos depois.

**Consequência.** Teste de sanidade (§13): "se amanhã voltarmos ao MCP, quantos ficheiros do katu
mudam?" Se for "só o adaptador da porta", o desenho está certo. Enquanto isso, **existe um só
adaptador** — a porta é exercida pelo `FakeMemory` nos testes, não por um segundo backend real.

**Teste que trava.** `E03-T06`: teste de conformidade do contrato corre contra o `FakeMemory` e o
adaptador in-process; nenhum tipo do knudge vaza para a API do katu (`xtask check-layers` falha se
`knudge-core` aparecer fora do adaptador). O adaptador MCP (E08) fica `deferred` e, se construído,
reusa a mesma suíte.

---

## DF7 — Conhecimento e política são artefactos de primeira classe ✅

**Enunciado.** ADRs com `Alternatives considered` obrigatório; postmortems quando um bug é
sútil, sistémico e caro de redescobrir; regras de estilo; catálogos gerados; política versionada
e revisível em PR.

**Evidência a favor.** `dsh` (postmortems, Agent Notes, §44); `zed` (Rules Hygiene, licenças em
CI, §57); `docling` (fitness functions, §36); `caveman` (`compile.mjs` falha-fechado, §62).

**Evidência contra.** O `maxima`: `.pi/` todo gitignored — as próprias regras do projeto ficaram
fora de revisão (§51.6).

**Consequência.** `policy/` versionado · `.katu/` runtime (não versionado) · `secrets` fora de
ambos. Toda decisão registada cita o artefacto que a produziu.

**Teste que trava.** `E14-T01`: `xtask check-docs` exige `## Alternatives considered` nas ADRs;
`xtask check-layers` falha se `policy/` não estiver sob controlo de versão; arquivos gerados sem
gate falham o build.

---

## DF8 — O provider built-in é first-party e é um endpoint de modelo ✅

**Enunciado.** O caminho built-in de modelos é **nosso**: o gateway **`opencode go/zen`** (hot
path, stateless) e o **`llama.cpp`** local (opcional). Todo o resto entra pelo **GDK**
declarativo/out-of-process ou é **ativamente ignorado**. O seam é um **endpoint de modelo**,
**nunca** o agente: a HttpApi (`/api/*`) do OpenCode **não** é substrato do loop. No built-in,
**latência > compressão** (keep-alive/pooling, `TCP_NODELAY`/HTTP2, SSE incremental,
`Accept-Encoding: identity`); a compressão do G6 continua a valer para o **input do modelo**. O
modelo e o grau de pensamento são acionados pelo **utilizador**, nunca auto-escalados pelo agente.

**Evidência a favor.** O gateway Zen/Go fala 4 dialetos compatíveis com SDKs conhecidos (Responses,
Messages, chat/completions, Gemini) com API key e session affinity (`x-opencode-session`); o
`llama.cpp` expõe `llama-server` HTTP compatível com OpenAI (L1) e FFI in-process (L2). O
`goose-rs`/GDK cobre o resto sem escrevermos N adaptadores.

**Evidência contra.** Quatro dialetos a normalizar; o GDK ainda é `0.1.0-alpha.11` (R1); a FFI do
`llama.cpp` é `unsafe`. Nada disto é único ao katu — é custo conhecido e isolado no adaptador.

**Consequência.** O built-in vive em `katu-providers` (`E12-T01/T06/T07/T08/T09/T10`); o resto é
delegado e out-of-process; o modelo é **cliente** do plano de dados, não parte dele (firewall
LLM-free). `katu-core`/`katu-policy`/`katu-tools` **não** dependem de nenhum crate de provider.

**Teste que trava.** `E12-T01`: `xtask check-layers` falha se `core`/`policy`/`tools` importarem um
provider. `E12-T07`: TTFT/throughput medidos com artefacto commitado (a latência do hot path é um
número com base, não uma promessa — DF5).

---

## DF9 — Instrumentação transversal on-demand, custo zero por defeito ✅

**Enunciado.** **Toda** operação do katu abre um `span!` (log estruturado + métrica de tempo) e
pode emitir `event!`. Os registos são **sempre estruturados**: identificador estável + campos
tipados; nunca texto livre interpolado. O consumo é um `Sink` plugável. O custo é **zero** quando a
instrumentação é compilada fora (`feature = "instrument"`, off por defeito); com a feature, é uma
leitura atómica *relaxed* + ramo até ser ligada em runtime (`KATU_INSTRUMENT=1`). A instrumentação
**nunca** entra no log de sessão nem no contexto do modelo.

**Evidência a favor.** O E18/E15 medem; sem instrumentação densa e barata, otimizar é adivinhar (os
falsos positivos que o §65 proíbe). O exemplo do `knudge` mostra o valor do recorte micro + e2e —
que exige instrumentação fina e determinística.

**Evidência contra.** Instrumentação espalhada pode degradar e poluir; mitigação: macros *no-op*
compiladas fora, ativação explícita e revisão do teto de superfície (E14).

**Consequência.** `katu-core::diag` (porta `Sink` + macros `span!`/`event!`); o binário instala o
sink só quando pedido; todos os épicos instrumentam as suas operações. DF9 é pré-condição do
E15/E18 e substitui o port `Logger` (uma só superfície de diagnóstico).

**Teste que trava.** `E19-T01`: com a feature ligada, spans/eventos são registados; desligada,
**zero** registos e o caminho ativo não é compilado (o build por defeito não contém `diag::active`).

---

## DF10 — Recusas são acionáveis; pré-requisitos são aprovação (soft) ✅

**Enunciado.** Toda recusa que chega ao modelo carrega o `rule_id` da regra que a produziu
(`Denied` carrega também `evidence`; `Unavailable` carrega `rule_id`). A **forma de aplicação**
decide o tipo de recusa, não a severidade: `DenyCommand`/`DenyWrite`/`DenyDelete` sem a capacidade
correspondente → `Denied` (violação, falha fechada); `RequireBefore`/`RequireAfter` (pré-requisito
de protocolo — ex.: recall antes de write, outcome antes de close) → `RequireApproval` →
`Unavailable`. `severity` distingue apenas `Deny*` `critical` (nega) de `warn` (aprovação).
Promover um pré-requisito a muro é decisão própria (nova DF/regra), nunca efeito colateral.

> **Superada parcialmente por [DF11](#df11--severity-decide-também-para-requirebeforeafter-pré-requisito-crítico--muro-):**
> a cláusula "`RequireBefore/After` são sempre aprovação" cai; passam a seguir a `severity`. O
> resto (recusa sempre acionável) mantém-se.

**Evidência a favor.** DF2/OA2: o veredicto é tipado e a distinção `Deny` vs `RequireApproval` tem
de significar algo — `Deny` = violação, `RequireApproval` = falta um pré-requisito/controlo.
`plan/03` (E02-T07) mapeia âncora e outcome a `RequireAfter` **de propósito**; a validação
semântica (duplicata/âncora/claim) vive no adaptador e chega como **capacidade** → `Deny`. O
`maxima` (§49) mostra o custo de "regras em prosa": aqui o que recusa é sempre identificável.

**Evidência contra.** "Gravar sem recall" fica mais fraco como *muro* (é aprovação, não negação);
num agente autónomo sem humano, aprovação = bloqueio, mas a evidência é de tipo diferente. Exige
que `Unavailable` transporte o `rule_id` (antes era opaco).

**Consequência.** `ToolOutcome::Unavailable { control, rule_id }` e `ToolOutcome::rule_id()`;
`pipeline::skipped_outcome` propaga `RequireApproval.request.rule_id`. A checklist E05-T07 aceita
`Denied` **ou** `Unavailable`, ambos com `rule_id`. A promoção de "sem recall" a `deny_command`
fica em aberto, a decidir com medição (E05-T06).

**Teste que trava.** `E05-T05` (`crates/katu/tests/mvk.rs`): (a) gravar sem recall → `Unavailable`
com `rule_id == mem-recall-before-write`, executor a zero; (b) duplicata → `Denied` com
`rule_id == mem-no-duplicate`; (c) fechar sem `outcome` → `Refusal`. O acesso `ToolOutcome::rule_id`
tem teste próprio (`error::tests::refusal_is_actionable_with_rule_id`).

---

## DF11 — `severity` decide também para `RequireBefore/After` (pré-requisito crítico = muro) ✅

**Enunciado.** `RequireBefore`/`RequireAfter` deixam de ser sempre aprovação: seguem a
**severidade**, tal como as `Deny*`. `critical` → `Deny { reason, rule_id, evidence }`; `warn` →
`RequireApproval`. Assim "gravar sem recall" e "fechar sem outcome" (ambos `critical`) passam a ser
**muros** na política; o `Refusal` de kernel para `→ Closed` permanece (defense-in-depth).
Substitui a cláusula de DF10 "`RequireBefore/After` são sempre aprovação"; o resto de DF10
(recusa sempre acionável) mantém-se.

**Evidência a favor.** O doc de `Severity` já diz "Crítica (nega)". A distinção `Deny` vs
`RequireApproval` continua a ter significado (muro vs controlo em falta) e passa a depender de um
campo explícito por regra, não do tipo de enforcement. O gate E05 pede um muro para "gravar sem
busca". DF10 garante que o modelo recebe `rule_id` + `evidence` — a correção continua possível
(fazer recall e repetir).

**Evidência contra.** Muda a semântica congelada de E02 (2 testes). Torna um pré-requisito um muro
(tensão com §47 "sem muros"); mitigação: `warn` existe para pré-requisitos genuinamente soft e o
`waiver` (regra/fase) é o escape explícito. Pode esconder o *porquê* se o `rule_id` não for
mostrado — coberto por DF10.

**Consequência.** `engine::verdict` escolhe por `severity` para `Deny*` e `Require*` (helper
`severity_verdict`); `ToolOutcome` mapeia `Deny` → `Denied { rule_id, evidence }`. Os testes de
E02/E05 atualizados.

**Teste que trava.** `E02-T05`/`E02-T07`: `critical_require_after_denies`,
`write_without_recall_is_denied`, `close_without_outcome_is_denied`; `E05-T05`: (a) → `Denied` com
`rule_id == mem-recall-before-write`; `warn_require_after_requires_approval` fixa o outro lado.

---

## DF12 — Ferramentas AI-first: envelope tipado + views; core otimizado por medição ✅

**Enunciado.** A superfície de tools é **fechada** (§1.1), mas a sua **forma** é AI-first:

1. Cada tool devolve um **envelope tipado único** (`ToolReport`: `kind`, `id`, `hash`, `data`,
   `page`, `next`, `cost`) e as tools são **ortogonais** (`read`/`write`/`edit`/`move`/`trash`/
   `bash`/`grep`/`find`/`ls`/`plan`) — nunca um `fs_op(mode=…)`. `ToolOutcome` continua o eixo de
   **estado** (Ok/Partial/Denied/…); o envelope é o **payload**.
2. `read` expõe **views** (`outline`/`summary`/`symbol`/`diff`/`full`); `grep`/`find` devolvem
   **hits semânticos** (símbolo + tipo de linha + informação negativa), não `arquivo:linha:texto`.
3. O formato **ao modelo** é um subconjunto **TOON** (canónico na emissão, sem `null`, vazios
   omitidos, ordem canónica), precedido de um **prime compacto**; **JSON** é a alternativa de
   máquina (`format=json` / `--json`).
4. O **core** (índice, cache, syscalls) **só** se otimiza onde o profiler apontar (adoptar-ou-
   reverter, E18); começa em `std::fs` + cache **L1** em memória por `path+fingerprint`.

**Evidência a favor.** §18 ("só o delta chega ao modelo") e G6 já o exigem; o knudge prova o TOON
como contrato de bytes (spec `TOON`, D74/D75/D166) e o custo de `cat`/`ls`/`grep` humanas (ruído
em tokens, sem *affordance* de decisão) é observável.

**Evidência contra.** TOON exige um **prime** (o modelo tem de ser ensinado); duplica a spec do
knudge (não há crate partilhado — firewall); um envelope "rico" adiciona tokens **por chamada** —
mitiga-se cortando **chamadas** (a métrica é tool calls/tarefa, não tokens/call).

**Consequência.** `katu-core::toon` (emissor próprio, zero deps, golden) + `ToolReport` no contrato
de tool; `move` entra na família de Escrita; o prime vive em E09 (contexto). Métricas por tool call
em E15/E18-T10; índice/cache/syscalls gated por medição.

**Teste que trava.** `E06-T01` (registry fechado), `E06-T03` (views + envelope), `E06-T05` (hits
semânticos), `katu_core::toon` (golden/proptest), `E15` (tool calls/tarefa medidos).

---

## Tabela de rastreabilidade rápida

| Decisão | Épicos que a implementam | Teste canónico |
|---|---|---|
| DF1 | E04, E05 | `E04-T04` |
| DF2 | E02, E06 | `E02-T05` |
| DF3 | E02, E14 | `E02-T07` |
| DF4 | E07, E03 | `E07-T03` |
| DF5 | E09, E15 | `E15-T02` |
| DF6 | E03 | `E03-T06` |
| DF7 | E14, E15 | `E14-T01` |
| DF8 | E12 | `E12-T01`, `E12-T07` |
| DF9 | E19, E15, E18 | `E19-T01`, `make instrument` |
| DF10 | E02, E04, E05 | `E05-T05` |
| DF11 | E02, E05 | `E02-T05`, `E05-T05` |
| DF12 | E06, E09, E15, E18 | `E06-T03`, `E06-T05`, `toon::tests` |

---

## Decisões em aberto (a resolver no épico indicado, nunca em prosa)

| # | Ponto | Épico | Default provisório |
|---|---|---|---|
| OA1 | Persistência: JSONL append-only vs SQLite | E04 | **JSONL append-only** (o log é a verdade; índice derivado) |
| OA2 | Semântica de negação: erro recuperável vs parada dura | E02 | **ambos**: `Denied` recuperável por default; `NeedsHuman` para irreversível. Refinada por **DF10** (recusa sempre acionável) e **DF11** (`critical Require*` = muro) |
| OA3 | TUI: `crossterm` cru vs `ratatui` | E10 | **`ratatui` 0.30 + `crossterm` 0.29** (skill [`ratatui-tui`](../../../.agents/skill/git-daily/SKILL.md): estado central, render puro, executor async); rever antes de E10-T01 |
| OA4 | Providers do MVP | E12 | built-in first-party **`opencode go/zen`** (hot path) + **`llama.cpp`** (local, opcional); todo o resto via **GDK** ou ignorado ativamente |
| OA5 | Contenção / jail | E07 → E17 | **soft agora** (E07: política imposta por operação, sem jail); **jail real depois** (E17: bwrap + Landlock + seccomp) |
| OA6 | Momento do WASM em plugins | E11 (futuro) | **não no MVP**; o modelo de capacidades entra na política (E02); runtime de plugins deferido |
| OA7 | Momento do adaptador MCP | E08 | **deferido**; fora do escopo atual (`00b` §7) |
| OA8 | Semântica de commit na porta `Memory`: `session_end` vs método explícito | E03 | **`record`/`commit` explícito** (mapeia `write`/`update`/`task::submit`/`commit`); `session_end` = finalização/`sync`, não persistência de notas |
| OA9 | Trade-off latência × compressão no provider built-in | E12 | **latência primeiro** (`opencode go/zen`), compressão off-path; orçamento medido em E12-T07 (negativo visível) |
| OA10 | `llama.cpp`: in-process (FFI) vs `llama-server` HTTP | E12 | **L1 HTTP primeiro**; in-process (L2) só se a medição (E12-T07) justificar; FFI confinada atrás de feature |
| OA11 | Lixeira (`trash`) como tool própria vs efeito de `write`/`rm` | E06 | **tool própria** `trash` → `.katu/trash` (recuperável); `bash rm` continua possível, mas a política prefere `trash` |
| OA12 | Quem aciona modelo/grau de pensamento e mapeamento por provider | E12 | **utilizador** aciona (`set_model`/`set_thinking`); mapeamento por dialeto (effort/budget/tokens); agente **não** auto-escala |
| OA13 | (Futuro E17) `bwrap` (binário externo) vs namespaces em Rust puro | E17 | **bwrap validado** (como o `ai-jail`): menos código sensível, correções upstream; Rust puro rejeitado |
| OA14 | Extensões por **código** (Lua / `cdylib` Rust / WASM) vs regras **só-dados** | E11 (futuro) | **dados puros agora**; código só como plugin **WASM/out-of-process** com capacidades; Lua/Rust in-process **rejeitados** (DSL + determinismo + G7) |
| OA15 | Semântica de `Deny*` e modelação das operações de memória | E02/E04 | regras `DenyWrite`/`DenyRead`/`DenyDelete`/`DenyCommand` são **portas falha-fechado**: disparam salvo se existir a `Capability` correspondente (`WritePath`/`ReadPath`/`DeletePath`/`Command`); `Capability::Workspace` é o grant **implícito** da raiz (v2, ADR 0003), mas **não** destranca `DenySensitiveRead` (`.ssh`/`.env`), que só cede a `ReadPath` explícito; as operações de memória são **comandos nominais** (`memory_recall`/`memory_write`/`memory_outcome`/`memory_close`) e o adaptador (E03) concede a capacidade quando o `pre_write` passa |
| OA16 | Escrita otimista (compare-and-swap) na porta `Fs` para ficheiros editáveis por humano | E06 (E01 se antecipado) | **aceite — `Fs::write_atomic_if` (E01-T02)**: só grava se o conteúdo atual casar com o esperado (senão `FsError::Stale`, recuperável); torna `edit`/read-before-write TOCTOU-safe e evita clobber de edição concorrente. Ideia do `write_atomic(text, expected)` do cordis (research) |
| OA17 | Modos de despacho `bail`/`serial` no event bus | E12 / E18-T09 / E07 | **só quando houver consumidor real**: `bail` = primeiro resultado decisivo vence (cadeias de fallback de provider, fusão de canais, guardas de contenção). O bus tem hoje `emit` + `waterfall`; **não** acrescentar por antecipação |
| OA18 | Filtro de nível de instrumentação por subsistema | E19 / E15 | **prefixo/`name`**: ligar só um subsistema (ex.: `provider.*`) além do nível global, mantendo o custo zero por defeito. Ideia dos `exporter.levels[name]` do cordis (research) |
| OA19 | Erros de validação com percurso (*issue path*) | E06 / E09 | **`Issue { path, message }`** agregado por validador (checkpoint, política, schemas de tool) em vez de `String`; alimenta "erros que ensinam" (E06-T02) e o diagnóstico estruturado. Ideia do `ValidationError` do cordis (research) |
