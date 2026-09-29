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

---

## Decisões em aberto (a resolver no épico indicado, nunca em prosa)

| # | Ponto | Épico | Default provisório |
|---|---|---|---|
| OA1 | Persistência: JSONL append-only vs SQLite | E04 | **JSONL append-only** (o log é a verdade; índice derivado) |
| OA2 | Semântica de negação: erro recuperável vs parada dura | E02 | **ambos**: `Denied` recuperável por default; `NeedsHuman` para irreversível |
| OA3 | TUI: `crossterm` cru vs `ratatui` | E10 | **`ratatui` 0.30 + `crossterm` 0.29** (skill [`ratatui-tui`](../.agents/skill/ratatui-tui/SKILL.md): estado central, render puro, executor async); rever antes de E10-T01 |
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
