# E03 — Porta `Memory` + adaptador in-process (knudge)

> **Fase 1.** Define a **porta `Memory`** com tipos do katu (DF6) — um módulo de `katu-core`
> (`katu_core::memory`) — e o adaptador **in-process** sobre `knudge-core` (o knudge é a memória do
> agente, G4), que vive no binário `katu` (`katu/src/memory/`) e é isolado por `check-layers`. A
> porta mantém a opção MCP viva, mas o MCP fica **fora do escopo atual** (`00b` §7,
> [`09`](09-adaptador-knudge.md)).
>
> **Decisões:** DF6, G4. **Depende de:** E01.
> **Gate do épico:** o contrato é substituível — trocar de in-process para outro adaptador muda
> **só** o adaptador, nunca o kernel.

---

## Princípio

Do §12–§13 da brainstorm — regras que mantêm a dependência **substituível**, não crítica:

1. **A porta é do katu, com tipos do katu.** Se `Memory` for definida em termos de
   `knudge_core::schema::Note`, nunca mais se troca.
2. **Um adaptador real agora, um futuro.** `in-process` (linka `knudge-core`) é **primário**;
   `mcp` é **futuro/deferido**.
3. **O núcleo puro do knudge é síncrono; o caminho do katu pode ser async.** Chamar via
   `spawn_blocking` + timeout — senão um I/O travado **congela** o agente.
4. **`0.x` e release deliberado.** Pinar versão; git-dependency durante a co-evolução.
5. **A porta existe para ser testável com `FakeMemory`** e para não acoplar o kernel a um backend.

---

## Pré-requisito externo

- `knudge-core = "0.5"` (v0.5.2) publicado, ou git-dep pinada. O katu **não** implementa o motor
  de memória; apenas o liga e fiscaliza o seu uso (§11). Fonte da verdade de integração:
  [`knudge/wiki/integration/`](../crates/knudge/wiki/integration/README.md).
- MSRV alinhado: knudge e katu em **Rust 1.97.0** (edição 2024) — coerente com o ponto inflexível.

---

## Alinhamento com o `knudge-core` (v0.5.2)

O guia de integração do núcleo ([`knudge/wiki/integration/`](../crates/knudge/wiki/integration/README.md))
documenta a API real que o adaptador vai traduzir. Restrições a incorporar:

- **Versão e disciplina.** `knudge-core = "0.5"` (v0.5.2); MSRV 1.97.0/edição 2024;
  `#![forbid(unsafe_code)]`, sem panic — a mesma disciplina de `D92`.
- **`Knudge` não é `Sync` nem `Clone`.** Segura o estado da sessão: o adaptador **não** guarda um
  `Knudge` compartilhado; reabre `Store`/`WriteContext` por thread ou protege com `Mutex`
  (combina com E03-T04).
- **Layout injetado (`D214`).** Default `.knudge` (`KnudgeBuilder::knowledge_dir`, `Project::at`);
  não há chave de config. Se o katu embute e o `kd` opera no projeto, ambos têm de usar o **mesmo**
  `knowledge_dir`.
- **Leitura tolerante × escrita estrita + `behavior.strict`.** O núcleo devolve resultado parcial
  + `warnings[]` (`R33`) e, com `strict = true`, o aviso vira `Error`. O adaptador fixa `strict`
  explicitamente — senão validações *soft* (âncora/slots/claims) passam silenciosas.
- **A persistência é por nota.** `write::write(&ctx, &draft, &thresholds)` faz dedup + decisão
  atómica e devolve `WriteAction::{Created, Merged, Rejected, Unchanged, Updated}`; `task::submit`
  cria tarefas/epics (`write` recusa, `D93`); `store::commit` grava nota+evento (`D20/D21`). O
  `knudge_session_end` do MCP é **hint**, não commit. → Ponto em aberto **OA8**.
