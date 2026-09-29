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
- Vocabulário **fechado e versionado** (`POLICY_VOCAB_VERSION`).

## Semântica de negação (OA15)

`DenyWrite`/`DenyRead`/`DenyDelete`/`DenyCommand` são **portas falha-fechado**: disparam salvo se o
contexto tiver a `Capability` correspondente (`WritePath`/`ReadPath`/`DeletePath`/`Command`).
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
| `audit` | auditoria de regras (E02-T04) |
| `error` | `PolicyError` |

## Artefactos versionados (na raiz `policy/`)

- `policy/memory.toml` — as 5 regras `Enforced` do protocolo de memória (E02-T07).
- `policy/containment.toml` — sensíveis `deny`-by-default + fora do workspace sob aprovação
  (E07-T05, vocabulário v2).
- `policy/coverage-ledger.json` — ledger de cobertura (`covered`/`not_applicable`/`deferred`),
  validado por `xtask ledger:validate` contra as regras `Enforced` de `policy/*.toml`.

## Fronteira

- **Não** depende de `katu-core`/`katu-tools`/`katu-providers`/`katu-tui` nem de `knudge-core`
  (verificado por `xtask check-layers`).
- Sem `unsafe`, sem `unwrap`/`expect`/`panic`, sem `HashMap` iterado.
- Testes: unitários por módulo + `tests/golden.rs` (matriz de veredictos) + `tests/memory_policy.rs`
  (gate do protocolo de memória) + `tests/containment.rs` (matriz real de `policy/containment.toml`).
