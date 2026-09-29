# `katu-tools`

**Épico:** E06/E07 · **Fase:** 3 · **Crate puro** (sem providers).

As **ferramentas** do core e a **contenção determinística soft**. Nenhuma operação sensível passa
sem veredicto; controlo em falta = recusa.

## Responsabilidade

- Superfície fechada de tools (§1.1): `read`/`write`/`edit`/`move`/`trash`/`bash`/`grep`/`find`/`ls`
  + `plan` — registada em `registry` (E06-T01).
- Forma AI-first (DF12): cada tool devolve um `ToolReport` (`katu_core::report`) renderizado em
  **TOON** ao modelo (JSON como alternativa).
- `read` (E06-T03): views `full`/`range`/`outline`/`summary`/`symbol`; estrutura por `outline`
  (heurística Rust-first); truncagem determinística; envelope com `id`/`hash`/`page`/`next`.
- `write_file` (E06-T03): só ficheiros **novos**; existentes via `edit`.
- `edit` (E06-T03/OA16): patch otimista com `write_atomic_if` (CAS), `dry-run`, `Stale` recuperável.
- `move_file` (E06-T11): renomeação atómica (`Fs::rename`) sob escopo; recusa destino existente.
- `search` (E06-T05): `grep`/`find`/`ls` determinísticos (`walk` com ignore + teto); hits
  clusterizados por símbolo; `ls` devolve o mapa semântico.
- `lang` (E06-T05): linguagem por extensão e conversões inteiras saturantes, partilhadas.
- `outline` (E06-T03): scanner heurístico de símbolos (Rust-first), sem tree-sitter.
- `write::WriteNoteTool` (E05-T01): commit de nota; só é invocado depois de a política permitir.
- Contenção **soft** (E07): caminhos canonicalizados, `argv` resolvido, autorização explícita.
- `trash` → `.katu/trash` (recuperável; esvaziar exige humano) — E06-T09.

## Fronteira

- Depende de `katu-policy`/`katu-core`; **não** depende de `katu-providers`/`katu-tui` nem de
  `knudge-core`.
- Honestidade: soft **não** é fronteira de segurança (a jail real é E17/futura).
