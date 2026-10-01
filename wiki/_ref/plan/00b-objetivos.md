# 00b — Objetivos e princípios

> **Documento de orientação.** Define o que o katu **é** e o que **nunca** será. Toda decisão
> posterior é avaliada contra esta página. Em caso de conflito, os objetivos vencem a conveniência.
>
> Complementa [`00-tese-e-escopo.md`](00-tese-e-escopo.md) (tese e fronteira) e
> [`01-decisoes-fundacionais.md`](01-decisoes-fundacionais.md) (decisões congeladas).

---

## 1. Definição

> **O katu é um kernel agêntico mínimo, focado em código, que possui apenas o necessário para ser
> útil — ler, escrever, editar, mover para a lixeira do projeto (`.katu/trash`), executar com as
> permissões do utilizador que o evocou, pesquisar ficheiros otimizadamente, registrar memória
> (knudge), planejar, compactar a conversa e escolher o modelo e o grau de pensamento — com o
> knudge integrado como memória do agente. É estrito, orientado a guardrails determinísticos e
> otimizado para tokens. Tem um CLI e um TUI, e só.**

### 1.1 O core (superfície exata)

Este é o conjunto **completo** do MVP. Nada além disto entra sem passar o filtro do §4.

| # | Capacidade | Forma | Onde |
|---|---|---|---|
| 1 | **Ler** ficheiros | tool `read` (views `outline`/`summary`/`symbol`/`diff`/`full`) | E06-T03 |
| 2 | **Escrever** ficheiros | tool `write` (ficheiros novos; existentes via `edit`) | E06-T03 |
| 3 | **Editar** ficheiros | tool `edit` (patch `old`→`new`, `dry-run`, hash) | E06-T03 |
| 4 | **Mover/renomear** ficheiros | tool `move` (atómico; invalida índice/cache) | E06-T11 |
| 5 | **Mover para a lixeira** do projeto | tool `trash` → `.katu/trash` (recuperável) | E06-T09 |
| 6 | **Executar** comandos | tool `bash`, **com as permissões do utilizador que evocou o processo** (nunca eleva) | E06-T04, E07 |
| 7 | **Pesquisar** ficheiros otimizadamente | tools `grep`/`find`/`ls` (respeitam ignore, streaming, só o delta) | E06-T05 |
| 8 | **Registrar memória** (knudge) | hooks/fases (**prioritários**) + tool `memory` policy-gated (pedido explícito) | E03, E05, E06-T10 |
| 9 | **Planejar** | tool `plan` (artefacto de fase) | E06-T06 |
| 10 | **Compactar a conversa** | comando/porta `katu-context` (off hot path, com recuperação) | E09-T07 |
| 11 | **Alterar modelo e grau de pensamento** | controlo de runtime (`set_model` / `set_thinking`) | E12-T10 |

**Distinção:** 1–7 e 9 são **tools** do modelo (superfície fechada, E06), com **envelope tipado** e
formato **TOON** ao modelo (JSON como alternativa) — DF12. 8, 10 e 11 são **capacidades do kernel**
(portas/controlos) — o modelo **não** ganha superfície nova por causa delas (a tool `memory` de #8
é a exceção, policy-gated e secundária aos hooks/fases).

**Fronteira de execução (MVP).** **Não há jail de SO ativo**: o katu é **global de facto** e corre
como o utilizador que o evocou. A limitação vem das **travas determinísticas** (caminhos
canonicalizados, capacidades, `RequireApproval`/`NeedsHuman`) — barreira **soft**, declarada, que
**não** é fronteira de segurança. A jail real é feature **futura** (E17, [`18`](18-jail-futuro.md)).

**A única parte que impõe fluxo é a máquina de estados** (E04). Todas as transições têm caminhos
explícitos de `waiver`/`Refusal` — o katu não engessa o utilizador nem o modelo com muros
silenciosos; saltar uma fase é uma decisão declarada e registada.

---

## 2. Os objetivos (G)

| # | Objetivo | Como se verifica |
|---|---|---|
| **G1** | **Kernel mínimo.** Núcleo pequeno, puro e possuído — a política é o kernel, não um acessório. | `xtask check-surface`; ficheiros ≤ 400 linhas (gate `file-length`); núcleo sem dependências de provider |
| **G2** | **Foco em código.** Tudo serve o fluxo de editar, executar e verificar código. Sem features laterais. | Toda capacidade nova passa o filtro do §4 |
| **G3** | **Conjunto mínimo de capacidades:** o **core** do §1.1 — ler/escrever/editar/mover/lixeira, executar, pesquisar, memória, planejar, compactar, modelo/pensamento. Nada mais. | A superfície de **tools** são as famílias fechadas do §1.1; memória/compaction/modelo são **controlos do kernel**; extras são `deferred` explícitos |
| **G4** | **knudge integrado como memória.** O knudge **não** é plugin opcional: é a memória do agente, in-process. | E03 (porta + adaptador in-process); `Memory` nunca desligada em produção |
| **G5** | **Guardrails determinísticos e estritos.** Regras avaliadas sobre factos, com bloqueio duro e evidência. | E02/E05; nenhuma regra `Enforced` sem teste pelo caminho real |
| **G6** | **Otimizado para tokens.** Só o delta chega ao modelo; orçamento de contexto; medir o custo por turno. | E09 (orçamento) + E15 (medição); `Metric` com base tipada. No caminho built-in (`opencode go/zen`), latência precede compressão (E12) |
| **G7** | **Duas superfícies e só: CLI e TUI.** Nenhum servidor, nenhum protocolo de rede, nenhum endpoint. | `katu` (CLI) + `katu-tui`; nenhuma crate de servidor/RPC no grafo |
| **G8** | **Fortemente testado e determinístico.** Nada de `SystemTime::now()` no hot path; ordem canônica; replay. | E13 (caminho real + guards invertidos); Miri/geiger; proptest |
| **G9** | **Fiel à filosofia do knudge; evitar os erros do arags.** Núcleo puro + portas, artefacto-texto como verdade, sem plataforma. | §5 abaixo; `check-surface`; nenhuma dualidade de backend |

