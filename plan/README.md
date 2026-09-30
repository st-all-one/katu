# Plano do katu — índice, convenções e gates

> Documento-mãe do `/plan`. Sequencia os passos **iniciais e futuros** do `katu` com critérios
> de validação **verificáveis**. Deriva de [`proposal/katu-brainstorm-decisoes.md`](../proposal/katu-brainstorm-decisoes.md)
> (§0–§67) e dos dossiês [`proposal/pi-rs/`](../proposal/pi-rs/) e [`proposal/goose-rs/`](../proposal/goose-rs/).

**Ordem de leitura:** [`00b-objetivos.md`](00b-objetivos.md) (o que o katu é) →
[`00-tese-e-escopo.md`](00-tese-e-escopo.md) (fronteira) →
[`01-decisoes-fundacionais.md`](01-decisoes-fundacionais.md) (contrato) → este README → épicos.

---

## 0. Ponto inflexível — Rust 1.97.0+

> **Todo o workspace do katu é baseado em Rust 1.97.0 ou superior. Isto não é preferência: é
> restrição do projeto.**

Forma de imposição (mecânica, não por prosa — ver §51.10 e §44 da brainstorm):

| Onde | O quê | Validação |
|---|---|---|
| `rust-toolchain.toml` | `channel = "1.97.0"` (raiz e `fuzz/`) | `rustc --version` no CI = `1.97.0` |
| `Cargo.toml` (workspace) | `edition = "2024"`, `rust-version = "1.97"` | `cargo msrv verify` / build em 1.97.0 puro |
| `clippy.toml` | `msrv = "1.97"`, `check-incompatible-msrv-in-tests = true` | `cargo clippy -D warnings` com o MSRV |
| CI | job `msrv` roda `cargo build --workspace` **em `1.97.0` exato** | job vermelho se qualquer crate exigir > 1.97 |
| Dependências | `default-features = false`; nenhuma dep com MSRV > 1.97 | `cargo update -Z minimal-versions` + build no job `msrv` |
| `rustfmt.toml` | `edition = "2024"` | `cargo fmt --check` |

**Regra:** um épico só fecha com o job `msrv` verde. Subir o piso acima de 1.97.0 exige uma
decisão registada (`DF` nova) e um commit próprio.

---

## 1. A tese, os objetivos e a definição de sucesso

**Objetivos e definição canónicos:** [`00b-objetivos.md`](00b-objetivos.md). Em uma frase:

> **O katu é um kernel agêntico mínimo, focado em código, com write/read/edit/trash/exec/search/
> planning como as únicas capacidades de tool e com memória/compactação/modelo como controlos do
> kernel; o knudge integrado in-process como memória, guardrails determinísticos, otimizado para
> tokens, com CLI e TUI — e só.**

Do §11 e §66.3 da brainstorm:

> **O katu é o loop possuído de um agente de código, com a política como kernel determinístico
> que avalia factos em vez de texto, capacidades em vez de denylists, e evidência tipada em vez
> de números afirmados — de modo que a única coisa que o agente não pode fazer é decidir por si
> próprio o que lhe é permitido.**

O diferencial defensável não é "mais um coding agent mínimo": é **um runtime onde o protocolo de
memória do knudge é invariante fiscalizada do loop** (§11). Sem esse recorte, o projeto não se
justifica (§53).

**Definição de sucesso do katu** (o que o produto prova ser):

1. Um agente **não consegue** gravar/editar conhecimento sem fazer a busca prévia — e o bloqueio
   é testado **pelo caminho real** (§51.9), não por extractores puros.
2. Toda negação carrega **evidência** (`rule_id`, facto, argumento) — a tríade
   `allowed`/`blocked`/`needs_human` (§29).
3. As regras são **dados versionados e revisíveis** (`policy/` em PR), não `regex` sobre strings
   nem prosa pedida com educação ao modelo (§51.4, §51.7).
4. O substrato (pi/goose) **não** impõe teto à política: o loop é nosso (§48).

---

## 2. Convenções

- **Épico:** `E<nn>` (ex.: `E05`). **Tarefa:** `E<nn>-T<rr>` (ex.: `E05-T03`).
- **Status:** ☐ pendente · ◐ em andamento · ☑ pronto. (Marcar no ficheiro do épico.)
- **Aceite:** critério **verificável** (teste, golden, propriedade, script, benchmark) — nunca prosa.
- **Rastreabilidade:** cada tarefa cita as **decisões fundacionais** que implementa (`DFxx`, ver
  [`01-decisoes-fundacionais.md`](01-decisoes-fundacionais.md)) e a secção da brainstorm (`§nn`).
- **Disciplina Rust 1.97.0+ (D92 do knudge):** ficheiros de produção ≤ 300 linhas; proibido
  `unwrap`/`expect`/`panic` em `src/`; `forbid(unsafe_code)` nas crates puras; `cargo fmt --check`
  + `clippy -D warnings` sempre verdes.
