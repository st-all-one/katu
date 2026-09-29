# 00b — Objetivos e princípios

> **Documento de orientação.** Define o que o katu **é** e o que **nunca** será. Toda decisão
> posterior é avaliada contra esta página. Em caso de conflito, os objetivos vencem a conveniência.
>
> Complementa [`00-tese-e-escopo.md`](00-tese-e-escopo.md) (tese e fronteira) e
> [`01-decisoes-fundacionais.md`](01-decisoes-fundacionais.md) (decisões congeladas).

---

## 1. Definição

> **O katu é um kernel agêntico mínimo, focado em código, que possui apenas o necessário para ser
> útil — ler, escrever, executar, pesquisar ficheiros e planejar — com o knudge integrado como
> memória do agente. É estrito, orientado a guardrails determinísticos e otimizado para tokens.
> Tem um CLI e um TUI, e só.**

---

## 2. Os objetivos (G)

| # | Objetivo | Como se verifica |
|---|---|---|
| **G1** | **Kernel mínimo.** Núcleo pequeno, puro e possuído — a política é o kernel, não um acessório. | `xtask check-surface`; ficheiros ≤ 300 linhas; núcleo sem dependências de provider |
| **G2** | **Foco em código.** Tudo serve o fluxo de editar, executar e verificar código. Sem features laterais. | Toda capacidade nova passa o filtro do §4 |
| **G3** | **Conjunto mínimo de capacidades:** escrever, ler, executar, pesquisar ficheiros e planejar. Nada mais. | A superfície de tools é exatamente essas 5 famílias; extras são `deferred` explícitos |
| **G4** | **knudge integrado como memória.** O knudge **não** é plugin opcional: é a memória do agente, in-process. | E03 (porta + adaptador in-process); `Memory` nunca desligada em produção |
| **G5** | **Guardrails determinísticos e estritos.** Regras avaliadas sobre factos, com bloqueio duro e evidência. | E02/E05; nenhuma regra `Enforced` sem teste pelo caminho real |
| **G6** | **Otimizado para tokens.** Só o delta chega ao modelo; orçamento de contexto; medir o custo por turno. | E09 (orçamento) + E15 (medição); `Metric` com base tipada |
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
| Web, desktop, Electron, mobile | Fora do fluxo de codificação no terminal. |
| Plugins/WASM | O **modelo** de capacidades entra; o runtime de plugins é futuro (§12). |
| Múltiplos providers, inferência local, voz | Amplitude é commodity; no MVP, **um** provider (§13). |
| Compressão de contexto no núcleo | Porta desligada por defeito, com recuperação obrigatória (§61). |
| Servir de "mais um agente de codificação" genérico | Sem o knudge integrado, o katu não se justifica (§53). |

---

## 4. O filtro (como decidir se algo entra)

Uma capacidade/feature só entra se responder **sim** a todas:

1. Serve diretamente um dos cinco mínimos (G3)?
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
katu-memory
   └── adapter in-process  → knudge-core        (PRIMÁRIO, no escopo)
   └── adapter mcp         → knudge-mcp / kd    (FUTURO, fora do escopo)
```

**Regra:** o adaptador MCP não é construído agora. Ele existe como porta desenhada e como teste
de sanidade ("se voltarmos ao MCP, só o adaptador muda"). Se for construído no futuro, é uma
decisão registada — nunca uma dependência silenciosa que quebra G7.
