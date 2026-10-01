# 00 — Tese e escopo

> **Fase 0 (conceitual).** Antes de qualquer código: fixar o que o katu é, o que não é, e por
> que ele existe. Este documento é a fronteira do projeto. Os **objetivos** (G1–G9) e o filtro de
> escopo vivem em [`00b-objetivos.md`](00b-objetivos.md).
>
> Base: [`proposal/katu-brainstorm-decisoes.md`](../brainstorm/katu-brainstorm-decisoes.md) §0–§13,
> §20–§22, §53, §66.

---

## 1. A pergunta que o katu responde

O knudge (memória durável por projeto) depende, hoje, de **disciplina do modelo**: chamar
`kd ask` antes de gravar, ancorar a nota ao código, dar evidência ao fechar tarefa. Na prática,
**agentes esquecem**; os gatilhos MCP mitigam, mas continuam a ser **sugestão** (§10).

O katu é a peça que falta: um runtime que **torne impossível** violar o protocolo de memória.
Não "peça ao modelo para buscar antes de gravar" — **não deixe a escrita passar sem a busca**.

> **Tese:** o **knudge** define o que é boa memória; o **katu** é a metade que garante que o
> agente a cumpra.

---

## 2. O que é tese e o que é commodity

Regra de decisão (§2): o katu **possui** a tese; **depende** do commodity.

| Commodity (custoso, genérico, não diferencia — reaproveitar) | Tese (onde se ganha — construir e possuir) |
|---|---|
| Adaptadores de provider e wire formats | **Motor de política com bloqueio duro em runtime + auditoria** |
| Registo canônico de modelos (pricing, limites) | **Regras e capacidades como dados** (a política, E02) |
| Compaction/sumarização de contexto | **Execução isolada de verdade** (sandbox, não só "hook nega") |
| Plumbing de MCP (client stdio/HTTP) | **Hot path de performance** (startup, tempo por turno) |
| Convenções de sessão/persistência | **UX/TUI de codificação** desenhada para o fluxo |
| Motor de memória (o knudge **é** o motor) | **Enforcement do protocolo de memória** (o recorte que justifica o katu) |

**Corolário:** o katu **não** reimplementa o motor de memória. Ele é **consumidor de referência**
do knudge, atrás da porta `Memory` (§12). Não fundir os repositórios: o knudge vale por ser
agnóstico (§11).

**Providers:** os *built-in* (possuídos) são o **gateway `opencode go/zen`** (hot path) e o
**`llama.cpp`** local (opcional), no mesmo trait e ambos **endpoints de modelo** — **não** o
agente OpenCode v2. Todos os demais entram pelo **GDK/declarativo** (commodity) ou são
**ativamente ignorados**. A otimização de comunicação do caminho built-in pertence à coluna
**tese** — latência antes de compressão ([`13-providers.md`](13-providers.md) E12).

---

## 3. Dentro de escopo (o produto)

1. **Kernel possuído** — máquina de estados do agente, não camada de regras sobre um substrato
   (§48, §51.1). O pi/goose podem continuar a existir; o katu não depende de os remendar.
2. **Política determinística** — avalia factos tipados (`ToolUse{name, args, resolved_paths,
   argv}`, `Phase`, `Budget`), devolvendo `Allow | Deny{reason, evidence} | RequireApproval |
   NeedsHuman`; nunca `Vec<Regex>` sobre texto (§51.4).
3. **Capacidades, não denylists** — `Capability::{ReadPath, WritePath, DeletePath, Exec, Net,
   SpawnPty, McpSession}`; caminhos canonicalizados antes de decidir (§51.4).
4. **Contenção determinística (soft)** — no MVP **sem jail de SO**: o katu corre global, como o
   utilizador; as travas determinísticas (capacidades, `RequireApproval`) limitam sem prometer
   fronteira de segurança. A jail real (bwrap/Landlock) é **futura** (§51.5, §43,
   [`18`](18-jail-futuro.md)).
5. **Conjunto mínimo de capacidades** — o **core** de [`00b`](00b-objetivos.md) §1.1:
   ler/escrever/editar/lixeira, executar, pesquisar, registrar memória, planejar, compactar e
   escolher modelo/grau de pensamento; nada além disso (G3). Superfície de tools fechada e medida.
6. **knudge integrado como memória** — porta `Memory` com tipos do katu e adaptador **in-process**
   (`knudge-core`) como **primário**; o adaptador MCP é futuro e fora do escopo atual (§12, §13,
   [`00b`](00b-objetivos.md) §7).
7. **Guardrails determinísticos e estritos** — regras sobre factos, bloqueio duro, evidência
   auditável (DF2, DF3).
8. **Otimizado para tokens** — só o delta chega ao modelo; orçamento de contexto; custo por turno
   medido com base tipada (§18, §62). No caminho built-in de provider (`opencode go/zen`), a
   **latência de comunicação precede a compressão** (E12).
9. **Duas superfícies e só** — CLI (`katu`) e TUI (`katu-tui`); nenhum servidor, protocolo de
   rede ou endpoint (G7).
10. **Evidência tipada** — todo número viaja com a sua base; o agente não pode fabricar as
    métricas que descrevem o seu desempenho (§62).
11. **UX de codificação** — TUI própria, panic-safe, que mostra `blocked`/`needs_human` com
    evidência e `override_reason` (§32).

