# ADR 0017 — Política de recursos e runtime mínimo

- **Estado:** aceite
- **Data:** 2026-10-01
- **Decisões fundacionais:** DF8 (provider commodity), D92 (disciplina de qualidade)
- **Épicos:** E01-T09, E03-T04, E10 (executor em background), E12
- **Relaciona:** [ADR 0011](0011-porta-provider-e-builtin-opencode.md),
  [plan/02](../plan/02-fundacao.md), [plan/04](../plan/04-contrato-da-porta-memory.md)

## Contexto

Com I/O real (providers E12, processo E06) e a promessa de "CLI magra", era preciso decidir o
**runtime** e os **tetos de recurso** antes de qualquer trabalho assíncrono. E03-T04
(`spawn_blocking` + timeout no caminho async) estava explicitamente *gated* nesta decisão. Sem uma
política escrita, o risco é puxar um runtime pesado "por conveniência" e perder o determinismo.

## Decisão

1. **Worker bloqueante por padrão; nenhum runtime assíncrono no MVP.** O núcleo, o transporte e a
   sessão são síncronos; a TUI corre o turno no handler. O futuro executor em background (E10) usa
   **canal bounded + threads**, não `tokio`. `tokio` só entra se aparecer um requisito duro, com
   *features* mínimas e atrás de feature flag.
2. **Pool limitado a `available_parallelism()`** quando existir pool; sem threads por pedido.
3. **Timeouts tipados em toda a fronteira:** ligação/resposta no transporte, execução no `Process`
   (`Error::Timeout`); *retry/backoff* **só** em operação idempotente e antes do primeiro delta
   (E12-T04).
4. **Teto de corpo e de cache:** respostas não-streaming são limitadas (`GET_BODY_CAP` no
   transporte); o streaming é consumido **por delta**, nunca bufferizado inteiro; caches declaram
   o seu teto.
5. **Backpressure:** qualquer canal produtor/consumidor é **bounded**; uma rajada acima do teto
   bloqueia o produtor em vez de crescer memória.
6. **`spawn_blocking` para o núcleo bloqueante do knudge** quando o caminho async existir; até lá, a
   chamada síncrona é o caminho de produção (e é a que os testes cobrem).
7. **E03-T04 resolve-se por esta decisão:** sem caminho async não há `spawn_blocking` a fazer; a
   tarefa fica **não aplicável** até existir o executor/caminho assíncrono.

## Alternatives considered

1. **`tokio` *full*.** Rejeitada: runtime pesado e assíncrono para uma CLI bloqueante; aumenta o
   `cargo tree` e o tempo de build sem necessidade medida.
2. **`async-std`/`smol`.** Rejeitada: mesmo custo de dependência; a decisão de worker bloqueante já
   cobre o caso de uso.
3. **Canais *unbounded*.** Rejeitada: uma rajada do provider cresce memória sem limite.
4. **Uma thread por pedido.** Rejeitada: sem teto; usar `available_parallelism()`.
5. **`spawn_blocking` agora.** Rejeitada: não existe caminho async; introduziria um runtime para
   nada.

## Consequências

- **Positivas:** `cargo tree` sem `tokio`; recursos previsíveis; timeouts e tetos explícitos; o
  determinismo mantém-se.
- **Negativas / dívida:** o executor em background e I/O assíncrono ficam síncronos até haver
  necessidade real; E03-T04 adiado (não aplicável).
- **Travas:** `GET_BODY_CAP` no transporte; timeouts tipados; `gate:provider` no `make check`.
