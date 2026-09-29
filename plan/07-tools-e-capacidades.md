# E06 — Tools, capacidades e `ToolOutcome`

> **Fase 3.** As ferramentas de codificação e o contrato de resultado. Só depois de E05 passar.
> A superfície é **fechada**: exatamente as cinco famílias de tools do **core** de
> [`00b`](00b-objetivos.md) §1.1 (G3).
>
> **Decisões:** DF2, DF3, DF4, G3. **Depende de:** E05.
> **Gate do épico:** a decisão é imposta **pela operação que a toma**, testada pelo executor;
> `Partial` é cidadão de primeira classe.

---

## Conjunto mínimo (G3) — a superfície inteira

| Família | Ferramentas | Nota |
|---|---|---|
| **Escrita** | `write`, `edit`, `move`, `trash` | altera ficheiros sob escopo, sempre sujeita à política; `move` é atómico e invalida índice/cache; `trash` move para `.katu/trash` (recuperável) |
| **Leitura** | `read` | fonte de verdade para a fase `KnowledgeConsulted` |
| **Execução** | `bash` | `argv` resolvido + `Capability::Exec`; corre **como o utilizador que evocou o processo** (nunca eleva); a única superfície de execução |
| **Pesquisa de ficheiros** | `grep`, `find`, `ls` | otimizada: respeita ignore, varredura em streaming, só o delta; output canónico e truncado |
| **Planejamento** | `plan` (artefacto) | mantém `scope_contract` / `feature_list` e as transições de fase |

**Nada além disso entra agora.** `git`, LSP, browser, imagem, etc. são **`deferred`** com razão
registada e auditável por `xtask check-surface` (filtro de [`00b`](00b-objetivos.md) §4).

> **Forma AI-first (DF12).** Cada tool devolve um **envelope tipado** (`ToolReport`:
> `kind`/`id`/`hash`/`data`/`page`/`next`/`cost`), renderizado em **TOON** ao modelo (JSON como
> alternativa de máquina); `read` expõe **views** (`outline`/`summary`/`symbol`/`diff`/`full`) e
> `grep`/`find` devolvem **hits semânticos**. O core (índice/cache/syscalls) otimiza-se **por
> medição** (adoptar-ou-reverter, E18); começa em `std::fs` + cache L1 por `path+fingerprint`.