## 4. Fora de escopo (e por quê)

| Fora | Porquê | Fonte |
|---|---|---|
| Framework de plugins geral / contentor de DI | O `dsh` pagou-o com 316 pacotes e dois postmortems do próprio carregador | §42, §45, §66.4 |
| WASM obrigatório no MVP | Custo real (enum de 10 versões × ~25 métodos, `wasi-sdk`); o **modelo** de capacidades entra já, o runtime pode esperar | §58–§59 |
| Compressão **inline no hot path** | A compactação é capacidade do core (§1.1 #10), mas corre **off hot path**, como porta com recuperação | §61, §64 |
| Motor de memória dentro do katu | O knudge é o motor; fundir mata o agnosticismo | §11–§13 |
| Servidor, multi-utilizador, daemon | O arags é a prova de que a plataforma maior que o agente é negativa líquida | §20 |
| DSL de configuração / expressões avaliadas | Postmortem 0002 do `dsh` (`!!js`): risco de correção e segurança | §44–§45 |
| Cobertura 100% como meta | Postmortem 0001 do `dsh`: 178 testes verdes com o produto quebrado | §44 |
| Métricas de output como prova de capacidade | A JetBrains mediu 8,5% menos tokens com qualidade igual — bom, não é prova de agente melhor | §60 |
| "Modo sem isolamento" apresentado como capacidade privilegiada | No MVP a contenção é **soft** por omissão, declarada — nunca um `--host` com banner a vender um privilégio | §49, §52 |
| **MCP no escopo atual** | É ideia **futura**: o knudge é integrado in-process; a porta `Memory` mantém a opção aberta | §13, `00b` §7 |
| Servidor / daemon / superfícies além de CLI+TUI | O arags provou que a plataforma maior que o agente é negativa líquida; o katu tem um binário e uma TUI | §20, G7 |
| Jail de SO real no MVP | Adiada como feature **futura** pós-MVP (contenção soft agora) | E17, [`18`](18-jail-futuro.md) |

---

## 5. Modelo mental em três camadas

```
katu-policy   ← TESE: regras, factos, capacidades, veredictos, evidência, auditoria
katu-core     ← kernel: máquina de estados, event log, sessão; PORTA Memory (tipos do katu)
katu-tools    ← write/read/edit/trash/exec/search + planning + contenção soft (fail-closed)
katu-providers← built-in `opencode go/zen` + `llama.cpp` (hot path) + GDK/declarativo (demais)
katu-tui      ← UX de codificação
katu (bin)    ← CLI + composição + adaptador in-process do knudge
```

**Firewall LLM-free** (§21): `katu-core`, `katu-policy` e `katu-tools` **não podem** depender de
crates de provider. O modelo é cliente do plano de dados, não parte dele.

---

## 6. As nove decisões fundacionais

Estão detalhadas (com evidência e teste que as trava) em
[`01-decisoes-fundacionais.md`](01-decisoes-fundacionais.md). Resumo:

| # | Decisão | Frase |
|---|---|---|
| DF1 | **O katu possui o loop** | Máquina de estados com pré-condições tipadas, não regras sobre substrato |
| DF2 | **A política avalia factos, não texto** | Capacidades explícitas, caminhos canonicalizados, argv resolvido |
| DF3 | **Toda regra declara a sua categoria** | `Enforced{rule_id}` \| `Advisory{rationale}` \| `Perception` |
| DF4 | **Fail-closed em todas as fronteiras** | Contenção **soft** relatada, recuo para o original, `unavailable` nega |
| DF5 | **A evidência viaja com o número** | Bases tipadas, `unpriced` ≠ zero, negativos visíveis |
| DF6 | **Uma capacidade, um provedor** | Porta `Memory` com tipos do katu; sem legado em paralelo |
| DF7 | **Conhecimento e política são artefactos** | Versionados e gate-verificados; ADRs com alternativas |
| DF8 | **O provider built-in é first-party e é um endpoint de modelo** | `opencode go/zen` + `llama.cpp`; resto GDK; latência > compressão; o modelo é cliente, não substrato |
| DF9 | **Instrumentação transversal on-demand** | Logs sempre estruturados + métrica de tempo; custo zero por defeito (`feature = "instrument"`) |

---

## 7. Critérios de sucesso e de parada

**Sucesso do MVK** (E05) — ver [`README.md`](../../../README.md) §5. Em uma frase: o agente **não
consegue** violar o protocolo de memória, e isso é provado pelo caminho real.

**Kill / pivot criteria** (detalhados em [`17-roadmap-riscos.md`](17-roadmap-riscos.md)):

- As regras de memória **não** são todas expressáveis no modelo de política → o substrato ainda
  decide; reconsiderar (talvez o problema seja do knudge, não do loop).
- O atrito medido do enforcement é maior que o benefício percebido face a `pi + knudge-mcp` →
  investir no knudge e na integração, não no katu.
- A superfície cresce para além do teto sem capacidade de a governar → parar e podar (lição do
  arags §20 e do maxima §51.13).

---

## 8. Não-objetivos deste documento

Não descreve **como** construir (isso são os épicos `02`–`13`), nem **as decisões fechadas**
(`01`), nem **o cronograma** (`17`). Aqui fixa-se apenas a fronteira.
