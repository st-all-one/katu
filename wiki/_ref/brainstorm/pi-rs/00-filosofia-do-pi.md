# 00 — A Filosofia do Pi

> Documento central do dossiê. Não é sobre "como portar código", é sobre **como o Pi pensa** e como essa visão deve orientar a construção de um agente de IA confiável e performático em Rust.

Fontes primárias desta filosofia (especificações normativas dentro do próprio repositório):
- `_REF/pi/packages/agent/docs/harness.md` — runtime durável, três stores, máquina de estados.
- `_REF/pi/packages/agent/docs/assistant-durability.md` — durabilidade parcial do streaming.
- `_REF/pi/packages/agent/docs/plugins.md` — autoridade, facets, hosts.
- `_REF/pi/packages/agent/docs/rpc.md` — protocolo como contrato, não como vazamento.
- `_REF/pi/packages/coding-agent/docs/how-pi-works.md` — sessão, contexto, loop.

---

## 0. A tese central

O Pi não se define como "um CLI que fala com um LLM". Ele se define, nos próprios termos, como:

> "Um runtime **durável** para conversas de agente: ele persiste o estado da conversa e das operações para que **trabalho interrompido seja retomado sem repetir efeitos já liquidados**." (`harness.md §0.1`)

Essa frase contém a filosofia inteira. Um agente de IA confiável não é aquele que "quase sempre responde bem"; é aquele cujo **estado é explícito, durável e retomável**, e cujos **efeitos externos nunca acontecem duas vezes por acidente**.

A tese pode ser decomposta em três compromissos:

1. **Confiabilidade é uma propriedade do estado, não do modelo.** O LLM é não-determinístico; o que se pode tornar determinístico é o *runtime* em volta dele.
2. **Performance é uma propriedade do contexto e do I/O, não do hardware.** O custo dominante de um agente é o *prompt* (tokens/cache) e o número de round-trips, não a CPU.
3. **Honestidade sobre o que não é garantido.** O Pi documenta seus **non-goals** com o mesmo rigor dos goals. Um sistema confiável sabe exatamente onde não é confiável.

---

## 1. Confiabilidade: o que o Pi considera "sério"

### 1.1 O estado é durável, completo e total

Após **cada** transição durável, o harness substitui `operationState(operationId)` pelo estado **completo e total** — "nunca dependendo de um estado anterior". A recuperação:

> "após perda de tarefa, lê o estado e começa no procedimento responsável, **nunca reexecutando um journal nem inferindo posição a partir do que está faltando**." (`harness.md §0.3`)

**Por quê?** Um journal exige replay; replay exige que toda transição seja idempotente; e "inferir posição a partir do que falta" é exatamente o tipo de heurística que quebra sob falha parcial. O Pi escolhe o caminho oposto: **estado total + transições atômicas** tornam o crash *enumerável*.

Consequência de projeto, citada literalmente:

> "entre transações, nunca dentro de uma" (`harness.md §0.3`)

### 1.2 Três stores, uma invariante

> "Todo payload está em uma entrada, um value/list vinculado, ou no ledger; **não há um terceiro lugar**." (`harness.md §0.3`)

```
entries        árvore de conversa   — write-once, append-only
values/lists   estado mutável atual — values substituíveis; lists append-only
usage ledger   histórico de custo   — linhas append-only
```

Isso elimina a ambiguidade que destrói sistemas de agentes: *onde mora a verdade?* Se um payload pode estar em dois lugares, em algum momento os dois vão divergir. O Pi proíbe o terceiro lugar por contrato.

### 1.3 Efeitos externos: intent → efeito → settlement

A parte mais profunda da filosofia. Um efeito externo (chamada de tool, request ao provider) é uma **janela incerta**: você não sabe se aconteceu.

> "Provider requests e chamadas reais de tool são envolvidos em **dois commits** — intent ("prestes a fazer X; a saída usará ids R e U"), o efeito incerto, depois settlement (saída completa + próximo estado...)." (`harness.md §0.3`)

- O **intent** reserva ids e grava a intenção *antes* de agir. Isso torna a recuperação possível: ao voltar, o sistema sabe qual id esperar.
- O **efeito** é a janela em que o processo pode morrer sem deixar rastro.
- O **settlement** grava a saída completa de forma atômica.

