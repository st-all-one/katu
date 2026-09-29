# `katu-policy`

**Épico:** E02 · **Fase:** 1 · **Crate puro** (sem I/O, sem providers).

O **motor de política** do katu: avalia factos tipados e devolve um veredicto determinístico
(DF2). É a **tese** do projeto — a superfície que impõe a política ao agente.

## Responsabilidade

- Tipos de facto: `ToolUse`, `ResolvedPath`, `ResolvedArgv`, `Capability`, `Phase`, `Facts`.
- Tipos de regra: `Rule`, `RuleScope`, `Enforcement`, `RuleCategory`, `Decision`, `Evidence`.
- `evaluate(facts, rules) -> Decision` — **puro**, sem relógio, sem FS, sem regex sobre texto.
- `audit(rules, now) -> AuditReport` — categorias (`Enforced`/`Advisory`), exemplos negativos,
  duplicados e enunciados vazios (E02-T04).
- Vocabulário **fechado e versionado** (`POLICY_VOCAB_VERSION`).

## Semântica de negação (OA15)

`DenyWrite`/`DenyDelete`/`DenyCommand` são **portas falha-fechado**: disparam salvo se o contexto
tiver a `Capability` correspondente (`WritePath`/`DeletePath`/`Command`). O motor não lê prosa nem
calcula similaridade; as pré-condições semânticas (dedup ≥ 0.92, âncora, uma afirmação) chegam como
**capacidade** concedida pelo adaptador de memória (E03). O motor mantém-se determinístico.

## Mapa de módulos

| Módulo | Conteúdo |
|---|---|
| `paths` | `ResolvedPath`/`ResolvedArgv` + normalização lexical (construtor privado) |
| `facts` | `Phase`, `ToolName` (inclui operações de memória), `ToolArgs`, `ToolUse`, `Capability`, `BudgetState`, `Facts` |
| `rule` | `Rule`, `RuleScope`, `Enforcement`, `Severity`, `RuleCategory`, `Waiver`, `RuleExamples`, `RuleSet` (TOML, fail-closed) |
| `decision` | `Decision`, `Evidence`, `Reason`, `ApprovalRequest`, `ControlId` |
| `engine` | `impl Rule` (âmbito/casamento/veredicto) + portas de capacidade |
| `evaluate` | `evaluate` puro + seleção do veredicto de maior `rank` |
| `audit` | auditoria de regras (E02-T04) |
| `error` | `PolicyError` |

## Artefactos versionados (na raiz `policy/`)

- `policy/memory.toml` — as 5 regras `Enforced` do protocolo de memória (E02-T07).
- `policy/coverage-ledger.json` — ledger de cobertura (`covered`/`not_applicable`/`deferred`),
  validado por `xtask ledger:validate` contra as regras `Enforced` de `policy/*.toml`.

## Fronteira

- **Não** depende de `katu-core`/`katu-tools`/`katu-providers`/`katu-tui` nem de `knudge-core`
  (verificado por `xtask check-layers`).
- Sem `unsafe`, sem `unwrap`/`expect`/`panic`, sem `HashMap` iterado.
- Testes: unitários por módulo + `tests/golden.rs` (matriz de veredictos) + `tests/memory_policy.rs`
  (gate do protocolo de memória).
