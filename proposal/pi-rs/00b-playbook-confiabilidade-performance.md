# 00b — Playbook: Confiabilidade e Performance em Rust

> Companheiro prático de [`00-filosofia-do-pi.md`](./00-filosofia-do-pi.md). Aquele documento explica **por que**; este explica **como**, em Rust, com decisões, tipos e checklist.

---

## Parte I — Confiabilidade

### 1. Torne estados inválidos irrepresentáveis

**Princípio:** a lei do dono único e a lei do estado total não devem depender de disciplina do programador.

```rust
// O estado da operação é uma união total, não um struct com campos opcionais.
#[non_exhaustive]
pub enum OperationState {
    Starting { scope: OperationScope },
    NeedAssistant { scope: OperationScope, overflow_recovery_used: bool },
    AssistantReady { scope: OperationScope, config: ConfigSnapshot },
    EffectPending { scope: OperationScope, response_id: EntryId, usage_id: UsageId },
    Tools { scope: OperationScope, calls: Vec<ToolCallState>, result_ids: Vec<EntryId> },
    // ... 13 folhas totais
}
```

- **Transição consome o estado**: `fn transition(self, ...) -> Next`. Não se "muta no lugar" — evita transições parciais.
- **Sem estado "finished"**: a conclusão terminal *deleta* o estado da operação e grava um `ResultRecord`.
- `scope.control` carrega `Control::Running | Control::CancelRequested`; a invariante "aborted ⇒ cancel_requested" vira `debug_assert!` + teste.

### 2. Transações atômicas como tipos

```rust
pub struct Transaction { writes: Vec<Write> }        // não clonável, não reutilizável
impl Storage {
    pub async fn commit(&mut self, tx: Transaction) -> Result<Seq>; // consome tx
}
```

- `Transaction` não expõe construtor público de "parcial"; só se acumula via builder e se materializa com `commit`.
- `Seq` é monotônico global por sessão; gaps são legais.
- **Toda transação SQLite que pode escrever abre com `BEGIN IMMEDIATE`** (regra do Pi, com teste de regressão que falha sob `BEGIN` deferred).

### 3. Intent → Efeito → Settlement, codificado no tipo

```rust
pub struct Intent<P> { /* ids reservados, args */ }
pub struct Effect<P> { intent: Intent<P>, /* ... */ }     // P = Pending | Settled

// Só existe commit para settled; Effect<Pending> não implementa Commit.
pub trait Commit { async fn commit(self, storage: &mut Storage) -> Result<()>; }
impl Commit for Effect<Settled> { /* ... */ }
```

- `reserve_response_id()` e `reserve_usage_id()` acontecem **antes** do request.
- `replay: Never | Safe` é um campo do intent; a recuperação lê e decide.
- Efeito com `replay: Never` interrompido → settlement sintético com warning; `replay: Safe` → reexecuta com args persistidos.

### 4. Cancelamento: invocação, não durável

```rust
pub struct Context {
    pub cancellation: CancellationToken,
    // NÃO implementa Serialize/Deserialize — impedido em compilação
}
```

- `Context` é autoridade de invocação, nunca dado durável.
- Cancelar um `Context` **não** escreve `cancel_requested`; isso é feito por `request_abort()` (que é uma transição durável).
- Invariante: "cancelamento de invocação nunca vira cancelamento durável" → teste de tipo + teste de comportamento.

### 5. RAII no lugar de `finally`

O TS exige `end()` em `finally`. Em Rust:

```rust
pub struct MutationGuard<'a> { session: &'a mut Session, ended: bool }
impl Drop for MutationGuard<'_> {
    fn drop(&mut self) { if !self.ended { self.session.end_line(); } }
}
```

- `Session::mutate(|s| { ... })` sempre fecha por `Drop`.
- Não há caminho onde a linha de mutação fica presa por um `return` esquecido.

### 6. Recuperação é o mesmo caminho

- Não escreva um "modo recovery". `drive(operation_id)` lê o estado total e continua.
- Teste obrigatório: para cada prefixo de recuperação, comparar
  `recuperar(prefixo) == recuperar_ininterrupto`.
- Teste obrigatório: `recuperar(recuperar(prefixo)) == recuperar(prefixo)` (idempotência da recuperação).

### 7. Idempotência por identificador estável

```rust
pub struct InvocationId(EntryId);     // = result_entry_id reservado
pub struct InvocationMemos(HashMap<InvocationId, ToolOutcome>);
```

- `InvocationId` é novotype: não se confunde com outros ids.
- Replay seguro consulta o memo antes de executar.

### 8. Um writer, estruturalmente

