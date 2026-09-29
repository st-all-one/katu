# `katu-tools`

**Épico:** E06/E07 · **Fase:** 3 · **Crate puro** (sem providers).

As **ferramentas** do core e a **contenção determinística soft**. Nenhuma operação sensível passa
sem veredicto; controlo em falta = recusa.

## Responsabilidade

- Superfície fechada de tools: `read`/`write`/`edit`/`trash`/`exec`/`search` + `plan`.
- `ToolOutcome`, capacidades e negação **pelo executor**.
- Contenção **soft** (E07): caminhos canonicalizados, `argv` resolvido, autorização explícita.
- `trash` → `.katu/trash` (recuperável; esvaziar exige humano).

## Fronteira

- Depende de `katu-policy`/`katu-core`; **não** depende de `katu-providers`/`katu-tui` nem de
  `knudge-core`.
- Honestidade: soft **não** é fronteira de segurança (a jail real é E17/futura).
