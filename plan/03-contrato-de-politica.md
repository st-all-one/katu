# E02 — Contrato do motor de política

> **Fase 1.** O coração da tese (DF2, DF3). Define as **estruturas de dados** e a **função pura**
> de veredicto antes de qualquer integração. É aqui que as regras de memória do knudge têm de
> caber — ou o projeto não se justifica.
>
> **Decisões:** DF2, DF3. **Depende de:** E01.
> **Gate do épico:** as regras do protocolo de memória (dedup ≥ 0.92, âncora obrigatória,
> `outcome` antes de fechar) são **todas** expressáveis no modelo.

---

## Objetivo do épico

Um motor de política que avalia **factos tipados** e devolve decisões com evidência, sem I/O,
sem relógio global e sem regex sobre texto. Testável isoladamente e determinístico.

---

## Modelo de dados (contrato conceitual)

```rust
/// Facto de uso de ferramenta, já resolvido (DF2).
pub struct ToolUse {
    pub name: ToolName,
    pub args: ToolArgs,            // tipado por tool, nunca String crua
    pub resolved_paths: Vec<ResolvedPath>, // canonicalizados
    pub argv: Option<ResolvedArgv>,
    pub cwd: ResolvedPath,
}

/// Estado do caminho único (DF1), como valor.
pub enum Phase { Task, KnowledgeConsulted, Planned, Implemented, Verified, Persisted, Closed }

/// Capacidades concedidas no contexto corrente (DF2, DF4).
pub enum Capability {
    ReadPath(PathRoot), WritePath(PathRoot), DeletePath(PathRoot), Exec(ExecSpec),
    Net(HostSet), SpawnPty, McpSession(SessionId),
}

/// Regra como dado versionado (DF3, DF7).
pub struct Rule {
    pub id: RuleId,
    pub statement: String,
    pub scope: RuleScope,               // Path | Command | Phase | Budget
    pub enforcement: Enforcement,
    pub severity: Severity,             // Critical | Warn
    pub category: RuleCategory,         // Enforced | Advisory | Perception
    pub expires_at: Option<Timestamp>,
    pub waiver: Option<Waiver>,
    pub examples: RuleExamples,         // neg/pos: um comando que ela nega
}

pub enum Enforcement {
    DenyCommand, DenyWrite, DenyDelete, RequireBefore(Phase), RequireAfter(ToolName),
    Budget(BudgetCap), Advisory,
}

/// Veredicto tipado e auditável.
pub enum Decision {
    Allow,
    Deny { reason: Reason, rule_id: RuleId, evidence: Evidence },
    RequireApproval { request: ApprovalRequest },
    NeedsHuman { reason: Reason, missing_control: ControlId },
}
```

**Regras de fronteira:**

1. O motor é **puro**: `fn evaluate(facts: &Facts, rules: &RuleSet) -> Decision`. Sem I/O.
2. Uma regra `DenyCommand`/`DenyWrite`/`DenyDelete` **só é aceite** se o motor conseguir
   demonstrar **um comando/ficheiro que ela nega** (§51.7). Sem exemplo negativo → vira `Advisory`
   e é rotulada.
3. `Evidence` é estruturada (`file:line`, facto, argumento, `rule_id`), nunca prosa (§29).
4. Regras com `expires_at` vencido entram em revisão (default 90 dias, §31).
5. **Sem jail no MVP; escopo global com travas.** O motor avalia capacidades sobre o host inteiro
   (o katu corre como o utilizador). Fora do workspace e caminhos sensíveis são
   `RequireApproval`/`NeedsHuman` por regra — não bloqueio de kernel; a jail real é futura (E17).

## Vocabulário fechado e versionado

O vocabulário do motor é **fechado por segurança** e **mínimo**: `RuleScope`
(`Path | Command | Phase | Budget`) e `Enforcement` (7 variantes) são a superfície **inteira**. Não
há expressões livres, callbacks nem DSL — uma regra é **dado**, e o seu poder é exatamente o que os
tipos permitem.

- **Versão:** `POLICY_VOCAB_VERSION` (inteiro) viaja com o `RuleSet`; o motor **recusa** carregar
  uma versão de vocabulário desconhecida (fail-closed).
- **Alargar** (novo `RuleScope`/`Enforcement`) é uma **decisão de kernel registada** (ADR em E14,
  com teste e exemplo negativo) — **nunca** configuração de utilizador. É o mesmo princípio que
  remete código arbitrário para plugin futuro (E11, OA14).
- **Critério de suficiência:** o vocabulário mínimo cobre o protocolo de memória (E02-T07). O que
  não couber indica ou que a regra está mal formulada, ou que o pedido pertence a um plugin.

---

## Tarefas