E a consequência é aceita de forma explícita:

> "Todo efeito externo pode, portanto, acontecer sem settlement durável; intents tornam isso explícito onde a política de replay depende disso." (`harness.md §0.3`)

O corolário é um **non-goal** deliberado:

> "**Exatamente-uma-vez para efeitos externos** — hooks com efeitos colaterais devem ser idempotentes, chaveados por operation id." (`harness.md §0.6`)

O Pi **não finge** garantir exactly-once. Ele dá a *estrutura* (intent/settlement + ids reservados) para que a aplicação possa decidir replay policy (`replay: "never"` vs `replay: "safe"`), e exige idempotência onde não dá para garantir.

### 1.4 Recuperação repara por append, não por rewrite

> "Recuperação nunca repara por rewrite — ela **appenda entradas** e substitui apenas valores que ela possui, com as mesmas transições que a execução normal comitaria; interromper e rodar de novo dá o mesmo resultado; leitores nunca veem estado parcial." (`harness.md §0.3`)

Isso é o **determinismo de recuperação**: recuperar não é um modo especial, é a mesma máquina de estados continuando do ponto durável. Se recuperação tivesse caminho próprio, haveria dois sistemas para testar e eles divergiriam.

### 1.5 Um writer, um Drive

> "No máximo uma operação aberta por lane. Duas é corrupção." (`harness.md §9.1`, invariante 17)
> "No máximo um `Drive` existe por lane." (invariante 22)

A concorrência é resolvida por **propriedade explícita**, não por locks otimistas espalhados: `laneState(lane)` confere posse da lane; `operationState(operationId)` confere posse do estado da operação. Toda mutação suportada de uma lane commita através daquele dono. Isso torna "duas escritas concorrentes" não um bug a ser prevenido, mas um **estado inválido que não existe**.

### 1.6 Non-goals como parte da confiabilidade

O Pi lista o que **não** é meta — e isso é parte do design, não um rodapé:

| Non-goal | Implicação honesta |
|---|---|
| Exactly-once external effects | hooks idempotentes chaveados por operation id |
| Provider stream resumption | frames commitados preservam o parcial durável; nunca se reconecta ao stream |
| Múltiplos owners de escrita | exatamente um owner grava uma Session |
| Work scheduling | o harness não cria alarms/leases; reporta esperas via `drive` |
| Replicação | uma sessão vive em um lugar |
| Histórico de escrita durável | values retêm só o estado atual |
| Deleção como feature de runtime | entries/usage nunca são deletados; única exceção é a "precise rewrite" administrativa |

Um sistema que não sabe o que promete é um sistema que mente. O Pi define o perímetro da garantia.

---

## 2. Performance: onde o custo realmente está

### 2.1 A invariante append-only do contexto (a regra de ouro do KV cache)

> "**Invariante append-only de contexto.** Ao longo das requisições de uma lane, o contexto do provider só pode crescer na cauda: uma inserção antes da cauda da requisição anterior **invalida o KV cache do provider e multiplica o custo**." (`harness.md` §2.5)

Esta é a decisão de performance mais importante do Pi. Em LLMs, o custo e a latência do prompt são dominados por *cache hits*. Reescrever o histórico no meio do prefixo força o provider a reprocessar tudo. Por isso:

- **Writes mid-run são adiados para checkpoints**, onde appenda na cauda.
- **Compaction é a única invalidação deliberada**, trocada por um contexto menor.

A performance não vem de micro-otimização, vem de **nunca invalidar o prefixo**.

### 2.2 Streaming sem backpressure de storage

> "O loop do provider **nunca espera storage por frame**; appends de frame são enfileirados sincronamente na ordem de eventos do provider, e esperar a última promise de escrita no settlement do stream implica que todo append aceito completou." (`assistant-durability.md`, invariante 33)

O streaming é o caminho quente. Se cada delta esperasse um commit em disco, o I/O se tornaria o gargalo e o provider sofreria backpressure. O Pi:

- **enfileira sem await** cada frame;
- mantém apenas a referência da última promise;
- no fim, espera uma vez (o que implica todas as anteriores, pois a linha de mutação é FIFO).

### 2.3 Frames são observação, não autoridade

