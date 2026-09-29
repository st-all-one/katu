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
| **Escrita** | `write`, `edit`, `trash` | altera ficheiros sob escopo, sempre sujeita à política; `trash` move para `.katu/trash` (recuperável) |
| **Leitura** | `read` | fonte de verdade para a fase `KnowledgeConsulted` |
| **Execução** | `bash` | `argv` resolvido + `Capability::Exec`; corre **como o utilizador que evocou o processo** (nunca eleva); a única superfície de execução |
| **Pesquisa de ficheiros** | `grep`, `find`, `ls` | otimizada: respeita ignore, varredura em streaming, só o delta; output canónico e truncado |
| **Planejamento** | `plan` (artefacto) | mantém `scope_contract` / `feature_list` e as transições de fase |

**Nada além disso entra agora.** `git`, LSP, browser, imagem, etc. são **`deferred`** com razão
registada e auditável por `xtask check-surface` (filtro de [`00b`](00b-objetivos.md) §4).

> **Controlos do kernel.** Compactar a conversa (E09-T07) e alterar modelo/grau de pensamento
> (E12-T10) são capacidades do [core](00b-objetivos.md) §1.1 #9/#10 — **não** acrescentam tools.
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

### E06-T01 ☐ Registry de tools fechado
- **Entregáveis:** registo tipado, namespace estável e ordem canônica; lista exata das cinco
  famílias.
- **Aceite:** qualquer tool fora do conjunto mínimo exige uma decisão registada (filtro `00b` §4);
  `xtask check-surface` falha ao exceder o teto; nenhum registo sem teste de teardown (§44).

### E06-T02 ☐ Tool-schema linter
- **Entregáveis:** linter que impõe `snake_case`, ordem verbo-substantivo, descrição "Use when X.
  Do not use for Y." (< 1024 chars), enums para conjuntos fechados, IDs tipados com `pattern`,
  e **erros que ensinam** (`Invalid input: 'city' is required. Example: {...}`); anti-poisoning
  (rejeitar `<SYSTEM>`, "ignore previous", markdown oculto).
- **Aceite:** o CI falha se um schema violar as regras; o erro de validação de exemplo é testado.

### E06-T03 ☐ Escrita e leitura (`write`, `read`, `edit`)
- **Entregáveis:** argumentos tipados, paths resolvidos, output determinístico; truncagem
  **determinística**; deltas (só o que mudou) como regra de contexto (§18).
- **Aceite:** propriedade: output canónico (ordenação estável, sem `HashMap`); teste de truncagem
  em limites minúsculos, exatos, chunks únicos enormes e multibyte (§45.22).

### E06-T04 ☐ Execução (`bash`) com argv resolvido e capacidades
- **Entregáveis:** execução que resolve `argv` e `cwd` **antes** da política; `Capability::Exec`;
  nenhuma decisão por regex sobre a string; o processo corre com o **uid/gid do utilizador que
  evocou o katu** (sem `sudo`/setuid; o sandbox só restringe, nunca amplia).
- **Aceite:** `cd x && rm`, `bash -c`, `find -delete`, `r''m` são avaliados sobre factos; o golden
  de E02-T05 cobre estes casos; `Deny` = efeito não ocorre; o filho herda o utilizador e nenhum
  caminho eleva privilégio.

### E06-T05 ☐ Pesquisa de ficheiros (`grep`, `find`, `ls`)
- **Entregáveis:** busca canónica com filtro de escopo e truncagem; **otimizada**: respeita
  `.gitignore`/ignore configurável, varredura em streaming (sem carregar o ficheiro inteiro) e
  paralela quando possível; resultado como **ponteiro** quando grande (só o delta chega ao modelo,
  §18).
- **Aceite:** output ordenado estavelmente; limite de resultados aplicado; teste com path fora do
  escopo é negado; um repositório grande é pesquisado sem pico de memória.

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
  `trash` quando a regra o exigir.

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

## Deferred (não-mínimo, registado)

| Ferramenta | Por que fica fora agora | Condição de retomada |
|---|---|---|
| `git` | Não é um item do core; `bash git …` cobre o caso | decisão registada + consumidor atual |
| LSP / diagnóstico | Superfície grande; fora do kernel mínimo | fase futura |
| Browser, imagem, web | Fora de CLI+TUI (G7) | — |

---

## Definition of Done

- [ ] E06-T01…T10 concluídas.
- [ ] Superfície = exatamente as cinco famílias (com `trash` na família de **Escrita**); `deferred` auditável.
- [ ] `Partial` tratado em todos os consumidores; "nunca Ok com erros" testado.
- [ ] Tool-schema linter no CI; `cargo xtask check` e job `msrv` verdes.

## Não-objetivos

- Sandbox de SO (E07). Aqui a execução já é limitada por capacidades, mas a contenção real é E07.