### E02-T01 ☑ Tipos de facto e de capacidade
- **Entregáveis:** `ToolUse`, `ResolvedPath`, `ResolvedArgv`, `Phase`, `Capability`, `Facts`.
- **Estado:** `katu-policy/src/{paths,facts}.rs`; `ResolvedPath` só constrói via `from_canonical`
  (absoluto + normalizado); proptest mostra que `../` é resolvido antes do veredicto. A resolução de
  symlinks fica no kernel (porta `Fs`) — o tipo é o invariante pós-resolução.
- **Aceite:** `ResolvedPath` só é construível a partir de canonicalização (construtor privado);
  proptest mostra que `../` e symlink são resolvidos antes de qualquer veredicto.

### E02-T02 ☑ Tipos de regra e de veredicto
- **Entregáveis:** `Rule`, `RuleScope`, `Enforcement`, `Severity`, `RuleCategory`, `Decision`,
  `Evidence`, `Waiver`, `RuleExamples`, `PolicyVocab` (`POLICY_VOCAB_VERSION`).
- **Estado:** `katu-policy/src/{rule,decision}.rs`; round-trip TOML preserva as regras;
  `#[non_exhaustive]` nos enums públicos; `RuleSet::from_toml` **recusa** vocabulário desconhecido
  (fail-closed).
- **Aceite:** round-trip TOML/JSON de `Rule` preserva ordem canônica; `#[non_exhaustive]` nos
  enums públicos; `Decision` não constrói estado inválido; o motor **recusa** um `RuleSet` com
  `POLICY_VOCAB_VERSION` desconhecida (fail-closed).

### E02-T03 ☑ Motor `evaluate` puro
- **Entregáveis:** avaliador determinístico, com ordem de custo crescente (§32): âmbito → casamento
  → orçamento. O veredicto de maior `rank` vence (`Allow < RequireApproval < Deny < NeedsHuman`).
- **Estado:** `katu-policy/src/{engine,evaluate}.rs`; testes de negação, dependência, orçamento,
  severidade e fail-closed; `evaluate` não toca `Clock`/`Fs`/`Rng`.
- **Aceite:** mesmo input → mesmo veredicto (proptest); nenhuma chamada a `Clock`/`Fs`/`Rng`;
  tempos medidos em microssegundos para um `RuleSet` de 30 regras.

### E02-T04 ☐ Categorias e auditoria de regras
- **Entregáveis:** `xtask policy:audit` que lista `Enforced` vs `Advisory`; falha se houver texto
  de instrução sem categoria; regras sem exemplo negativo não podem ser `Enforced`.
- **Aceite:** `E02-T04` tem teste próprio; o relatório é comparado com o esperado (DF3).

### E02-T05 ☐ Golden de veredictos e proptests
- **Entregáveis:** golden com matriz de factos (`../`, symlink, `bash -c`, `&&`, `find -delete`,
  `docker run`, `python -c`), cada um com `Allow`/`Deny`/`RequireApproval` esperado.
- **Aceite:** o golden falha se alguém trocar o motor por regex sobre string (DF2).

### E02-T06 ☐ Gate de cobertura de regras (ledger)
- **Entregáveis:** `coverage-ledger.json` de regras: cada regra com `coverage_id` canónico e
  estado explícito (`covered` | `not_applicable` | `deferred`), nos moldes do security-audit (§28).
- **Aceite:** nenhuma superfície fica sem regra nem sem decisão explícita; validador zero-dep
  (`xtask ledger:validate`) rejeita duplicados/colisões de `coverage_id`.

### E02-T07 ☐ **Gate do épico:** as regras do knudge cabem
- **Objetivo:** provar que o protocolo de memória é expressável.
- **Entregáveis:** `policy/memory.toml` com, no mínimo:
  - `deny write` quando `pre_write` aponta duplicata ≥ 0.92 → `DenyWrite`;
  - exigir `--anchor` em nota sobre código → `RequireBefore`/validação de argumento;
  - exigir `--outcome` antes de fechar tarefa → `RequireAfter`/pré-condição de fase;
  - uma afirmação por nota → `DenyWrite`.
- **Aceite (gate):** todas as regras acima são `Enforced` com exemplo negativo que nega; a lista
  `Advisory` resultante está **vazia** para o protocolo de memória. Se alguma não for
  expressável, o épico **não passa** e o projeto entra em revisão (§5 do README).
- **Rastreabilidade:** §11, §12, §51.7.

---

## Definition of Done

- [ ] E02-T01…T07 concluídas.
- [ ] `xtask policy:audit` e `xtask ledger:validate` verdes.
- [ ] As regras de memória da `policy/memory.toml` são todas `Enforced` com teste.
- [ ] `cargo xtask check` e job `msrv` verdes.

## Não-objetivos

- Nenhuma integração com o loop (E04) nem com a memória (E03/E05).
- Nenhuma tool real (E06). A política avalia factos; quem os produz é E04/E06.