```rust
pub struct Lane<'s> { session: &'s mut Session, state: LaneState }

impl Lane<'_> {
    pub async fn accept(&mut self, prompt: Prompt) -> Result<OperationId>; // &mut
    pub async fn drive(&mut self, op: OperationId) -> Result<DriveOutcome>;
}
```

- `&mut` garante um writer em tempo de compilação.
- Um `Drive` é uma futura que possui `&mut Lane` durante a execução; não há dois passos concorrentes da mesma lane.

### 9. Perímetro de garantia explícito

Documente no crate, com o mesmo destaque dos recursos:

```rust
//! # Non-goals
//! - Exactly-once external effects. Hooks must be idempotent.
//! - Provider stream resumption. Never reattach; frames preserve the durable partial.
//! - Replication. A session lives in one place.
//! - Deletion as a runtime feature. Entries/usage are never deleted.
```

---

## Parte II — Performance

### 10. Contexto append-only

```rust
pub struct ContextBuilder { tail: Vec<ProviderMessage> }   // sem insert/remove no meio
impl ContextBuilder {
    pub fn push_tail(&mut self, msg: ProviderMessage);
    // NÃO expor insert(index), remove(index), replace(index) para o caminho normal.
}
pub struct CompactionRewrite { /* a única operação que invalida prefixo */ }
```

- A API normal **não tem** como inserir no meio — a lei do prefixo vira contrato de tipo.
- `CompactionRewrite` é um tipo distinto que exige justificativa e é contabilizado na telemetria como "cache invalidation".

### 11. Streaming sem await de storage no hot path

```rust
// enfileira sem await; guarda só a última escrita
let mut last_write: Option<JoinHandle<Result<()>>> = None;
for event in provider_stream {
    let frame = encoder.encode(&event);
    if let Some(frame) = frame {
        last_write = Some(storage.enqueue_append(frames_addr.clone(), frame)); // sync enqueue
    }
    events.emit(event).await?;   // eventos esperam; storage não
}
// settlement: uma espera só
if let Some(h) = last_write { h.await??; }
```

- `mpsc` FIFO para a linha de mutação garante que esperar a última escrita implica todas.
- Nunca `await` por frame — medir com benchmark antes/depois.

### 12. Frames compactos e auxiliares

```rust
pub struct AssistantMessageFrame { /* deltas, não snapshot completo */ }
pub struct Auxiliary<T>(T);     // nunca aceito por funções que exigem autoridade
```

- Encoder mantém contadores por bloco aberto e trima prefixos já representados.
- Nenhum snapshot completo por evento.
- Deletados atomicamente no settlement.

### 13. Paralelismo com ordem determinística

```rust
// preflight sequencial; execução concorrente; materialização em ordem-fonte.
let mut set = JoinSet::new();
for (idx, call) in calls.iter().enumerate() { set.spawn(run_tool(idx, call)); }
// outcomes podem completar fora de ordem → stage
// materialize_src_order() coloca apenas o prefixo contíguo pronto
```

- Eventos de conclusão em ordem de conclusão; entradas persistidas em ordem-fonte.
- Se **qualquer** tool do batch for `Sequential`, o batch inteiro vira sequencial.

### 14. Limites com política de overflow

```rust
pub const MAX_FRAME_BYTES: NonZeroUsize = NonZeroUsize::new(16 * 1024 * 1024).unwrap();
pub enum OverflowPolicy { Reset { snapshot: Snapshot } }
pub const MAX_PENDING_UPDATES: NonZeroUsize = NonZeroUsize::new(100).unwrap();
```

- Buffers bounded; overflow tem política declarada (reset), não crescimento silencioso.
- `NonZeroUsize` elimina zero por tipo.

### 15. Leituras index-driven e paginadas

```rust
pub struct EntryCursor { seq: u64 }
pub struct Page<T> { items: Vec<T>, next: Option<EntryCursor> }
// não existe read_whole_list() ilimitado.
```

- Hot paths nunca fazem fold/scan completo; perfis de query são testados (`EXPLAIN QUERY PLAN`).
- Resultados ordenados antes do `limit`.

### 16. Writes paralelos em tabelas têm foco no custo real

- O Pi mede "pending-payload write amplification" (double write deliberado de itens enfileirados) antes de otimizar. Faça o mesmo: **benchmarks com `criterion` antes de mudar a estrutura**.

---

## Parte III — Verificação

### 17. Invariantes como testes nomeados

```rust
#[test] fn inv_17_at_most_one_operation_per_lane() { /* ... */ }
#[test] fn inv_22_at_most_one_drive_per_lane() { /* ... */ }
#[test] fn inv_31_operation_state_is_sole_restart_authority() { /* ... */ }
```