> "Assistant/deferred operation state é a **única autoridade de reinício** para parciais streamados. ... frames **nunca estabelecem conclusão do provider nem suprimem recuperação de desfecho desconhecido**." (`assistant-durability.md`, invariante 31)

Consequência de performance: como os frames são auxiliares e não autoridade, eles podem ser **compactos** (nunca um snapshot completo por evento) e descartáveis (deletados atomicamente no settlement). Isso evita *write amplification*.

### 2.4 Execução paralela de tools com ordem determinística

> "Chamadas de tool completas formam um **prefixo ordenado pela fonte**. ... outcome staging em ordem de conclusão **nunca estende o prefixo**; a materialização ordenada pela fonte estende." (`harness.md`, invariante 28)

Filosofia: **concorrência para latência, ordem para correção**. As tools rodam em paralelo (latência), mas as mensagens persistidas e a materialização seguem a ordem-fonte do assistente (reprodutibilidade). Efeitos concluídos fora de ordem ficam *staged* até que possam virar entradas contíguas.

### 2.5 Idempotência por memoização de invocação

> "Cada `invocationId` público de chamada de tool é seu `resultEntryId` reservado: único na sessão e **inalterado ao longo de replay seguro**." (invariante 27)

Isso é ao mesmo tempo correção e performance: um replay seguro reusa o memo em vez de reexecutar o efeito.

### 2.6 Leituras hot-path são index-driven, nunca fold de histórico

> "Nenhuma leitura em hot path pode dobrar histórico ou inferir estado de um valor ausente — não existe histórico de valores para dobrar." (`harness.md`, invariante 5)

O Pi proíbe deliberadamente APIs como "leia a lista inteira ilimitada". Leituras ordenadas são **paginadas e derivadas do estado tipado atual**. Performance e corretude andam juntas: sem história, não há fold caro nem heurística.

### 2.7 Budgets e limiares explícitos

- Protocolo: 16 MiB por frame, 1M elementos, 64 níveis.
- Subscriptions: no máximo 100 updates pendentes; overflow vira `reset` (snapshot completo).
- Frames: bytes limitados pelo output do modelo + overhead.
- Tool progress: checkpoints bounded.

Em vez de buffers ilimitados, o Pi escolhe **limites com política de overflow explícita**. Um agente performático não pode crescer sem limite sob carga.

---

## 3. As sete leis do Pi (síntese)

1. **Lei do estado total.** Um runtime confiável grava o estado completo após cada transição e nunca depende de replay.
2. **Lei do terceiro lugar.** Todo payload tem exatamente um lugar. Não existe terceiro.
3. **Lei da janela incerta.** Todo efeito externo é envolvido por intent e settlement; exactly-once é non-goal e idempotência é responsabilidade declarada.
4. **Lei do prefixo.** Contexto só cresce na cauda; compactação é a única invalidação permitida.
5. **Lei da observação.** Eventos e frames informam; nunca decidem. A autoridade é o estado da operação.
6. **Lei do dono único.** Uma lane tem um writer; uma operação tem um Drive. Concorrência vira estado inválido, não bug.
7. **Lei da honestidade.** Non-goals são documentados com a mesma precisão que goals.

---

## 4. Traduzindo a filosofia para o `pi-rs` (Rust)

Rust não é só "mais rápido": ele torna várias dessas leis **verificáveis pelo compilador**.

| Lei do Pi | Como Rust reforça |
|---|---|
| Estado total e tipado | `enum OperationState` com 13 variantes + `#[non_exhaustive]`; transição consome o estado antigo (`fn next(self) -> NextState`) e torna estados inválidos irrepresentáveis |
| Terceiro lugar proibido | `enum Payload { Entry(..), Value(..), Ledger(..) }`; o compilador não deixa um payload ter duas casas |
| Intent → efeito → settlement | tipos `Intent`, `Effect<Pending>`, `Settled`; `Effect<Pending>` não implementa `Commit` |
| Dono único | `Session` com `&mut self` para mutação; `begin_mutation()` devolve um `MutationGuard` que faz `Drop` = `end()` (usando RAII no lugar do `finally` do TS) |
| Append-only no contexto | builder de contexto que só permite `push_tail`; compactação é um tipo distinto (`CompactionRewrite`) que exige justificativa |
| Observação ≠ autoridade | `frames` em tipos `Auxiliary<T>` que nunca são aceitos por funções que exigem autoridade |
| Limites explícitos | `NonZeroUsize` para budgets; enums de overflow (`Reset { snapshot }`) |
| Cancelamento por invocação | `Context` (não durável) carrega `CancellationToken`; nunca é serializado (impedido por não implementar `Serialize`) |
| Idempotência | `InvocationId` novotype; memoização em mapa `InvocationId -> Outcome` |