---

## 3. Não-objetivos (explícitos)

| Fora | Porquê |
|---|---|
| **MCP no escopo atual** | É uma **ideia futura**, opcional. O katu integra o knudge in-process; a porta `Memory` mantém a opção aberta, mas nada de MCP agora (§7). |
| Servidor, daemon, RPC, rede | O arags provou que a plataforma maior que o agente é negativa líquida (§20). |
| Multi-utilizador, auth, papéis, quórum | Camada social antes do segundo utilizador. |
| Web, desktop, Electron, mobile, voz, imagem, browser | Fora do fluxo de codificação no terminal (G2/G7). |
| Plugins/WASM | O **modelo** de capacidades já entra na política (E02); o plugin host/ABI é **futuro**, fora do plano principal (E11, [`12`](12-plugins-e-abi.md)). |
| Reimplementar providers, OAuth, gateways de plataforma | Os built-in são só o gateway `opencode go/zen` e o `llama.cpp` local; os demais vêm do **GDK/declarativo** ou são **ativamente ignorados** (§13). |
| Compressão **inline no hot path** | A compactação é um controlo do core (§1.1 #10), mas corre **off hot path**, como porta com recuperação obrigatória (§61). |
| Jail de SO real no MVP | É feature **futura** (E17), pós-MVP; no MVP a contenção é **soft** e declarada (E07). |
| Servir de "mais um agente de codificação" genérico | Sem o knudge integrado, o katu não se justifica (§53). |

---

## 4. O filtro (como decidir se algo entra)

Uma capacidade/feature só entra se responder **sim** a todas:

1. Serve diretamente um item do **core** (G3/§1.1)?
2. Aumenta a utilidade para **código** (G2)?
3. Pode ser determinística e testada pelo caminho real (G8)?
4. Não abre uma nova superfície fora de CLI+TUI (G7)?
5. O seu custo de superfície é justificado por um consumidor **atual** (anti-YAGNI, §45.19)?

Se qualquer resposta for "não", fica **`deferred` com razão registada** — nunca entra "porque é
bonito". A lista de `deferred` é auditável por `xtask check-surface`.

---

## 5. Inspiração: knudge (copiar) × arags (evitar)

| Copiar do **knudge** | Evitar do **arags** |
|---|---|
| Núcleo puro + portas (`Clock`/`Rng`/`Fs`/`Env`), 4 dependências | Plataforma maior que o agente; 9 crates, 30 tabelas, 28 RPCs |
| A nota é a verdade; o índice é derivado | Server-first: daemon long-running, Docker, tokens, TLS |
| Determinismo obrigatório: ordem canônica, `total_cmp`, sem RNG no scoring | 4 espaços de conhecimento × 4 motores vetoriais replicados |
| Funções puras no core, testáveis isoladamente (`matematica.md`) | RLM recursivo e orquestração acoplada a uma ferramenta de memória |
| `propose, never act`; `forget`/`restore` reversíveis | Auth/quórum/review gates antes do segundo utilizador |
| Binário único, sem servidor/DB/daemon | Config em 3 ficheiros e dezenas de knobs |
| Embeddability como decisão de desenho (D65/D68) | "Nova capacidade = replicar o molde inteiro" (complexidade multiplicativa) |

**Regra de ouro:** quando houver dúvida entre "mais uma capacidade" e "manter o kernel mínimo",
**ganha o kernel mínimo** (a lição do §20 e do §51.13).

---

## 6. Consequências no plano

| Objetivo | Onde se materializa |
|---|---|
| G1/G2/G3 | [`02-fundacao.md`](02-fundacao.md), [`07-tools-e-capacidades.md`](07-tools-e-capacidades.md) |
| G4 | [`04-contrato-da-porta-memory.md`](04-contrato-da-porta-memory.md) (adaptador in-process **primário**; MCP futuro em [`09`](09-adaptador-knudge.md)) |
| G5 | [`03-contrato-de-politica.md`](03-contrato-de-politica.md), [`06-mvk-enforcement-memoria.md`](06-mvk-enforcement-memoria.md) |
| G6 | [`10-contexto-checkpoint-evidencia.md`](10-contexto-checkpoint-evidencia.md), [`16-performance-benchmarks.md`](16-performance-benchmarks.md) |
| G7 | [`11-tui-e-ux.md`](11-tui-e-ux.md), [`02-fundacao.md`](02-fundacao.md) |
| G8 | [`14-testes-e-qualidade.md`](14-testes-e-qualidade.md) |
| G9 | [`15-governanca-superficie.md`](15-governanca-superficie.md), este documento |

---

## 7. A porta `Memory` e o futuro MCP

O knudge é integrado **in-process** (linka `knudge-core`) — é a memória do agente, não um
sidecar. A porta `Memory` com **tipos do katu** mantém a substituibilidade (DF6):

```
katu-core::memory (porta, tipos do katu)
   └── adapter in-process  → knudge-core        (PRIMÁRIO, no escopo; vive no binário `katu`)
   └── adapter mcp         → knudge-mcp / kd    (FUTURO, fora do escopo)
```

**Regra:** o adaptador MCP não é construído agora. Ele existe como porta desenhada e como teste
de sanidade ("se voltarmos ao MCP, só o adaptador muda"). Se for construído no futuro, é uma
decisão registada — nunca uma dependência silenciosa que quebra G7.
