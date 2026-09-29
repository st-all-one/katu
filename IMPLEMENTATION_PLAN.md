# IMPLEMENTATION_PLAN — Ordem otimizada de implementação

> Documento **operacional**: dá a **ordem** de execução do [`plan/`](plan/) (E01–E18) — o que é
> crítico, o que é paralelo, o que é **from-zero** (não pode ser adiado) e onde estão os **gates**.
> Não substitui os épicos: cada passo aponta para o ficheiro que o detalha.
>
> **Regras de sequência**
> 1. Um épico só arranca quando **todas** as suas dependências fecham (grafo §2).
> 2. Dentro de uma onda, as *lanes* correm em **paralelo**.
> 3. Um **gate** que falha **para** a onda seguinte (não se "contorna").
> 4. O que está marcado **`✂`** é **corta-primeiro** se o orçamento apertar (nunca antes do MVK).
>
> **Fonte de verdade:** decisões em [`plan/01`](plan/01-decisoes-fundacionais.md); escopo/core em
> [`plan/00b`](plan/00b-objetivos.md) §1.1; riscos/kill criteria em
> [`plan/17`](plan/17-roadmap-riscos.md).

---

## 1. Invariantes from-zero (decidir/projetar já, não "otimizar depois")

São escolhas **arquiteturais** que, adiadas, viram retrabalho. Entram no *design* das ondas abaixo:

| Item | Onde entra | Porquê from-zero |
|---|---|---|
| **Contrato de determinismo numérico** (E18-T01) | design de E02/E04 | `evaluate` puro e replay dependem dele; retrofit contaminaria todo o cálculo |
| **Estado com partilha estrutural + snapshot+Δ** (E18-T05) | E04-T03 | o log/snapshot nascem assim; converter depois é reescrita |
| **Confiança Beta-Bernoulli + Wilson** (E18-T06) | E02-T04 | define `Enforced` vs `Advisory` por evidência, não por fé |
| **Cargo profiles/lints/workspace** | E01 | `[profile.release]`, `[workspace.lints]`, resolver 2, 6 crates + `xtask` |
| **Harness de medição (measure-first)** | E15-T01/T02 ∥ E01 | o exemplo 19 começou pelo harness; sem baseline não há A/B |
| **Firewall LLM-free** (`katu-core`/`policy`/`tools` sem provider) | E01 | `xtask check-layers` desde o primeiro commit |

---

## 2. Grafo de dependências

```
E01 ──┬── E02 ──┐
      └── E03 ──┴── E04 ── E05 ══ GATE DE DECISÃO ══╗
                                  │                  ║
                                  ├── E06 ── E07      ║
                                  ├── E09 ── E10      ║
                                  ├── E12             ║
                                  └── E18 (F2/F3/F4/F8/F9)  ║
                                                     ╠══ E13, E14, E15, E18 (transversais)
                                                     ╚══ E16 (revisão por fase)

Futuro (só após valor provado): E08 MCP · E11 plugins WASM · E17 jail de SO
```

**Caminho crítico:** `E01 → E02 → E04 → E05`. Tudo o resto é paralelizável depois do gate.

---

## 3. Ondas (a ordem)

| Onda | Foco | Lanes (paralelas) | Gate de saída |
|---|---|---|---|
| **0** | Fundação + instrumentação | **E01** · E13(scaffold) · E14(scaffold) · E15-T01/T02 · E18-T01 · E16 | `make check` verde em 1.97.0 |
| **1** | Contratos da tese | **E02** · E03 · (E18-T06 hook) | regras do knudge expressáveis; porta substituível |
| **2** | Núcleo | **E04** (+E18-T05) · E13(replay) | replay determinístico |
| **3** | **MVK** | **E05** | **GATE DE DECISÃO** (passa ou para) |
| **4** | Capacidades | E06 · E07 · E18-T08 `✂` · E13/E14 | negação pelo executor; soft honesto |
| **5** | Contexto + Providers | E09 (+E18-T02/T03/T09) · E12 (+E18-T04) · E18-T06/T07 · E15 | artefactos validados; firewall intacta |
| **6** | UX | E10 | panic-safe + evidência no ecrã |
| **7** | Endurecer + fechar | E13 · E14 · E15 · E18-T10 (+T07 `✂`) · E16 | números com artefacto; teto de superfície |

---

## 4. Detalhe por onda

### Onda 0 — Fundação e instrumentação (paralelo)

- **E01** ([`02`](plan/02-fundacao.md)) — *crítico*: workspace de **6 crates + `xtask`**, MSRV
  1.97.0, ports, `[profile.release]` (lto=fat, codegen-units=1, panic=abort), `[workspace.lints]`,
  `check-layers`, `check-crate-coverage`.