### Implicações concretas de design

1. **Ownership como concorrência.** O "um writer por lane" vira `&mut Lane`, o "um Drive" vira posse exclusiva de uma task. Sem `Arc<Mutex>` no caminho quente.
2. **Transações como tipos.** Um `Transaction` só é materializado via `commit(self)`, consumindo-o. Não há "commit parcial".
3. **Recuperação é só mais um caminho.** A função `drive()` roda igual após crash; o teste de recuperação compara "recuperar ininterrupto" com "recuperar de prefixo", como o próprio Pi exige.
4. **Async sem await no hot path.** Frame appends entram numa fila FIFO; `await` acontece uma vez no settlement. Em Rust: `mpsc` bounded + `JoinHandle` da última escrita.
5. **Falha de provider é dado, não exceção.** Como o contrato do `pi-ai` exige, o stream emite evento `error` em vez de propagar `Err` depois do `start`.

### O que **não** copiar mecanicamente

- **Não** introduzir um journal/replay "porque é mais fácil em Rust". O Pi rejeita isso deliberadamente.
- **Não** prometer exactly-once de efeitos. Mantenha o non-goal e a idempotência por `operationId`.
- **Não** usar `Arc<Mutex<...>>` para "resolver" concorrência de lane. A lei do dono único é estrutural.
- **Não** guardar `partial` mutável compartilhado do stream. Emita snapshots coerentes com `contentIndex` (ver ADR-004).

---

## 5. Como validar que a filosofia foi respeitada

A própria metodologia de teste do Pi é parte da filosofia. Um `pi-rs` fiel deve adotar:

1. **Invariantes numerados.** Portar os 38 invariantes como testes nomeados.
2. **Race catalog.** Para cada corrida durável, exatamente **dois históricos** possíveis; testar os dois, com commits controlados.
3. **Conformance entre backends.** Memória, JSONL e SQLite produzem resultados idênticos, incluindo cursores de sequência e redução de frames.
4. **Afirmações de write-order.** Um decorator instrumentado sobre `commit()` verifica a ordem exata das escritas contra as tabelas de transação da especificação.
5. **Test tiers A/B/C.** A: máquina de estados/drive; B: conformance de writer; C: interleavings determinísticos.
6. **Ponto de reinício por prefixo.** Para cada prefixo de recuperação: fechar, reabrir, dirigir e comparar com a recuperação ininterrupta.
7. **Recuperação idempotente.** "Invocar recuperação duas vezes a partir do prefixo inicial **não** é suficiente."

Essa é a diferença entre "tem testes" e "tem uma teoria da confiabilidade".

---

## 6. Síntese

O Pi trata um agente de IA como **um sistema distribuído de um único nó com efeitos externos incertos**. Sua confiabilidade não vem de tentar eliminar a incerteza, mas de **modelá-la explicitamente**:

- estado total e durável (retomável);
- três stores e uma invariante (sem ambiguidade);
- intent/efeito/settlement (janela incerta explícita);
- um dono por lane (concorrência estrutural);
- non-goals honestos (perímetro de garantia claro).

Sua performance não vem de código rápido, mas de **respeitar a economia do provider**:

- contexto append-only (KV cache);
- streaming sem I/O bloqueante;
- frames auxiliares compactos;
- paralelismo com ordem determinística;
- limites explícitos com política de overflow.

Portar o Pi para Rust é, em essência, **usar o sistema de tipos para tornar as leis do Pi estruturais em vez de convenções**. O restante — providers, TUI, tools — é trabalho, mas a filosofia é o que decide se o resultado será confiável de verdade ou apenas compilado.

> "Toda transição substitui o valor completo; não há estado finished — a conclusão terminal o deleta." (`harness.md §3.2`)
>
> Essa frase resume o Pi: **nada parcial, nada implícito, nada esquecido.**
