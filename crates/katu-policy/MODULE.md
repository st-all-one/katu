# `katu-policy`

**Épico:** E02 · **Fase:** 1 · **Crate puro** (sem I/O, sem providers).

O **motor de política** do katu: avalia factos tipados e devolve um veredicto determinístico
(DF2). É a **tese** do projeto — a superfície que impõe a política ao agente.

## Responsabilidade

- Tipos de facto: `ToolUse`, `ResolvedPath`, `ResolvedArgv`, `Capability`, `Phase`.
- Tipos de regra: `Rule`, `RuleScope`, `Enforcement`, `RuleCategory`, `Decision`, `Evidence`.
- `evaluate(facts, rules) -> Decision` — **puro**, sem relógio, sem FS, sem regex sobre texto.
- Vocabulário **fechado e versionado** (`POLICY_VOCAB_VERSION`).

## Fronteira

- **Não** depende de `katu-core`/`katu-tools`/`katu-providers`/`katu-tui` nem de `knudge-core`
  (verificado por `xtask check-layers`).
- Sem `unsafe`, sem `unwrap`/`expect`/`panic`, sem `HashMap` iterado.