- **E13** ([`14`](plan/14-testes-e-qualidade.md)) — scaffold de testes (proptest, goldens, harness de
  mútuo). Cresce com o código; nasce agora.
- **E14** ([`15`](plan/15-governanca-superficie.md)) — scaffold de governança: ADRs com
  `Alternatives considered`, `policy/` versionado, `check-docs`.
- **E15-T01/T02** ([`16`](plan/16-performance-benchmarks.md)) — harness e **portão de publicação de
  números** (DF5). **Mede antes de otimizar.**
- **E18-T01** ([`19`](plan/19-otimizacao-profunda.md)) — **contrato de determinismo numérico**:
  entra como restrição de design de E02/E04.
- **E16** ([`17`](plan/17-roadmap-riscos.md)) — contínuo (riscos, kill criteria).

**Saída:** build reprodutível, lints verdes, harness a correr, contrato numérico escrito.

### Onda 1 — Contratos da tese

- **E02** ([`03`](plan/03-contrato-de-politica.md)) — *crítico*: `ToolUse`, `Capability`, `Rule`,
  `Enforcement`, `Decision`; `evaluate` **puro**; **vocabulário fechado e versionado**
  (`POLICY_VOCAB_VERSION`). Aqui liga **E18-T06** (Wilson → `Enforced`/`Advisory`).
- **E03** ([`04`](plan/04-contrato-da-porta-memory.md)) — *paralelo*: porta `Memory` em
  `katu-core` + adaptador in-process do knudge no binário (`KnudgeBuilder`, D214).
- **E13/E14** — goldens de veredicto; ledger de regras.

**Gate:** as regras do protocolo de memória (dedup ≥ 0.92, âncora, `outcome` antes de fechar) são
**todas** expressáveis; `E03-T06` prova a substituibilidade da porta.

### Onda 2 — Núcleo

- **E04** ([`05`](plan/05-kernel-loop.md)) — *crítico*: máquina de estados, `Event`, log
  append-only, `derive_messages`/`snapshot`. Aplica **E18-T05** (estado persistente, snapshot+Δ).
- **E13** — testes de replay e transições ilegais.

**Gate:** replay byte-a-byte; `Model-visible ⟺ logged` verificado em runtime.

### Onda 3 — MVK (o gate que decide o projeto)

- **E05** ([`06`](plan/06-mvk-enforcement-memoria.md)) — enforcement da memória sobre o kernel.
- **GATE DE DECISÃO:** se o MVK **não** prova a tese (regras inexpressáveis, atrito > benefício),
  **para-se** e reconsidera-se ([`17`](plan/17-roadmap-riscos.md) §5). Nada abaixo disto arranca
  antes deste ponto.

### Onda 4 — Capacidades (paralelo)

- **E06** ([`07`](plan/07-tools-e-capacidades.md)) — write/read/edit/trash/exec/search + planning;
  `ToolOutcome`; `Capability`. Inclui `trash` (`.katu/trash`) e a tool `memory` (policy-gated).
- **E07** ([`08`](plan/08-sandbox-fail-closed.md)) — contenção **soft** determinística; controlo em
  falta = recusa; honestidade soft ≠ segurança (jail é E17/futuro).
- **E18-T08** `✂` ([`19`](plan/19-otimizacao-profunda.md)) — PERT/CPM sobre o DAG do plano (com E06).

### Onda 5 — Contexto e Providers (paralelo)

- **E09** ([`10`](plan/10-contexto-checkpoint-evidencia.md)) — `assemble`/`compact`, checkpoint
  tipado, gate de verificação, `Metric`, cost governor. Aplica:
  - **E18-T02** (contexto = mochila submodular + MMR),
  - **E18-T03** (compactação por entropia/surprisal + JS),
  - **E18-T09** (fusão de canais RRF/PPR).
- **E12** ([`13`](plan/13-providers.md)) — built-in `opencode go/zen` + `llama.cpp`; demais via GDK.
  Aplica **E18-T04** (filas/hedging/prefix-cache; TTFT com artefacto — E12-T07).
- **E18-T06/T07** — confiança por artefacto e deteção de anomalia (CUSUM/SPRT `✂`).
- **E15** — benchmark do caminho real.

**Gate:** nenhum número sem base; firewall LLM-free intacta.

### Onda 6 — UX

- **E10** ([`11`](plan/11-tui-e-ux.md)) — TUI (ratatui 0.30 + crossterm 0.29), panic-safe,
  evidência no ecrã, controlos do core (modelo/thinking, compactar, trash).

### Onda 7 — Endurecer, medir e fechar (paralelo)