### 18. Race catalog com dois históricos

Para cada corrida, force ambas as ordens com commits controlados:

```rust
#[tokio::test]
async fn race_abort_vs_settlement_marker_first() { /* ... */ }
#[tokio::test]
async fn race_abort_vs_settlement_settlement_first() { /* ... */ }
```

Lista de corridas a testar (do Pi): prompt vs prompt; accept vs process loss; drive vs drive; stale drive vs current; abort vs settlement; abort vs tool outcome; checkpoint vs settlement; frame append vs settlement; live event vs queued commit; later tool vs earlier tool; cancel_queued vs boundary; set_model vs generation; close vs settlement.

### 19. Write-order assertions com storage instrumentado

```rust
pub struct InstrumentedStorage<S> { inner: S, writes: Arc<Mutex<Vec<Vec<Write>>>> }
```

- Afirma a **ordem exata** das escritas contra as tabelas de transação.
- Pega: efeito antes de intent; settlement sem delete da frame list; frames persistidos para `done`/`error`; `tool_end` antes do staging; outcomes não staged antes de replay impossível.

### 20. Conformance entre backends

- Uma suíte, três backends (memória, JSONL, SQLite), **resultados idênticos**.
- Inclui cursores de sequência, redução de frames, e "torn transaction não expõe elemento de lista".

### 21. Teste de tipo para isolamento de Context

- `static_assertions::assert_not_impl_any!(Context: Serialize)` — garante que contexto não vira dado durável.

---

## Parte IV — Checklist de implementação

Antes de considerar o núcleo do agente pronto:

**Estado e transações**
- [ ] `OperationState` é união total; transições consomem o estado anterior.
- [ ] Não existe estado "finished"; terminal deleta.
- [ ] `commit` consome a transação; sem commit parcial.
- [ ] SQLite usa `BEGIN IMMEDIATE` para escrita.

**Efeitos**
- [ ] Intent reserva response/usage/result ids antes do efeito.
- [ ] `replay: Never | Safe` persistido e honrado na recuperação.
- [ ] Hooks documentados como idempotentes (non-goal exactly-once).

**Concorrência**
- [ ] `&mut Lane` (um writer); um `Drive` por lane.
- [ ] `MutationGuard` fecha por `Drop`.
- [ ] Cancelamento por invocação não escreve estado durável.

**Contexto/performance**
- [ ] Context builder só appenda na cauda.
- [ ] Compactação é tipo distinto e telemetrada.
- [ ] Streaming não espera storage por frame.
- [ ] Frames são auxiliares e compactos.
- [ ] Tools paralelas com materialização em ordem-fonte.
- [ ] Buffers bounded com política de overflow.
- [ ] Leituras paginadas e index-driven.

**Verificação**
- [ ] Invariantes portados como testes nomeados.
- [ ] Race catalog com ambos os históricos.
- [ ] Write-order via storage instrumentado.
- [ ] Conformance nos três backends.
- [ ] Recuperação idempotente e comparada ao caminho ininterrupto.
- [ ] `Context` não serializável.

**Honestidade**
- [ ] Non-goals documentados no crate.
- [ ] Erros de provider viram evento `error` no stream após `start`, não `Err`.

---

## Parte V — Anti-padrões (o que reprova a filosofia)

| Anti-padrão | Por que viola o Pi |
|---|---|
| Journal + replay | viola a lei do estado total; exige idempotência universal |
| `Arc<Mutex<Lane>>` | viola a lei do dono único; esconde corridas |
| `insert/remove` no meio do contexto | invalida KV cache; viola a lei do prefixo |
| `await` de storage por delta | backpressure no provider; mata latência |
| `partial` mutável compartilhado | aliasing; o Pi já proíbe reter `partial` |
| Prometer exactly-once de efeito | desonestidade de contrato |
| Buffer ilimitado de updates | sem política de overflow; OOM sob carga |
| Inferir estado a partir do que falta | recuperação não determinística |
| Deletar entries/usage em runtime | viola "deleção não é feature de runtime" |
| Serializar `Context`/`AbortSignal` | mistura autoridade de invocação com dado durável |

---

## Síntese

O playbook reduz-se a quatro movimentos:

1. **Tipos que carregam as leis** (estado total, dono único, intent/settlement, contexto append-only).
2. **Hot path sem I/O bloqueante** (streaming enfileira, settlement espera uma vez).
3. **Recuperação como continuação** (mesmo caminho, determinístico, idempotente).
4. **Verificação como teoria** (invariantes, races, write-order, conformance).

Se esses quatro movimentos estiverem no `pi-rs`, a filosofia do Pi estará preservada — independentemente de quantos providers já foram portados.