- **Dedup.** `create_below = 0.75` / `merge_below = 0.92` vêm de `[dedup]`; o `≥ 0.92` da política
  é esse limiar traduzido, não uma constante nova.
- **Fecho por evidência.** `health::close_task(&ctx, id, &[CheckOutcome], actor)` **exige** evidência
  e infere o `outcome`; é o caminho de E05-T04.
- **Hook nativo.** `ports::HookRunner` valida/muta o `Draft` antes da gravação (mapeamento
  `hooks.pre_record` na borda): *defense-in-depth* com o enforcement do loop (DF1).
- **Portas e fakes espelháveis.** `Clock`/`Rng`/`Fs`/`Env`/`Git`/`HookRunner`/`Logger`/`Embedder`
  com fakes (`FixedClock`, `SeqRng`, `MemFs`, `FaultyFs`, `FakeGit`, `FakeEmbedder`,
  `RecordingLogger`) — a mesma forma nas portas do katu permite reusar a disciplina (`D65`).
- **Embeddings opcionais.** Sem provedor, o recall usa lexical + âncoras; o `Embedder` é porta
  plugável (`D79`). Provider de LLM (E12) ≠ embedder.
- **Erros e exit codes.** `ErrorKind` é o contrato (`NotFound`/3, `InvalidInput`/2, `Conflict`/4,
  `Io`/5, `Timeout`/6, `Config`/7, `Schema`/8, `UnsafeBlocked`/9, `Internal`/70; `retryable()` só
  em `Timeout`). Preservar `kind()`; o CLI pode reusar a tabela.

---

## Contrato conceitual

```rust
/// Tipos do katu. Nenhum tipo do knudge aparece aqui (DF6).
pub trait Memory: Send + Sync {
    fn pre_write(&self, req: &PreWriteReq) -> Result<PreWriteOutcome, MemoryError>;
    fn pre_edit(&self, req: &PreEditReq) -> Result<PreEditOutcome, MemoryError>;
    fn session_end(&self, req: &SessionEndReq) -> Result<SessionEndOutcome, MemoryError>;
    fn status(&self) -> Result<MemoryStatus, MemoryError>;
}

pub struct PreWriteReq { pub statement: String, pub note_type: NoteType,
                         pub anchor: Option<Anchor>, pub body: String }
pub enum PreWriteOutcome {
    Create, Merge { target: NoteRef, score: Score },
    Reject { duplicate: NoteRef, score: Score },   // ≥ 0.92
}
pub struct MemoryError { pub kind: MemoryErrorKind, pub retryable: bool,
                         pub message: String }
pub enum MemoryErrorKind { Unavailable, Timeout, Invalid, Internal }
```

**Bases de evidência:** todo score/estado regressa com a base (`measured`/`inferred`) — DF5.

---

## Tarefas

### E03-T01 ☑ Porta e tipos do katu (em `katu-core`)
- **Entregáveis:** módulo `katu_core::memory` com `Memory`, `PreWriteReq/Outcome`,
  `PreEditReq/Outcome`, `SessionEndReq/Outcome`, `MemoryStatus`, `NoteRef`, `Anchor`, `Score`,
  `MemoryError`; `record` (commit por nota, OA8) — usado por E05-T01.
- **Estado:** implementado em `crates/katu-core/src/memory.rs` + `memory/{types,io,error,fake}.rs`.
  `Score` é **pontos base** (`0..=10_000`, sem `f32`, E18-T01); `MemoryError::retryable()` só em
  `MemoryErrorKind::Timeout`; enums públicos `#[non_exhaustive]`. `FakeMemory` conta `record`
  (prova de "sem efeito" quando a política nega).
- **Aceite:** `xtask check-layers` falha se `knudge-core` aparecer fora do módulo do adaptador
  (`katu/src/memory/`); nenhum tipo do knudge na API pública.