- **Um facto, um lar** (§44): o plano descreve *passos*; as decisões vivem em `01`; os contratos
  vivem em `03`/`04`. Não duplicar.

---

## 3. Fases e épicos

| Fase | Épico | Documento | Depende de | Gate |
|---|---|---|---|---|
| **0 — Fundação** | E01 Workspace, MSRV 1.97.0, ports, lints | [`02-fundacao.md`](02-fundacao.md) | — | `make check` verde em 1.97.0 |
| **1 — Contratos da tese** | E02 Motor de política (dados puros) | [`03-contrato-de-politica.md`](03-contrato-de-politica.md) | E01 | regras da memória expressáveis |
| | E03 Porta `Memory` + adaptador in-process (knudge-core) | [`04-contrato-da-porta-memory.md`](04-contrato-da-porta-memory.md) | E01 | contrato + knudge in-process |
| **2 — Núcleo vertical (MVK)** | E04 Kernel: loop possuído, estado, log | [`05-kernel-loop.md`](05-kernel-loop.md) | E02, E03 | replay determinístico |
| | E05 **MVK: enforcement da memória** | [`06-mvk-enforcement-memoria.md`](06-mvk-enforcement-memoria.md) | E04 | **gate de decisão** — ✅ passa ([ADR 0001](../docs/adr/0001-mvk-gate-aprovado.md)) |
| **3 — Capacidades** | E06 Tools, capacidades e `ToolOutcome` | [`07-tools-e-capacidades.md`](07-tools-e-capacidades.md) | E05 | negação pelo executor |
| | E07 Contenção determinística (soft) e fail-closed | [`08-sandbox-fail-closed.md`](08-sandbox-fail-closed.md) | E05 | controlo em falta = recusa; honestidade soft |
| **4 — Contexto e evidência** | E09 Contexto, checkpoint, evidência | [`10-contexto-checkpoint-evidencia.md`](10-contexto-checkpoint-evidencia.md) | E05 | artefactos validados |
| **6 — UX** | E10 TUI focada em codificação | [`11-tui-e-ux.md`](11-tui-e-ux.md) | E09 | panic-safe + evidência no ecrã |
| **8 — Amplitude** | E12 Camada de providers | [`13-providers.md`](13-providers.md) | E05 | firewall LLM-free |
| **Transversal** | E13 Testes e qualidade | [`14-testes-e-qualidade.md`](14-testes-e-qualidade.md) | todas | por regra, teste real |
| | E14 Governança e superfície | [`15-governanca-superficie.md`](15-governanca-superficie.md) | todas | teto de superfície |
| | E15 Performance e benchmarks | [`16-performance-benchmarks.md`](16-performance-benchmarks.md) | E05 | números com artefacto |
| | E18 Otimização profunda (matemática, info, estatística) | [`19-otimizacao-profunda.md`](19-otimizacao-profunda.md) | E05 | fórmula + artefacto + teste; adotar-ou-reverter |
| | E19 Instrumentação transversal (logs estruturados + métricas) | [`20-instrumentacao-transversal.md`](20-instrumentacao-transversal.md) | E01 | zero eventos desligada; `make instrument` |
| **Gestão** | E16 Roadmap, riscos e kill criteria | [`17-roadmap-riscos.md`](17-roadmap-riscos.md) | — | revisão por fase |

### Futuro (fora do escopo atual)

| Épico | Documento | Estado |
|---|---|---|
| E08 Adaptador MCP (`knudge-mcp`) — segunda implementação da porta `Memory` | [`09-adaptador-knudge.md`](09-adaptador-knudge.md) | **deferido**; a porta mantém a opção aberta (DF6, G4, G7) |
| E11 Plugin host e ABI (WASM); reimplementar providers; compressão inline no hot path | [`12`](12-plugins-e-abi.md), [`13`](13-providers.md), [`10`](10-contexto-checkpoint-evidencia.md) | **futuro/fora do plano principal**; o seam de regras é a política (E02) |
| E17 Jail de SO real (bwrap/Landlock/seccomp) | [`18-jail-futuro.md`](18-jail-futuro.md) | **futuro pós-MVP**; contenção **soft** agora (E07) |

**MVK (mínimo que prova a tese) = Fase 0 + 1 + 2.** Tudo o resto é incremental e **condicional ao
gate de decisão de E05**. O que não serve o core de [`00b`](00b-objetivos.md) §1.1 fica
`deferred` com razão registada.

> **E20 — Superfície v2** (reforma do CLI/TUI inspirada no `kd`: `prime`/`help`, verbos
> `[prime, upgrade, config, memo, run, tui]`, config global/local e TUI por `/`):
> [`../SURFACE_IMPLEMENTATION.md`](../SURFACE_IMPLEMENTATION.md).

---

## 4. Grafo de dependências

