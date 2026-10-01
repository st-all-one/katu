# Q-13 · Hedging e backpressure no transporte — protocolo e resultado (**rejeitado**)

## Pergunta

O transporte deve fazer *hedged requests* (disparar um segundo pedido quando o primeiro demora) e
*backpressure* por ρ? (Item Q-13 do `OPTIMIZATION_PLAN.md` §Q-E: "TTFT remoto p50 2,3 s / p95 4,1 s;
sem hedge; buffer fixo".)

## O que foi medido

**1. TTFT do transporte local** (`provider-smoke`, 8 turnos, ligação quente): p50 ≈ **55 ms**,
p95 ≈ **59 ms**, total 518-580 ms com um prompt de 27 tokens; o primeiro pedido depois do *warm* pagou
351 ms. Com o prefixo canónico (2774 tokens) o mesmo transporte mede **29-32 s a frio** e **86 ms com
a cache quente** ([`bench/e18/prewarm`](../prewarm/PROTOCOL.md)).

**2. Um hedge não cabe no servidor.** Um *hedge* exige **dois pedidos em voo com o mesmo prompt**. Com
o prefixo canónico são 2 × 2774 tokens contra `n_ctx = 4096`: o servidor recusa o segundo pedido e o
turno volta vazio (medido em Q-14, com o log do servidor como evidência:

```
decode: failed to find a memory slot for batch of size 1024
slot operator(): need to evaluate at least 1 token for each active slot (n_past = 2774, task.n_tokens() = 2774)
```

**3. ρ (lei de Little) não se aplica.** O cliente é **bloqueante** e corre um turno de cada vez:
`ρ = λ × W` com `λ = 1` não define um alvo de paralelismo. O *pool* de ligações já existe
(`max_idle_connections_per_host(4)` no `UreqTransport`) e a ligação é reutilizada (o `--warm` prova-o:
o 2.º pedido cai de 351 ms para 54 ms).

**4. O buffer de stream não tem o que adaptar.** O buffer de leitura é reutilizado (stack, 4096 B) e,
desde o **P-04**, o parser SSE **não aloca por delta**; o overhead de cliente ponta a ponta medido é
~0,19 ms p50 / 0,24 ms p95 contra um orçamento de 5 ms (`gate:provider`).

**5. A taxa de acerto da cache de prefixo é publicada** (era o último pedido do item): o artefacto de
latência traz `usage.cache_hit_ratio` (0,8 no corpus canónico) e as corridas ao vivo trazem
`cached`/`input` (26/27 = 0,96 neste caso).

## Decisão: **rejeitado, com os números**

- **Hedge**: no servidor local é **impossível** (cache KV curta e partilhada); num provider remoto
  duplicaria um pedido **pago** e o ganho não é medível aqui (sem `KATU_OPENCODE_KEY`). Não se
  implementa o que não se consegue medir — a regra do E18 §0.3.
- **Backpressure por ρ**: não se aplica (λ = 1); o que existia (pool + keep-alive) já está feito.
- **Buffer adaptativo**: medido, não há alocação por chunk desde o P-04 e a folga contra o orçamento é
  de ~20×.
- **Taxa de acerto da cache**: já publicada.

## Limites

- Sem uma chave de provider remoto não há A/B de hedge onde ele teria valor; fica registado como
  **medida pendente de infraestrutura**, não como promessa.
- O servidor local (4096 tokens de KV, prefixo de 2774) é o caso mais desfavorável possível para
  concorrência; um endpoint com contexto folgado mudaria a conclusão do hedge — e só lá se re-mede.
