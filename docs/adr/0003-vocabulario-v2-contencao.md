# ADR 0003 — Vocabulário de política v2: leitura sensível e acesso fora do workspace

- **Estado:** aceite
- **Data:** 2026-09-29
- **Decisões fundacionais:** DF2 (factos tipados), DF3 (regra como dado), DF4 (fail-closed),
  DF10/DF11 (recusa acionável, severidade decide o muro), DF12 (core por medição)
- **Épicos:** E02 (política), E07 (contenção soft), E14 (ADRs)

## Contexto

O MVP não tem jail de SO (E17): a contenção é **soft** e declarada. Faltava expressar, no
vocabulário **fechado** do motor (v1: `Path | Command | Phase | Budget` e 7 `Enforcement`), duas
travas exigidas por E07-T05:

1. **Sensíveis `deny`-by-default** (`.ssh`/`.env`/chaves) — negados mesmo **dentro** do workspace,
   porque a raiz do workspace é um grant amplo e não pode autorizar segredos.
2. **Fora do workspace → aprovação** — leitura/escrita fora da raiz exige controlo humano.

A raiz do workspace já é estado do kernel (`State::workspace`), mas a capacidade derivada era
`ReadPath`+`WritePath`, indistinguível de um grant **explícito** (aprovação humana). Sem essa
distinção, ou os sensíveis ficavam abertos (o workspace destrancava), ou o normal ficava fechado.

## Decisão

Subir `POLICY_VOCAB_VERSION` para **2** e alargar o vocabulário com o mínimo suficiente:

1. **`Enforcement::DenyRead { root }`** — nega leitura sob `root` sem capacidade de leitura.
   Destrancado por `Capability::ReadPath` **ou** `Capability::Workspace`.
2. **`Enforcement::DenySensitiveRead { globs }`** — nega leitura de um caminho cujo **componente**
   casa um glob sensível. Destrancado **apenas** por `Capability::ReadPath` explícito: o workspace
   **não** conta. O predicado é o glob determinístico partilhado (sem regex).
3. **`Capability::Workspace { root }`** — grant implícito da raiz. Substitui o par
   `ReadPath`/`WritePath` que `workspace_capabilities` derivava; `DenyWrite`/`DenyRead` aceitam-no,
   `DenySensitiveRead` não.
4. **Glob único** (`katu_policy::matches_glob`), movido de `katu-core::plan` e reexportado, para
   haver **uma** semântica (`*`/`?`) no projeto.

Regras em `policy/containment.toml`: `contain-sensitive-read` (critical), e
`contain-write-outside-workspace`/`contain-read-outside-workspace` (warn → `RequireApproval`).

## Alternatives considered

1. **`RuleScope::Sensitive` (glob no âmbito) + `DenyRead`.** Rejeitada: obrigaria `DenyRead` a
   inspecionar o âmbito para escolher o conjunto de capacidades, acoplando "onde" a "como". O
   `DenySensitiveRead` self-contido mantém a separação âmbito/enforcement.
2. **Deixar o workspace destrancar sensíveis.** Rejeitada: viola a aceitação de E07-T05 (`.env`
   dentro do repo seria legível sem autorização) e transforma um grant de conveniência em
   autorização de segredos.
3. **Uma variante `DenyRead { root, sensitive: bool }`.** Rejeitada: um booleano esconde dois
   conjuntos de capacidades diferentes; `struct_excessive_bools` e a clareza do vocabulário pedem
   variantes distintas.
4. **Regex no predicado sensível.** Rejeitada: o motor é puro e sem regex sobre entrada não
   confiável (DF2); o glob determinístico cobre os nomes e é o mesmo do contrato de escopo.
5. **Um `Capability::SensitiveRead { path }` por caminho.** Rejeitada: cresce sem limite e mistura
   *what* (ler) com *where* (o caminho); `ReadPath { root }` já exprime o grant explícito.
6. **Manter `workspace_capabilities` como `ReadPath`+`WritePath`.** Rejeitada: não permite separar
   implícito de explícito; era a raiz do problema.

## Consequências

- **Positivas:** sensíveis fechados por default **sem** bloquear o fluxo normal no workspace;
  fora do workspace é aprovação explícita; um único glob no projeto; recusa acionável com `rule_id`
  (`contain-sensitive-read`) e evidência estruturada.
- **Negativas / dívida:** o fluxo interativo de aprovação (`override_reason`+`granted_by`, E10)
  ainda não existe — hoje o pedido fica `Unavailable { control: "approval", rule_id }`. A busca
  (`grep`/`find`) já é avaliada como leitura (`is_read_tool` inclui `Search`; a raiz resolvida vai em
  `resolved_paths` via `search_use`) e `Capability::Net` está implementada (`argv::inspect` marca
  programas de rede e extrai o host; `is_plain()` recusa-os; só `Capability::Net { host }` os
  destranca).
- **Travas:** `katu-policy/tests/containment.rs` (matriz real do `policy/containment.toml`),
  `pipeline/tests.rs` (`sensitive_read_is_denied_even_with_a_workspace`), ledger
  (`containment.*`), `policy:audit` (exemplo negativo por regra `Enforced`).