```
E01 ──┬── E02 ──┐
      └── E03 ──┴── E04 ── E05 ══ gate de decisão ══╗
                                  │                  ║
                                  ├── E06 ── E07      ║
                                  ├── E09 ── E10      ║
                                  └── E12             ║
                                                     ╠══ E13, E14, E15, E18, E19 (transversais)
                                                     ╚══ E16 (revisão por fase)

Futuro (fora do escopo atual): E08 adaptador MCP · E11 plugins WASM · múltiplos providers · jail de SO (E17)
```

---

## 5. Gate de decisão (o mais importante)

A resposta a "faz sentido implementar o katu?" **não** se decide por convicção; decide-se em E05.
O MVK existe para transformar a tese em **medida** (§62).

**Critérios de sucesso do MVK (todos obrigatórios):**

- [ ] O agente **falha** ao tentar gravar sem `pre_write` prévio, e o teste conduz o **loop real**.
- [ ] A negação traz `rule_id` + evidência estruturada e é devolvida como `Denied` ao modelo.
- [ ] Introduzir a regressão (remover o enforcement) deixa o teste **vermelho** (§51.9).
- [ ] As regras de memória do knudge (dedup ≥ 0.92, âncora obrigatória, `outcome` antes de fechar)
      são expressáveis **todas** como `DenyCommand`/`DenyWrite`/`RequireBefore`/`RequireAfter`.
- [ ] O atrito é medido e reportado com base de evidência tipada (§62), incluindo casos negativos.

**Se o MVK falhar** (regras não expressáveis, atrito inaceitável, ganho não sentido face a
`pi + knudge-mcp`), **para-se aqui**: não se escala para as fases 3–7 e não se constrói o `pi-rs`.
O investimento restante deve ir para **melhorar o knudge** e a sua integração com um agente
existente. Isto é o inverso do erro do `maxima` (§49): provar primeiro, escalar depois.

**Se o MVK passar**, as fases 3–7 são justificadas pela medida e cada uma mantém o seu próprio
gate na tabela do §3.

> **Decisão (2026-09-29): o MVK passou** ([ADR 0001](../docs/adr/0001-mvk-gate-aprovado.md)).
> As regras do protocolo são todas `Enforced`; as negações são acionáveis e provadas pelo caminho
> real com regressão vermelha; o atrito medido é aceitável. O loop possuído (DF1) vira compromisso.
> A comparação cross-tool com `pi + knudge-mcp` foi **dispensada pelo dono** e permanece `unpriced`
> (não inventada). As fases 3–7 arrancam; cada uma mantém o seu gate.

---

## 6. Definition of Done global

Um épico só fecha quando:

- [ ] `cargo fmt --check` e `clippy --workspace --all-targets -- -D warnings` passam.
- [ ] `cargo test --workspace` verde, incluindo as propriedades do épico.
- [ ] O job **`msrv` (Rust 1.97.0 exato)** está verde.
- [ ] Nenhum ficheiro de produção > 300 linhas; zero `unwrap`/`expect`/`panic` em `src/`.
- [ ] `unsafe` só onde autorizado, com comentário `// SAFETY:` e `#[allow(unsafe_code)]` local.
- [ ] As decisões citadas pelas tarefas têm teste/golden que as trava.
- [ ] `ARCHITECTURE.md` / `MODULE.md` e `CHANGELOG.md` atualizados.
- [ ] A **superfície** do épico foi medida e está dentro do teto (E14).
- [ ] Nenhum número publicado sem o artefacto que o produziu (E15, §62).

---

## 7. Rastreabilidade brainstorm → plano

| Brainstorm | Plano |
|---|---|
| §0–§9 (tese, goose, híbrido, GDK) | `00`, `00b`, `01`, `13` |
| §10–§13 (knudge, porta `Memory`, binário único) | `00b`, `01`, `04`, `09` |
| §20–§22 (lições do arags: firewall LLM-free, tiers, superfície) | `01`, `03`, `13`, `15` |
| §23–§25 (matemática pura, determinismo, derivar no read) | `03`, `10`, `14` |
| §28–§29 (security-audit: tríade, ledger, orçamento) | `03`, `05`, `14` |
| §31–§34 (workbench: superfícies, gate determinístico, autonomia) | `03`, `05`, `09`, `10` |
| §35–§40 (docling: camadas, `Partial`, slim) | `02`, `07`, `12`, `14` |
| §41–§46 (dsh: seams, invariantes, postmortems) | `01`, `04`, `12`, `15` |
| §47–§53 (maxima: rascunho fundacional, teto do substrato) | `00`, `01`, `05`, `06`, `17` |
| §54–§59 (zed: capacidades, ABI, governança) | `03`, `08`, `12`, `15` |
| §60–§65 (caveman: evidência tipada, recuo, honestidade) | `10`, `14`, `15`, `16` |
| §66–§67 (sete decisões, próximo artefacto) | `00`, `01`, `17` |