> **Controlos do kernel.** Compactar a conversa (E09-T07) e alterar modelo/grau de pensamento
> (E12-T10) são capacidades do [core](00b-objetivos.md) §1.1 #10/#11 — **não** acrescentam tools.
> Registrar memória (#7) é **primariamente** pelos hooks/fases, com a tool `memory` (E06-T10) como
> **pedido explícito policy-gated** no grupo de controlo — nunca uma família de codificação nova.

> **Planejar = backlog do knudge, não um segundo motor** (skill
> [`knudge/07`](../.agents/skill/knudge/07-tarefas-e-planejamento.md) e
> [`06_tarefas_e_handoff`](../knudge/wiki/integration/06_tarefas_e_handoff.md)). A hierarquia
> `epic ⊃ {issue ⊃ task | task}` (pai único, profundidade ≤ 4), a máquina de `status`, o
> `outcome`/evidência do fecho, o WBS (`task graph`), o fluxo/caminho crítico (`task flow`) e o
> `task plan --template feature|bug|refactor` são **views derivadas da memória**, nunca um banco
> paralelo. O `scope_contract`/`feature_list` de E06-T06 é outra coisa: o **artefacto de fase**
> que a política usa para `Task → Planned`; não substitui nem duplica o backlog.

---

## Contrato de saída (do docling, §38; open-mtr, §16)

```rust
pub enum ToolOutcome {
    Ok,
    Partial { errors: Vec<OutcomeError> },   // "correu 8 de 10"; NÃO é Ok nem Err
    Denied  { rule_id: RuleId, evidence: Evidence },
    Timeout { after: Duration },
    Unavailable { control: ControlId },
}
pub struct OutcomeError { pub component: Component, pub category: FailureCategory,
                          pub scope: Scope, pub message: String }
```

**Invariante travada por teste:** nunca reportar `Ok` com `errors` pendurados; o gate de
verificação recusa `SUCCESS` com erros (§38).

---

## Tarefas

### E06-T01 ☑ Registry de tools fechado
- **Entregáveis:** registo tipado, namespace estável e ordem canônica; lista exata das famílias de
  §1.1 (Escrita `write`/`edit`/`move`/`trash`; Leitura `read`; Execução `bash`; Pesquisa
  `grep`/`find`/`ls`; Planeamento `plan`).
- **Estado:** `katu_tools::registry` (`Family`/`ToolId`/`ToolSpec`/`TOOLS`/`is_registered`), com
  `ToolName::Move` adicionado ao vocabulário de política. Testes: 10 tools, ids únicos, famílias
  4/1/1/3/1, `is_registered` para tools estranhas. `xtask check-surface` chega em E14-T05.
- **Aceite:** qualquer tool fora do conjunto mínimo exige uma decisão registada (filtro `00b` §4);
  `xtask check-surface` falha ao exceder o teto; nenhum registo sem teste de teardown (§44).

### E06-T02 ☐ Tool-schema linter
- **Entregáveis:** linter que impõe `snake_case`, ordem verbo-substantivo, descrição "Use when X.
  Do not use for Y." (< 1024 chars), enums para conjuntos fechados, IDs tipados com `pattern`,
  e **erros que ensinam** (`Invalid input: 'city' is required. Example: {...}`); anti-poisoning
  (rejeitar `<SYSTEM>`, "ignore previous", markdown oculto).
- **Erros (OA19):** a validação devolve `Issue { path, message }` **agregado** (nunca `String`
  solta), reusando o mesmo tipo de erro do validador de checkpoint (E09-T02).
- **Aceite:** o CI falha se um schema violar as regras; o erro de validação de exemplo é testado.

### E06-T03 ◐ Escrita e leitura (`write`, `read`, `edit`)
- **Entregáveis:** argumentos tipados, paths resolvidos, output determinístico; truncagem
  **determinística**; deltas (só o que mudou) como regra de contexto (§18).
- **Estado:** `read` com **views** `full`/`range`/`outline`/`summary`/`symbol` (heurística Rust-first
  em `katu_tools::outline`; `diff` ainda indisponível) devolve `ToolReport` com `id`/`hash`/`loc`/
  `page`/`next`; `write` (`write_file`) só para ficheiros **novos** (existentes → `Unavailable`);
  `edit` otimista com `write_atomic_if` (CAS), `dry-run` e `Unavailable{stale}`/`ambiguous`.
  Truncagem determinística (linhas/bytes) testada. **Falta:** view `diff`, tree-sitter (gated) e
  a matriz multibyte exata de §45.22.
- **Views e envelope (DF12):** `read` aceita `view=outline|summary|symbol|diff|full` (default
  `summary`), devolvendo o **envelope** com `id`/`hash`/`loc`/`truncated`/`next`; `symbol` devolve
  só o range. A estrutura sai de uma **heurística leve** (Rust-first); tree-sitter fica gated por
  medição. `write` é para ficheiros **novos**; existentes passam por `edit`.
- **`edit` otimista (OA16):** `read` → aplicar patch → `Fs::write_atomic_if(path, novo, lido)`;
  `FsError::Stale` (o ficheiro mudou desde a leitura) mapeia para `ToolOutcome` **recuperável**
  ("relê e reaplica"), **nunca** sobrescreve edição concorrente; `dry-run` mostra o patch antes de
  aplicar.
- **Aceite:** propriedade: output canónico (ordenação estável, sem `HashMap`); teste de truncagem
  em limites minúsculos, exatos, chunks únicos enormes e multibyte (§45.22); teste de `Stale`
  (edição externa entre a leitura e a escrita não é perdida); round-trip `view=full` = bytes lidos.
- **Estado (aceite):** round-trip `full` = bytes lidos testado; truncagem determinística testada;
  `Stale`/ambíguo/`dry-run` testados; falta a matriz multibyte de §45.22.

### E06-T04 ☐ Execução (`bash`) com argv resolvido e capacidades
- **Entregáveis:** execução que resolve `argv` e `cwd` **antes** da política; `Capability::Exec`;
  nenhuma decisão por regex sobre a string; o processo corre com o **uid/gid do utilizador que
  evocou o katu** (sem `sudo`/setuid; o sandbox só restringe, nunca amplia).
- **Aceite:** `cd x && rm`, `bash -c`, `find -delete`, `r''m` são avaliados sobre factos; o golden
  de E02-T05 cobre estes casos; `Deny` = efeito não ocorre; o filho herda o utilizador e nenhum
  caminho eleva privilégio.

### E06-T05 ☑ Pesquisa de ficheiros (`grep`, `find`, `ls`)
- **Entregáveis:** busca canónica com filtro de escopo e truncagem; **otimizada**: respeita
  `.gitignore`/ignore configurável, varredura em streaming (sem carregar o ficheiro inteiro) e
  paralela quando possível; resultado como **ponteiro** quando grande (só o delta chega ao modelo,
  §18).
- **Saída AI-first (DF12):** motor `grep` (crate do ripgrep); hits **clusterizados por símbolo**
  (`s_*`) com tipo de linha (código/comentário/string/import/teste), `path:line:preview`,
  **informação negativa** ("0 outros callers"), relevância e `next`. `find` ranqueia por relevância
  à tarefa; `ls` devolve **mapa semântico** (linguagem, loc, exports, testes) — nunca `ls -la`.
- **Estado:** `SearchTool` (`tool.search`) com `SearchMode::{Grep,Find,Ls}`; varredura recursiva
  determinística (`walk`, ordem canónica, profundidade 16, teto de 4096 ficheiros, dotfiles e
  `.gitignore` simples ignorados); `grep` classifica a linha (code/comment/import/string/test),
  clusteriza pelo símbolo mais interior (`outline`) e devolve informação negativa; `find` ranqueia
  por nome (3 exato / 2 contém / 1 no caminho) e `ls` dá o mapa semântico (lang/loc/symbols/
  exports/tests). Varrredura **ficheiro-a-ficheiro** com teto (não carrega o repositório inteiro);
  motor ripgrep, streaming intra-ficheiro e paralelismo ficam **gated por medição** (E15/E18).
- **Aceite:** output ordenado estavelmente; limite de resultados aplicado; teste com path fora do
  escopo é negado; um repositório grande é pesquisado sem pico de memória; dois `grep` iguais dão
  bytes iguais (determinismo).

### E06-T06 ☐ Planejamento (`plan`) como capacidade de primeira classe
- **Entregáveis:** artefacto de plano tipado (`scope_contract` + `feature_list`), com
  `allowed_files`/`forbidden_files` (globs), `acceptance_criteria`, `rollback_plan`; invariante
  "≤ 1 `in_progress`" verificada no startup; o agente transita `Task → Planned` ao criar/atualizar.
- **Aceite:** plano sem `forbidden_files` ou sem rollback **não** é aceite; plano é validado por
  schema; `Task → Planned` sem plano é `Refusal` (E04).

### E06-T07 ☐ Feedback runner e registo de comando
- **Entregáveis:** cada comando captura `stdout_tail`/`stderr_tail`/`exit_code`/`duration_ms`/
  `parent_command_id`; truncagem determinística; redação no write; rotação; `exit_code: null` ⇒
  **recusa avançar** (§31).
- **Aceite:** `exit_code: null` bloqueia a transição de fase; redação tem teste.

### E06-T08 ☐ **Gate do épico:** imposição na operação, não em wrapper
- **Objetivo:** a regra "impor a decisão na operação que a toma" (§45.20).
- **Entregáveis:** para cada tool, um teste que tenta contornar a política por um chamador direto
  ou caminho alternativo e **falha**.
- **Aceite (gate):** nenhuma regra depende de ordem de listeners ou de filtro de prompt para ser
  imposta; a negação é observada no executor.

### E06-T09 ☐ Lixeira do projeto (`trash` → `.katu/trash`)
- **Entregáveis:** `trash` **move** (não copia+apaga) para `.katu/trash`, preservando o caminho
  relativo e um índice com o original + timestamp do log; `restore` é **sempre** permitido; a
  lixeira é **por projeto**, fora do escopo de escrita normal; `Capability::Delete` cobre a
  operação; **nada** em `.katu/trash` é apagado automaticamente (sem TTL/auto-purge) e esvaziar a
  lixeira é `RequireApproval`/`NeedsHuman`.
- **Aceite:** `trash` de ficheiro sob escopo funciona e é reversível; `trash` fora do escopo é
  negado; nada em `.katu/trash` é servido ao modelo por omissão; nenhum processo apaga a lixeira
  sem aprovação humana; a política **não** trata `bash rm` como equivalente a `trash`, mas prefere
  `trash` quando a regra o exigir; `refs` antes de apagar e `undo_token` no retorno (DF12).

### E06-T10 ☐ Tool `memory` (pedido explícito, policy-gated, secundária)
- **Objetivos:** o modelo pode **solicitar explicitamente** gravar memória, mas os hooks/fases do
  protocolo continuam **prioritários** (a tool não os substitui nem contorna).
- **Entregáveis:** tool `memory` (ex.: `memory.record`/`memory.search`) no **grupo de controlo**,
  fora das cinco famílias de codificação; a chamada passa pela **mesma** política e pelas mesmas
  pré-condições de fase (`pre_write`/dedup/âncora/`outcome`); a tool só **pede** — quem grava é a
  porta `Memory` (E03/E05).
- **Aceite:** usar a tool nunca contorna `pre_write`/dedup/âncora; uma gravação pedida pelo modelo
  sem âncora é negada tal como num hook; desligar a tool **não** desliga o enforcement (o protocolo
  continua pelos hooks/fases).

---

### E06-T11 ☑ Mover/renomear (`move`, atómico)
- **Entregáveis:** `move` atómico (`rename`) sob escopo, sujeito à política; invalida o índice/cache
  do caminho antigo e do novo; devolve o novo `id`/`hash`; em lote, atualiza referências uma vez.
- **Estado:** `Fs::rename` (porta; `fs.rename`) + `MoveFileTool` (`tool.move`); recusa destino
  existente (`Unavailable{exists}`) e origem ausente (`Unavailable{missing}`); devolve `from_id`/
  `to_id`/`hash` e `next: read <to_id>`. Testes: move+relatório, não-sobrescrita, origem ausente e
  `MemFs::rename`. Índice/cache e referências em lote chegam com E07/E09 (não há índice no MVP).
- **Aceite:** `move` de ficheiro sob escopo funciona e é reversível; `move` fora do escopo é negado;
  nenhuma referência fica pendurada (a busca após o `move` encontra o caminho novo); o índice não
  serve o caminho antigo.

### E06-T12 ☑ Formato AI-first (envelope + TOON + JSON)
- **Entregáveis:** `ToolReport` tipado; emissor **TOON** próprio em `katu-core::toon` (canónico,
  sem `null`, vazios omitidos, ordem canónica, zero deps); `format=json`/`--json` como alternativa;
  `cost` (bytes/tokens estimados/ms) com base DF5; IDs content-addressed estáveis.
- **Estado:** `katu_core::toon` (com `Block` literal e `Flow` inline, `Serialize` para JSON) +
  `katu_core::report` (`ToolReport`/`Page`/`Cost`/`content_id`/`content_hash`). O envelope é
  transportado por `ToolOutput`/`Dispatch::report()` (o `Tool` trait devolve estado + payload).
  Golden + proptest do TOON; TOON/JSON coerentes; vazios omitidos. A flag `--json` do CLI reusa o
  `to_json` numa fase de wiring.
- **Aceite:** golden + proptest do emissor (determinismo byte-a-byte); o mesmo `ToolReport` em TOON
  e JSON reconstrói a mesma informação; nenhum campo vazio/`null` é emitido; o JSON é válido.

## Deferred (não-mínimo, registado)

| Ferramenta | Por que fica fora agora | Condição de retomada |
|---|---|---|
| `git` | Não é um item do core; `bash git …` cobre o caso | decisão registada + consumidor atual |
| LSP / diagnóstico | Superfície grande; fora do kernel mínimo | fase futura |
| Browser, imagem, web | Fora de CLI+TUI (G7) | — |

---

## Definition of Done

- [ ] E06-T01…T10 concluídas.
- [ ] Superfície = exatamente as famílias de §1.1 (com `move`/`trash` na família de **Escrita**);
  `deferred` auditável.
- [ ] `Partial` tratado em todos os consumidores; "nunca Ok com erros" testado.
- [ ] Tool-schema linter no CI; `cargo xtask check` e job `msrv` verdes.

## Não-objetivos

- Sandbox de SO (E07). Aqui a execução já é limitada por capacidades, mas a contenção real é E07.
