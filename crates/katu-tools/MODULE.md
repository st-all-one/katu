# `katu-tools`

**Épico:** E06/E07 · **Fase:** 3 · **Crate puro** (sem providers).

As **ferramentas** do core e a **contenção determinística soft**. Nenhuma operação sensível passa
sem veredicto; controlo em falta = recusa.

## Responsabilidade

- Superfície fechada de tools (§1.1): `read`/`write`/`edit`/`move`/`trash`/`bash`/`grep`/`find`/`ls`
  + `plan` + `memory` (grupo de **controlo**, E06-T10) — registada em `registry` (E06-T01).
- Forma AI-first (DF12): cada tool devolve um `ToolReport` (`katu_core::report`) renderizado em
  **TOON** ao modelo (JSON como alternativa).
- `read` (E06-T03): views `full`/`range`/`outline`/`summary`/`symbol`/`diff`; estrutura por
  `outline` (heurística Rust-first); truncagem determinística; envelope com `id`/`hash`/`page`/`next`.
  A view `diff` compara com a versão anterior (`base`) via `diff::unified`.
- `diff` (E06-T03): diff unificado determinístico (prefixo/sufixo comum, sem LCS O(n·m)).
- `write_file` (E06-T03): só ficheiros **novos**; existentes via `edit`.
- `edit` (E06-T03/OA16, Q-07): patch otimista com `write_atomic_if` (CAS), `dry-run`, `Stale`
  recuperável. Uma chamada leva **várias** substituições (`Replacement`), aplicadas **por ordem** e
  **atomicamente** (`apply`): uma que não case exatamente uma vez ⇒ **nada** é gravado e o relatório
  `edit.rejected` diz qual falhou, porquê e as **âncoras únicas mais próximas** (`nearest_anchors`,
  Q-08) — o modelo recebe o que existe perto em vez de adivinhar. Medido em
  [`bench/e18/edit`](../../bench/e18/edit/PROTOCOL.md): 5 chamadas → 1 (**−80 %**), payload do modelo
  −19,9 % e, com a 3.ª substituição a falhar, a forma antiga deixava o ficheiro a meio (375 B) e a
  atómica não escreve nada (383 B = original).
- `exec` (E06-T04/T07): `ExecTool` com `argv`/`cwd` resolvidos, ambiente filtrado de segredos e
  timeout; devolve um `CommandRecord` (redigido + truncado pela cauda, `duration_ms`); outcomes
  ortogonais (`exit`/`signal`/`timed_out`); porta `Process` (fake `MemProcess`).
- `move_file` (E06-T11): renomeação atómica (`Fs::rename`) sob escopo; recusa destino existente.
- `trash` (E06-T09): move para `<root>/.katu/trash` (preserva o relativo) com índice append-only;
  `list` (vista do utilizador, filtra removidos), `restore` (sempre permitido) e `empty`
  (destrutivo, exige challenge na UI; E10-T07). Nada é apagado automaticamente.
- `search` (E06-T05): `grep`/`find`/`ls` determinísticos (`walk` com ignore + teto); hits
  clusterizados por símbolo; `ls` devolve o mapa semântico. `search_use` traz a **raiz resolvida**
  em `resolved_paths` (E07-T05), para a política avaliar leitura fora do workspace/sensíveis.
- `lang` (E06-T05): linguagem por extensão e conversões inteiras saturantes, partilhadas.
- `outline` (E06-T03): scanner heurístico de símbolos (Rust-first), sem tree-sitter.
- `plan` (E06-T06): `PlanTool` valida o plano tipado (`katu_core::plan`) e devolve `plan.validate`
  ou `Unavailable{control}` acionável.
- `write::WriteNoteTool` (E05-T01/E06-T10): commit de nota; só é invocado depois de a política
  permitir; devolve o envelope `memory.record`. A tool `memory` só **pede** — o gate
  (`pre_write`/dedup/âncora) vive no kernel e não é contornável.
- `recall::RecallTool` (E06-T10): consulta a porta `Memory` (`search`); devolve o envelope
  `memory.recall` e marca `memory_recall` no log (pré-condição de `memory_write`).
- Contenção **soft** (E07): caminhos canonicalizados, `argv` resolvido, autorização explícita.
- `trash` → `.katu/trash` (recuperável; esvaziar exige challenge humano) — E06-T09/E10-T07.

## Fronteira

- Depende de `katu-policy`/`katu-core`; **não** depende de `katu-providers`/`katu-tui` nem de
  `knudge-core`.
- Honestidade: soft **não** é fronteira de segurança (a jail real é E17/futura).
- Testes de imposição: `tests/enforcement.rs` (E06-T08: cada tool negada por `dispatch` não produz
  efeito) e `tests/containment_gate.rs` (E07-T03: controlo em falta = recusa; soft não confina).
