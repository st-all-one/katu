# E16 — Roadmap, riscos e kill criteria

> **Gestão.** A ordem de execução, os gates por fase, os riscos e — o mais importante — **quando
> parar**. Revisão obrigatória no fim de cada fase.
>
> **Decisões:** todas. **Depende de:** —.
> **Gate:** cada fase só começa com o gate da anterior **verde e assinado**.

---

## 1. Ordem de execução

```
Fase 0  E01  Fundação (Rust 1.97.0+, ports, camadas)
   ▼
Fase 1  E02  Contrato de política   ─┐
        E03  Porta Memory + knudge  ─┴─► GATE: regras de memória expressáveis + porta substituível
   ▼
Fase 2  E04  Kernel (loop possuído)  ─┐
        E05  MVK enforcement          ─┴─► ═══ GATE DE DECISÃO (passa/para) ═══
   ▼
   (só se E05 passar)
Fase 3  E06  Tools/capacidades (write/read/exec/search + planning)
        E07  Sandbox fail-closed
Fase 4  E09  Contexto/checkpoint/evidência (token-optimized)
Fase 5  E10  TUI
Fase 6  E11  Plugins/ABI
Fase 7  E12  Providers
Transversal  E13 testes · E14 governança · E15 performance
Futuro (fora do escopo): E08 adaptador MCP
```

**Por que esta ordem:** ao contrário do §67 da brainstorm (que propunha as 7 ADRs primeiro), este
plano põe **a medida antes da escala**. O ADR do loop possuído (DF1) só vira compromisso depois de
E05 provar que a fiscalização da memória funciona e vale o esforço.

---

## 2. Gates por fase (assinatura obrigatória)

| Fase | Gate | Evidência mínima |
|---|---|---|
| 0 | `make check` verde em 1.97.0 | job `msrv` verde |
| 1 | regras de memória **todas** `Enforced`; porta substituível | `policy/memory.toml` + E02-T07 + E03-T06 |
| 2 | **MVK passa** (ou para) | E05-T07 checklist completa + relatório de atrito |
| 3 | negação pelo executor; sem passthrough silencioso | E06-T07 + E07-T03 |
| 4 | nenhum número sem base; artefactos validados; token medido | E09-T05 + E15-T02 |
| 5 | terminal panic-safe; evidência no ecrã | E10-T01/T04 |
| 6 | capacidades declaradas ∧ concedidas | E11-T02/T06 |
| 7 | firewall LLM-free intacta | `xtask check-layers` |

---

## 3. Riscos

| # | Risco | Prob. | Impacto | Mitigação | Fonte |
|---|---|---|---|---|---|
| R1 | GDK alpha quebra API | alta | médio | pinar versão; vendorizar peças pequenas; isolar atrás de trait próprio | §8 |
| R2 | Escopo "controle" vira produto inteiro | alta | **alto** | teto de superfície (E14-T05); kill criteria; MVK antes da escala | §8, §51.13 |
| R3 | Sandbox cross-platform é caro | média | médio | Linux primeiro; fallback degradado **explícito** | §8, §51.13 |
| R4 | Performance regride por persistência | média | médio | benchmarks desde a Fase 1; gate de CI | §8, E15 |
| R5 | Reimplementar commodity sem querer | média | alto | regra "commodity = dependência"; revisão por fase | §2 |
| R6 | Segurança de plugin (código não confiável) | média | alto | capacidades + isolamento; nunca shell irrestrito | §8, E11 |
| R7 | As regras de memória não caberem no modelo | média | **alto** | é precisamente o gate E02-T07/E05 | §5 do README |
| R8 | Acoplamento ao knudge (tipo vaza) | média | alto | porta com tipos do katu; teste de sanidade | §12–§13, E03-T06 |
| R9 | Regressão silenciosa no enforcement | média | alto | teste invertido por regra (E13-T03) | §51.9 |
| R10 | Métrica inventada funda decisão | média | alto | bases tipadas + artefacto (DF5, E15) | §49, §62 |
| R11 | MSRV 1.97.0 violado silenciosamente | baixa | médio | job `msrv` dedicado; `clippy.toml msrv` | ponto inflexível |
| R12 | A superfície de docs cresce (2.436 notas do dsh) | alta | médio | um facto um lar + orçamentos (E14-T03) | §44 |
| R13 | Contexto/tokens crescem sem medição | média | alto | só o delta; orçamento; custo por turno com base tipada (G6) | §18, E09, E15 |

---

## 4. Kill / pivot criteria (o mais importante)

O projeto **para ou pivota** se qualquer destes ocorrer:

1. **K1 — Regras de memória não expressáveis.** Se E02-T07 ou E05-T07 falharem (alguma regra do
   protocolo do knudge não couber em `DenyWrite`/`RequireBefore`/`RequireAfter`/pré-condição),
   então o substrato ainda decide. **Pivot:** investir no knudge (hooks `pre-record`, mensagens que
   ensinam, gatilhos mais fortes) e na integração com o agente existente. Registar postmortem.
2. **K2 — Atrito maior que o benefício.** Se o relatório honesto de E05-T06 mostrar atrito
   inaceitável face a `pi + knudge-mcp` — inclusive nos casos em que o katu **não** ganha —
   **parar**. Não há vergonha em dizer que a tese não se paga (a honestidade do caveman §60).
3. **K3 — Superfície incontível.** Se `xtask check-surface` exceder o teto em duas fases
   consecutivas sem que o valor cresça, **podar antes de continuar** (a lição do arags §20).
4. **K4 — Firewall violada.** Se `katu-core`/`policy`/`tools` passarem a depender de um provider
   para funcionar, a tese do plano de dados ruiu. Reverter ou repensar (DF, §21).
5. **K5 — MSRV não sustentável.** Se manter o piso 1.97.0 exigir vendorizar dependências demais,
   decisão registada (`DF` nova) ou reduzir escopo — nunca "deixar passar".

---

## 5. Relação com `pi` e `goose` (a pergunta original)

- **goose:** commodity (via GDK) e referência de arquitetura. Não é cérebro, não se forka (§1–§9).
- **pi:** substrato de onde se aprende (o `pi-rs` é um port do commodity — **não** é o caminho do
  katu, §2). O pi pode continuar a existir como *outro* consumidor do knudge.
- **katu:** existe para **inverter a proporção** de regras entre os ~70% (prosa) e os 100%
  (runtime) — e só se E05 provar que essa inversão é real e sentida (§48, §52).

---

## 6. Próximo passo imediato (a partir de agora)

1. Criar o repositório de código **fora** deste repo de proposta (workspace `katu`), com
   `rust-toolchain.toml` pinado a `1.97.0`.
2. Executar **E01-T01…T03** (workspace, ports, gate de qualidade + job `msrv`).
3. Executar **E02** (contrato de política) e **E03** (porta Memory) até ao gate de Fase 1.
4. Só então **E04/E05** — e deixar o gate decidir o resto.

**Regra de ouro do plano:** *medir primeiro, escalar depois.*