### E03-T02 ☐ Adaptador in-process (primário, no binário)
- **Entregáveis:** no binário `katu` (`src/memory/`): dependência `knudge-core = "0.5"` (v0.5.2) ou
  git-dep pinada; feature `memory-in-process`; montagem via fachada `KnudgeBuilder` (adaptadores
  `std` + `Project` + config); tradução de tipos katu ↔ knudge com proptest de round-trip; `behavior.strict` fixado (avisos *soft* do knudge promovidos a erro); sem perda
  silenciosa (lacunas explícitas `None`, §15.3).
- **Aceite:** o adaptador cumpre a suíte de conformidade; nenhum tipo do knudge vaza para a API
  pública; `cargo tree` mostra `knudge-core` **apenas** em `katu/src/memory/`.

### E03-T03 ☐ Construtor à moda `build_with_transport()`
- **Entregáveis:** `build_with_memory(adapter)` (molde do `HttpTransport` do open-mtr, §16);
  seleção por feature; `FakeMemory` no núcleo para teste.
- **Aceite:** um teste de kernel usa `FakeMemory` e **não** puxa `knudge-core`; o binário de
  produção usa o adaptador in-process.

### E03-T04 ☐ `spawn_blocking` + timeout no caminho async
- **Entregáveis:** wrapper que executa o adaptador fora do hot path, com timeout e cancelamento
  (o núcleo do knudge é bloqueante, embedding HTTP inclusive).
- **Aceite:** um adaptador artificialmente bloqueante **não** congela o loop; o timeout é
  observável e não vaza tarefas.

### E03-T05 ◐ `FakeMemory` e suíte de conformidade
- **Entregáveis:** `FakeMemory` com cenários (`Create`/`Merge`/`Reject`) e scores fixos; suíte de
  conformidade do contrato.
- **Estado:** `FakeMemory` feito (`memory::fake`, com `rejecting`/`failing`/`with_hits` e falha
  injetável). A suíte de conformidade `memory::assert_contract` (E03-T05) corre contra o fake e
  fica pronta para o adaptador in-process (E03-T02) e o futuro MCP (E08). **Falta** correr a suíte
  contra o adaptador in-process (bloqueado por E03-T02, que precisa do `knudge-core`).
- **Aceite:** a suíte passa com o fake **e** com o adaptador in-process; fica pronta para reuso
  pelo futuro adaptador MCP (E08), testando paridade entre backends (§16.6).

### E03-T06 ☐ **Gate do épico:** substituibilidade
- **Objetivo:** provar DF6 com um teste de sanidade.
- **Entregáveis:** uma ADR (em E14) e um script `xtask check-memory-swap` que compila o binário
  **uma vez com `memory-in-process`** e **outra com a feature desligada** (`FakeMemory`).
- **Aceite (gate):** as duas compilações diferem **só** no adaptador; nenhum ficheiro do kernel
  muda. Este é o "se amanhã voltarmos ao MCP, quantos ficheiros mudam?" (§13).

### E03-T07 ☐ Memória como invariante, não como plugin (G4)
- **Entregáveis:** o binário de produção **sempre** com memória ativa; não existe modo "sem
  knudge"; `status()` exposto ao kernel para a UI/estado.
- **Aceite:** o build de produção não compila sem um adaptador de memória; o kernel recusa
  arrancar se `status()` falhar (fail-closed, DF4).

---

## Definition of Done

- [ ] E03-T01…T07 concluídas.
- [ ] Nenhum tipo do knudge vaza para a API do katu.
- [ ] Conformidade verde com `FakeMemory` e com o adaptador in-process.
- [ ] `cargo xtask check` e job `msrv` verdes.

## Não-objetivos

- Nenhuma lógica de dedup/âncora **dentro** do katu: isso é do knudge. O katu **fiscaliza** o uso
  do protocolo (E02/E05), não o reimplementa (§11).
- **Adaptador MCP: deferido** (E08, fora do escopo atual). A porta mantém a opção aberta, mas
  nenhum código de MCP é escrito agora (`00b` §7).
