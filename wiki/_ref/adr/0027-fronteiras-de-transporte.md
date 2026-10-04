# 0027 — Fronteiras de transporte fechadas: SSE no provider, kernel in-process

- **Estado:** aceite
- **Data:** 2026-10-03
- **Decisões fundacionais:** DF8 (provider commodity), DF5 (evidência tipada), DF9 (instrumentação)
- **Épicos:** E12 (T06/T07), E18 (latência), `KERNEL_SURFACE` (F4)
- **Relação:** reafirma [ADR 0012](0012-catalogo-dialetos-e-providers-declarativos.md) §3 e
  [ADR 0013](0013-cache-de-prefixo-e-compressao-de-pedido.md) §5 (HTTP/2/WebSocket) e **rejeita a
  fase F4** (`katu serve`) do [`KERNEL_SURFACE`](../plan/KERNEL_SURFACE.md)

## Contexto

Duas fronteiras de transporte do katu estavam a ser tratadas como "dívida em aberto" sem o serem.

1. **Provider ↔ endpoint.** O `katu-providers` fala HTTP/1.1 com **SSE incremental**: o parser
   `crates/katu-providers/src/sse.rs` consome `text/event-stream` por delta (sem buffer integral),
   o `crates/katu-providers/src/wire.rs` é o driver com retry e o
   `crates/katu-providers/src/engine.rs` pede `accept: text/event-stream`. O HTTP/2 está bloqueado
   pelo `ureq` 3 (HTTP/1.1) e o WebSocket é `Unsupported` (ADR 0012/0013). Ou seja: **streaming não
   é um item pendente — é o substrato**; HTTP/2/WebSocket são decisões fechadas, não lacunas
   silenciosas.
2. **Superfície ↔ kernel.** A `KERNEL_SURFACE` F4 propunha um `katu serve` (o protocolo sobre Unix
   socket/stdio) como fase condicional. O protocolo `Command`/`Event` já é `serde`-pronto, mas
   **não há segundo cliente**: a TUI é o único front-end com loop próprio e o CLI é sequencial.

A pergunta: **compensa trocar de stack (HTTP/2) ou de fronteira (processo) sem uma segunda parte
interessada?**

## Decisão

**Fechar as duas fronteiras no estado atual. O ganho é condicional a uma necessidade que não
existe; a troca pagaria dependências e complexidade sem contrapartida medida.**

1. **Provider: HTTP/1.1 + SSE, definitivo por agora.** O streaming é por delta (*latency-first*,
   ADR 0014) e não se troca por HTTP/2 sem multiplexagem a ganhar.
2. **HTTP/2 e WebSocket ficam `Unsupported`** (reafirma ADR 0012 §3 e ADR 0013 §5), com as
   condições de revisita abaixo — nunca um caminho silencioso.
3. **Sem fronteira de processo.** O kernel mantém-se uma **thread in-process**; o protocolo
   permanece `serde`-serializável como **opção barata de futuro**, não como fase comprometida. F4
   (`katu serve`) é **rejeitada**.

**Condições para revisitar** (o ADR não se edita; abre-se uma nova ADR se se cumprirem):

1. **HTTP/2:** um endpoint de modelo em uso **exige** HTTP/2 (recusa HTTP/1.1), ou há muitos
   streams concorrentes onde a multiplexagem paga o `tokio`/`h2`.
2. **WebSocket:** um modelo em uso exige WebSocket e não HTTP/SSE.
3. **Processo:** aparece um **segundo front-end real** (outro processo, isolamento de falhas ou
   cliente remoto) que não possa viver na thread do kernel.

## Alternatives considered

1. **Trocar o transporte do provider para HTTP/2 (`hyper`/`reqwest`/`h2`).** Rejeitada: sem
   multiplexagem a ganhar (um só stream por turno), pagaria `tokio` e uma reescrita do adaptador
   bloqueante; o custo supera o ganho medido (ADR 0013 §5).
2. **Adotar WebSocket no provider.** Rejeitada: sem evidência de que um modelo em uso o exija; a
   rota fica tipada `Unsupported` (ADR 0012 §3).
3. **`katu serve` sobre socket/stdio (F4).** Rejeitada: um só front-end, sem requisito de
   isolamento; pagaria IPC + versionamento de protocolo + um modo de falha novo, sem ganho. O
   protocolo continua `serde`-pronto, pelo que a opção não se perde — apenas não se compromete.
4. **Bufferizar a resposta (sem SSE).** Rejeitada: contraria o *latency-first* (ADR 0014) e o
   painel de atividade ao vivo (E10-T05); o SSE incremental é o caminho medido.

## Consequências

- **Positivas:** zero `tokio`/`h2`/WebSocket no binário; o SSE incremental medido mantém-se o
  substrato; a fase F4 deixa de ser uma dívida morta; a fronteira de superfície é honesta (uma
  thread, um dono).
- **Negativas / dívida:** a validação **ao vivo** dos dialetos `responses`/`messages`/`google`
  continua pendente (faltam credenciais de modelos que os usem); o WebSocket fica `Unsupported`;
  `prompt_cache_retention` não observado ao vivo (ADR 0013); `bench/e18/cancel/` + `gate:cancel`
  por medir (`LOOP_RESILIENCE`).
- **Travas:** testes do parser `sse` e do `wire`; `gate:provider` (orçamento de overhead, ADR 0014);
  o teto **total** do corpo no transporte (G2 do `KERNEL_SURFACE`); `cargo xtask check` verde.
