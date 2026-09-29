# E06 — Tools, capacidades e `ToolOutcome`

> **Fase 3.** As ferramentas de codificação e o contrato de resultado. Só depois de E05 passar.
> A superfície é **fechada**: exatamente as cinco famílias de [`00b`](00b-objetivos.md) §2 (G3).
>
> **Decisões:** DF2, DF3, DF4, G3. **Depende de:** E05.
> **Gate do épico:** a decisão é imposta **pela operação que a toma**, testada pelo executor;
> `Partial` é cidadão de primeira classe.

---

## Conjunto mínimo (G3) — a superfície inteira

| Família | Ferramentas | Nota |
|---|---|---|
| **Escrita** | `write`, `edit` | altera ficheiros sob escopo, sempre sujeita à política |
| **Leitura** | `read` | fonte de verdade para a fase `KnowledgeConsulted` |
| **Execução** | `bash` | `argv` resolvido + `Capability::Exec`; a única superfície de execução |
| **Pesquisa de ficheiros** | `grep`, `find`, `ls` | só o delta; output canónico e truncado |
| **Planejamento** | `plan` (artefacto) | mantém `scope_contract` / `feature_list` e as transições de fase |

**Nada além disso entra agora.** `git`, LSP, browser, imagem, etc. são **`deferred`** com razão
registada e auditável por `xtask check-surface` (filtro de [`00b`](00b-objetivos.md) §4).

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
  nenhuma decisão por regex sobre a string.
- **Aceite:** `cd x && rm`, `bash -c`, `find -delete`, `r''m` são avaliados sobre factos; o golden
  de E02-T05 cobre estes casos; `Deny` = efeito não ocorre.

### E06-T05 ☐ Pesquisa de ficheiros (`grep`, `find`, `ls`)
- **Entregáveis:** busca canónica, com filtro de escopo e truncagem; resultado como **ponteiro**
  quando grande (só o delta chega ao modelo, §18).
- **Aceite:** output ordenado estavelmente; limite de resultados aplicado; teste com path fora do
  escopo é negado.

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

---

## Deferred (não-mínimo, registado)

| Ferramenta | Por que fica fora agora | Condição de retomada |
|---|---|---|
| `git` | Não é um dos cinco mínimos; `bash git …` cobre o caso | decisão registada + consumidor atual |
| LSP / diagnóstico | Superfície grande; fora do kernel mínimo | fase futura |
| Browser, imagem, web | Fora de CLI+TUI (G7) | — |

---

## Definition of Done

- [ ] E06-T01…T08 concluídas.
- [ ] Superfície = exatamente as cinco famílias; `deferred` auditável.
- [ ] `Partial` tratado em todos os consumidores; "nunca Ok com erros" testado.
- [ ] Tool-schema linter no CI; `cargo xtask check` e job `msrv` verdes.

## Não-objetivos

- Sandbox de SO (E07). Aqui a execução já é limitada por capacidades, mas a contenção real é E07.
