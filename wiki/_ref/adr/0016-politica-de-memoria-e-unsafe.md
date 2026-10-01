# ADR 0016 — Política de memória e `unsafe`

- **Estado:** aceite
- **Data:** 2026-10-01
- **Decisões fundacionais:** D92 (disciplina de qualidade do knudge), G8 (determinismo)
- **Épicos:** E01-T08, E13 (Miri)
- **Relaciona:** [plan/02](../plan/02-fundacao.md), [`clippy.toml`](../../../clippy.toml),
  [`Cargo.toml`](../../../Cargo.toml)

## Contexto

O katu é um agente que corre comandos e escreve ficheiros; um `unsafe` mal isolado é um vetor de
UB no caminho de maior confiança. A disciplina `D92` do `knudge-core` já fixava o essencial; faltava
**publicá-la** como decisão de fase, para não se re-litigar e para o gate de E13 (Miri) ter um
contrato escrito.

## Decisão

1. **`#![forbid(unsafe_code)]` na raiz de todos os crates puros** (`katu-core`, `katu-policy`,
   `katu-tools`, `katu-providers`, `katu-tui`). `forbid` (não `deny`) porque **não** pode ser
   anulado por `#[allow]` local: a única forma de usar `unsafe` é mudar a política do crate. A única
   exceção registada é o binário `katu`, que declara `#![deny(unsafe_code)]` e tem **um** `#[allow]`
   local, listado em `xtask check-unsafe` (que exige que a lista esteja **exatamente** esgotada).
2. **`unsafe` só onde `std` não chega, com fronteira provada.** A exceção atual: `kill(-pgid,
   SIGKILL)` no `StdProcess` (E07-T04), para matar o **grupo** de processos no timeout — o único
   sítio sem wrapper seguro. Usa `#[allow(unsafe_code, reason = …)]` e um `// SAFETY:` **colado** ao
   bloco (`accept-comment-above-attributes = false`), provando a invariante (o `pgid` é o do grupo
   que criámos). Fora daí, zero `unsafe`.
3. **Tipos proibidos** (`clippy.toml`, `disallowed_types`): `Rc`/`Weak` (não `Send`/`Sync`),
   `RefCell`/`Cell` (interior mutability em runtime; use atómicos/`Mutex`), `LinkedList`
   (cache-hostil), `HashMap`/`HashSet` (iteração **não determinística** — G8; use `BTreeMap`/`Vec`).
4. **Sem atalhos de pânico:** `unwrap_used`/`expect_used`/`panic`/`todo`/`unimplemented`/
   `unreachable`/`dbg_macro` negados em `src/` (também nos testes); indexação `[]` negada (usar
   `.get()`); `mem::forget` negado; `Drop` determinístico.
5. **Aritmética defensiva:** `overflow-checks` no perfil; `checked_*`/`saturating_*` no caminho de
   custo/limites; conversões `as` negadas (usar `TryFrom`/`from`).
6. **Todo `#[allow]` exige `reason`** (`allow_attributes_without_reason = deny`).
7. **Abertura de ficheiros:** canonicalização/symlink rejeitado onde a política o exige
   (E07-T02); `try_reserve`/`Cow<'_, str>` onde couber.

## Alternatives considered

1. **Só `deny(unsafe_code)` (o nível do workspace).** Rejeitada como regra geral: um
   `#[allow(unsafe_code)]` local anularia a política sem revisão; `forbid` torna a exceção uma
   decisão de crate. Adotada **só** no binário `katu`, onde a exceção do `kill(-pgid)` é registada
   e verificada por `check-unsafe` (lista esgotada).
2. **Permitir `unsafe` no transporte/parse (hot path).** Rejeitada: não há ganho medido; o
   transporte bloqueante é seguro e o gate de latência (E12-T07) não exige `unsafe`.
3. **`HashMap` para o catálogo/política.** Rejeitada: iteração não determinística quebra G8 e os
   snapshots; `BTreeMap`/`Vec` linear são determinísticos.
4. **`Rc`/`RefCell` para o estado da TUI.** Rejeitada: não `Send`/`Sync`; o estado central é
   propriedade única e o resto é `Arc`/`Mutex`.

## Consequências

- **Positivas:** segurança de memória **por construção**; iteração determinística; nenhum UB fora
  do sandbox; o contrato é verificável por gate.
- **Negativas / dívida:** algumas APIs de conveniência ficam indisponíveis (mapas O(log n) em vez
  de O(1) amortizado); qualquer `unsafe` futuro obriga a uma ADR/feature.
- **Travas:** `#![forbid(unsafe_code)]` por crate (exceto o binário, `deny` + exceção registada);
  `xtask check-unsafe` exige que a lista de exceções esteja **exatamente** esgotada;
  `clippy -D warnings` com os `disallowed_types`; Miri verde (E13); `xtask check` no CI.