- **E13/E14** — fecho: cobertura de regras, teto de superfície, `check-docs`.
- **E15** — fecho: IC 95 %, ≥ 3 repetições, `dhat`; negativos visíveis.
- **E18-T10** ([`19`](plan/19-otimizacao-profunda.md)) — harness estatístico + gate de regressão.
- **E18-T07** `✂` — anomalia/drift, **só se o falso-positivo medido pagar**.
- **E16** — revisão por fase.

---

## 5. Gates (resumo)

| # | Gate | Evidência |
|---|---|---|
| 0 | `make check` verde em 1.97.0 | job `msrv` |
| 1 | regras de memória todas expressáveis; porta substituível | `policy/memory.toml` + E02-T07 + E03-T06 |
| 2 | **MVK passa** (ou para) | E05-T07 + relatório de atrito |
| 3 | negação pelo executor; soft honesto | E06-T07 + E07-T03 |
| 4 | artefactos validados; token medido | E09-T05 + E15-T02 |
| 5 | firewall LLM-free intacta | `xtask check-layers` |
| 6 | nenhuma otimização sem artefacto; adotar-ou-reverter | E18-T10 + `xtask gate:bench` |

---

## 6. Corta-primeiro (`✂`)

Se o orçamento apertar, cortam-se **nesta ordem** (nunca antes do MVK):

1. **E18-T08** (PERT/CPM) — conveniência de priorização, não tese.
2. **E18-T07** (anomalia CUSUM/SPRT) — só entra com falso-positivo medido.
3. **E15-T05** (instrumentação do prefixo) — já marcada `✂` no épico.
4. **E12-T09** (llama.cpp in-process FFI) — L1 HTTP é o default (OA10).

Tudo o resto é incremental e **condicional ao gate de E05**.

---

## 7. Futuro (não entra nesta ordem)

| Épico | Documento | Quando |
|---|---|---|
| **E08** Adaptador MCP (2.ª implementação da porta `Memory`) | [`09`](plan/09-adaptador-knudge.md) | depois do MVK; só se a porta MCP for necessária |
| **E11** Plugin host e ABI (WASM) | [`12`](plan/12-plugins-e-abi.md) | só com produto estável + consumidor atual |
| **E17** Jail de SO real (bwrap/Landlock/seccomp) | [`18`](plan/18-jail-futuro.md) | pós-MVP, para converter a barreira **soft** em fronteira de kernel |

---

## 8. Mapa épico → ficheiro

| Épico | Documento | Onda |
|---|---|---|
| E01 Fundação | [`02-fundacao.md`](plan/02-fundacao.md) | 0 |
| E02 Política | [`03-contrato-de-politica.md`](plan/03-contrato-de-politica.md) | 1 |
| E03 Porta `Memory` | [`04-contrato-da-porta-memory.md`](plan/04-contrato-da-porta-memory.md) | 1 |
| E04 Kernel | [`05-kernel-loop.md`](plan/05-kernel-loop.md) | 2 |
| E05 MVK | [`06-mvk-enforcement-memoria.md`](plan/06-mvk-enforcement-memoria.md) | 3 (gate) |
| E06 Tools | [`07-tools-e-capacidades.md`](plan/07-tools-e-capacidades.md) | 4 |
| E07 Contenção soft | [`08-sandbox-fail-closed.md`](plan/08-sandbox-fail-closed.md) | 4 |
| E08 MCP *(futuro)* | [`09-adaptador-knudge.md`](plan/09-adaptador-knudge.md) | — |
| E09 Contexto | [`10-contexto-checkpoint-evidencia.md`](plan/10-contexto-checkpoint-evidencia.md) | 5 |
| E10 TUI | [`11-tui-e-ux.md`](plan/11-tui-e-ux.md) | 6 |
| E11 Plugins *(futuro)* | [`12-plugins-e-abi.md`](plan/12-plugins-e-abi.md) | — |
| E12 Providers | [`13-providers.md`](plan/13-providers.md) | 5 |
| E13 Testes | [`14-testes-e-qualidade.md`](plan/14-testes-e-qualidade.md) | 0,2,7 |
| E14 Governança | [`15-governanca-superficie.md`](plan/15-governanca-superficie.md) | 0,1,7 |
| E15 Performance | [`16-performance-benchmarks.md`](plan/16-performance-benchmarks.md) | 0,5,7 |
| E16 Gestão | [`17-roadmap-riscos.md`](plan/17-roadmap-riscos.md) | contínuo |
| E17 Jail *(futuro)* | [`18-jail-futuro.md`](plan/18-jail-futuro.md) | — |
| E18 Otimização profunda | [`19-otimizacao-profunda.md`](plan/19-otimizacao-profunda.md) | 0–7 |
