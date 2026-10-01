# `katu-policy`

**Épico:** E02 · **Fase:** 1 · **Crate puro** (sem I/O, sem providers).

O **motor de política** do katu: avalia factos tipados e devolve um veredicto determinístico
(DF2). É a **tese** do projeto — a superfície que impõe a política ao agente.

## Responsabilidade

- Tipos de facto: `ToolUse`, `ResolvedPath`, `ResolvedArgv`, `Capability`, `Phase`, `Facts`.
- Tipos de regra: `Rule`, `RuleScope`, `Enforcement`, `RuleCategory`, `Decision`, `Evidence`.
- `evaluate(facts, rules) -> Decision` — **puro**, sem relógio, sem FS, sem regex sobre texto.
- `inspect(argv) -> ArgvInspection` — inspetor de `argv` determinístico (E07-T02): interpretadores,
  código inline, flags destrutivas/aninhadas, programas de **rede** (`curl`/`ssh`/…) e o host
  extraído; sem regex.
- `audit(rules, now) -> AuditReport` — categorias (`Enforced`/`Advisory`), exemplos negativos,
  duplicados e enunciados vazios (E02-T04).
- `verdict(id, Trials, Threshold) -> Verdict` (Q-11/F6) — confiança **medida**: posterior
  Beta–Bernoulli (`Trials`) + limite inferior de Wilson; `Enforced` só com `LB ≥ θ` **e**
  `n ≥ n_min`, senão demove a `Advisory` **com a evidência** no motivo. O limiar é dado
  (`Threshold::DEFAULT`), nunca derivado dos dados (DF8). Com registo perfeito o limiar sozinho
  prova a regra a **n = 25** (LB `902 ≥ 900`); com o controlo de família (abaixo) a **n = 29**. Uma
  violação em 20 derruba o LB para `804` e marca `contradiction` (artefacto em
  [`bench/e18/confidence`](../../bench/e18/confidence/PROTOCOL.md)).
- `benjamini_hochberg(&[u32], q) -> MultipleTests` + `control_fdr(&mut [Verdict], &Threshold)`
  (C5) — **controlo de múltiplas comparações** sobre a família de regras `Enforced`: p-value exacto
  unilateral (`H0: p ≥ θ`, cauda binomial superior `P[X ≥ s | n, θ]`, em micro com piso 1 µ) por
  regra, BH a `q = 5 %` (`Threshold.q_milli`). `control_fdr` só pode **demover** uma promoção
  (fail-closed) e deixa o motivo com a evidência. Medido: 40 regras com 25 honras cada → o limiar
  sozinho promove 40, o BH **nenhuma** (`p = 0,0718 > 0,05`).
- `calibrate(&[Verdict]) -> Calibration` (C3/W8-2) — **calibração** do LB face à frequência
  empírica do log (base `inferred`): ECE, Brier e diagrama de fiabilidade (10 baldes), determinístico.
  No registo perfeito o ECE desce de `730‰` (n = 1) a `83‰` (n = 30); o `policy:confidence` publica
  ECE/Brier. Mede o **conservadorismo** do limite (in-sample), não o acerto do modelo.
- Vocabulário **fechado e versionado** (`POLICY_VOCAB_VERSION`, v3 desde o ADR 0022).

## Semântica de negação (OA15)

`DenyWrite`/`DenyRead`/`DenyDelete`/`DenyCommand` são **portas falha-fechado**: disparam salvo se o
contexto tiver a `Capability` correspondente (`WritePath`/`ReadPath`/`DeletePath`/`Command`).
`DenyWriteOutside { root }` (E20-T11) é um **muro duro**: nega escrita **fora** de `root` mesmo com
capacidade — é o que faz o modo `/plan` escrever só sob `.katu/` (ADR 0022).
`Capability::Workspace { root }` é o grant **implícito** da raiz (E07-T05): destranca o normal
dentro do workspace, mas **nunca** `DenySensitiveRead` — esse só cede a `ReadPath` explícito
(aprovação humana). `Capability::Exec { program }` é mais fino: só destranca `DenyCommand { Exec }`
para um `argv` **verificável** (não opaco/destrutivo) cujo programa casa exatamente — `bash -c`,
`find -delete`, `find -exec` e `r''m` continuam negados. Programas de **rede** não são destrancados
por `Exec`: só `Capability::Net { host }` (`*` = qualquer) os concede, casando o host do URL/`user@host`
(E07-T05). A leitura inclui a **busca** (`read` + `grep`/`find`/`ls`): o `ToolUse` traz a raiz
resolvida em `resolved_paths`. O motor não lê prosa nem calcula
similaridade; as pré-condições semânticas (dedup ≥ 0.92, âncora, uma afirmação) chegam como
**capacidade** concedida pelo adaptador de memória (E03). O motor mantém-se determinístico.

## Mapa de módulos

| Módulo | Conteúdo |
|---|---|
| `paths` | `ResolvedPath`/`ResolvedArgv` + normalização lexical (construtor privado) |
| `argv` | `inspect`/`ArgvInspection`/`ProgramKind` — inspetor determinístico de `argv` (E07-T02) |
| `glob` | `matches_glob` — glob único do projeto (`*`/`?`), reexportado por `katu-core::plan` (E07-T05) |
| `facts` | `Phase`, `ToolName` (inclui operações de memória), `ToolArgs`, `ToolUse`, `Capability`, `BudgetState`, `Facts` |
| `rule` | `Rule`, `RuleScope`, `Enforcement`, `Severity`, `RuleCategory`, `Waiver`, `RuleExamples`, `RuleSet` (TOML, fail-closed) |
| `decision` | `Decision`, `Evidence`, `Reason`, `ApprovalRequest`, `ControlId` |
| `engine` | `impl Rule` (âmbito/casamento/veredicto) + portas de capacidade |
| `evaluate` | `evaluate` puro + seleção do veredicto de maior `rank` |
| `approval` | `capability_for`/`capability_for_request` — capacidade **mínima** que satisfaz um `RequireApproval` (E07-T05, §33) |
| `audit` | auditoria de regras (E02-T04) |
| `confidence` | `Trials`/`Threshold`/`Confidence`/`Verdict`/`verdict`/`benjamini_hochberg`/`control_fdr` — confiança medida (Q-11/F6) e FDR (C5) |
| `error` | `PolicyError` |

## Artefactos versionados (na raiz `policy/`)

- `policy/memory.toml` — as 5 regras `Enforced` do protocolo de memória (E02-T07).
- `policy/containment.toml` — sensíveis `deny`-by-default + fora do workspace sob aprovação
  (E07-T05, vocabulário v3).
- `policy/coverage-ledger.json` — ledger de cobertura (`covered`/`not_applicable`/`deferred`),
  validado por `xtask ledger:validate` contra as regras `Enforced` de `policy/*.toml`.

## Fronteira

- **Não** depende de `katu-core`/`katu-tools`/`katu-providers`/`katu-tui` nem de `knudge-core`
  (verificado por `xtask check-layers`).
- Sem `unsafe`, sem `unwrap`/`expect`/`panic`, sem `HashMap` iterado.
- Testes: unitários por módulo + `tests/golden.rs` (matriz de veredictos) + `tests/memory_policy.rs`
  (gate do protocolo de memória) + `tests/containment.rs` (matriz real de `policy/containment.toml`).
