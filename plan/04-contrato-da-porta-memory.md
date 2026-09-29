# E03 — Porta `Memory` + adaptador in-process (knudge)

> **Fase 1.** Define a porta `Memory` com **tipos do katu** (DF6) e o adaptador **in-process**
> sobre `knudge-core` — o knudge é a memória do agente (G4), não um sidecar. A porta mantém a
> opção MCP viva, mas o MCP fica **fora do escopo atual** (`00b` §7, [`09`](09-adaptador-knudge.md)).
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

- `knudge-core` disponível como dependência (publicado em `0.x` ou git-dep pinada). O katu
  **não** implementa o motor de memória; apenas o liga e fiscaliza o seu uso (§11).
- MSRV alinhado: knudge e katu em **Rust 1.97.0** (edição 2024) — coerente com o ponto inflexível.

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

### E03-T01 ☐ Tipos do katu
- **Entregáveis:** `Memory`, `PreWriteReq/Outcome`, `PreEditReq/Outcome`, `SessionEndReq/Outcome`,
  `MemoryStatus`, `NoteRef`, `Anchor`, `Score`, `MemoryError`.
- **Aceite:** `xtask check-layers` falha se `knudge-core` aparecer fora do módulo do adaptador;
  nenhum tipo do knudge na API pública.

### E03-T02 ☐ Adaptador in-process (primário)
- **Entregáveis:** dependência `knudge-core` (git/`0.x` pinada); feature `memory-in-process`;
  tradução de tipos katu ↔ knudge com proptest de round-trip; sem perda silenciosa (lacunas
  explícitas `None`, §15.3).
- **Aceite:** o adaptador cumpre a suíte de conformidade; nenhum tipo do knudge vaza para a API
  pública; `cargo tree` mostra `knudge-core` **apenas** no módulo do adaptador.

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

### E03-T05 ☐ `FakeMemory` e suíte de conformidade
- **Entregáveis:** `FakeMemory` com cenários (`Create`/`Merge`/`Reject`) e scores fixos; suíte de
  conformidade do contrato.
- **Aceite:** a suíte passa com o fake **e** com o adaptador in-process; fica pronta para reuso
  pelo futuro adaptador MCP (E08), testando paridade entre backends (§16.6).

### E03-T06 ☐ **Gate do épico:** substituibilidade
- **Objetivo:** provar DF6 com um teste de sanidade.
- **Entregáveis:** uma ADR (em E14) e um script `xtask check-memory-swap` que compila o kernel
  **uma vez com `memory-in-process`** e **outra com feature desligada** (ligando `FakeMemory`).
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
