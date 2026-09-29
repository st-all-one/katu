# 12 — Observabilidade e performance

> Fontes: `grpc_docs/doc/binary-logging.md`, `trace_flags.md`,
> `ssl-performance.md`, `doc/cpp/perf_notes.md`.

## 1. Observabilidade

### 1.1 Logging binário

gRPC oferece um formato de **log binário** (grpc-proto `grpc/binlog/v1`) que
registra, por `rpc_id`: metadata inicial, mensagens, status/metadata final e
pares chave/valor adicionais. Partes do log podem ser gravadas em arquivos
separados e correlacionadas por `rpc_id`.

- API separada por linguagem; habilitação explícita.
- Útil para depuração/auditoria de tráfego sem parsear o wire manualmente.
- Cuidado com PII: logs binários podem conter payload.

### 1.2 Trace flags (C-core)

`GRPC_TRACE` ativa tracers internos (glob de nomes) e `GRPC_VERBOSITY` (deprecado)
define o nível. Exemplos:

```bash
GRPC_TRACE=http,transport,lb,subchannel,http_keepalive \
GRPC_VERBOSITY=DEBUG ./server
```

- Lista de tracers em `doc/trace_flags.md`.
- Use em debug, nunca em produção (custo e ruído).
- `GRPC_ABORT_ON_LEAKS=1` para detectar leaks em testes.

### 1.3 Logging de aplicação

- Registre `method`, `code`, `duration`, `peer`, `trace_id`.
- **Nunca** logue payload bruto; redija PII.
- Correlacione com `x-request-id`/`traceparent`.
- Use interceptors para log estruturado.

### 1.4 Métricas (RED)

- **Rate**: RPCs/s por método.
- **Errors**: por status code.
- **Duration**: latência p50/p95/p99.
- Labels de baixa cardinalidade (serviço/método/status), nunca IDs.
- Contadores de retry, timeouts, mensagens grandes, conexões.

### 1.5 Tracing (OpenTelemetry)

- Instrumentação gRPC OTel (`otelgrpc`, `opentelemetry-instrumentation-grpc`).
- Propague `traceparent`/`tracestate` em metadata.
- Spans por RPC; atributos de método/peer/status (sem payload).
- Exporte via OTLP para coletor (Jaeger/Tempo/Datadog).

## 2. Performance

### 2.1 Princípios

1. Meça com carga realista (`ghz`, benchmarks por linguagem).
2. Reutilize canais/conexões (não crie por chamada).
3. Prefira streaming para grandes volumes; evite mensagens gigantes.
4. Ajuste limites de mensagem sem exagerar (memória).
5. Use compressão só quando banda importa.
6. Mantenha TLS otimizado.
7. Faça LB para distribuir carga.

### 2.2 TLS / SSL performance

- TLS 1.3 reduz handshakes.
- **Session resumption** (tickets) e **ALPN**.
- Reutilize conexões (multiplexação HTTP/2) para amortizar handshake.
- Evite renegociação frequente e certificados grandes.
- Cifras AEAD (AES-GCM/ChaCha20) com aceleração de hardware.
- mTLS adiciona custo de handshake; cacheie sessões.

### 2.3 Tuning de servidor

| Parâmetro | Efeito |
|---|---|
| `max_receive/send_message_length` | evita `RESOURCE_EXHAUSTED` |
| Janelas HTTP/2 | throughput |
| `max_concurrent_streams` | concorrência por conexão |
| Keepalive | detecta conexões mortas |
| Thread pool / event loop | vazão |
| Polling engine (C-core) | `epoll`/`poll` (ver env vars) |
| Compressão | banda × CPU |

### 2.4 Gargalos comuns

- Serializar/parsear em caminho quente sem cache.
- Mensagens grandes (>4 MiB) → fragmentar/streaming.
- `pick_first` acidental → concentra carga em um backend.
- Falta de deadline → threads presas.
- Keepalive agressivo → GOAWAY.
- Log em nível DEBUG em produção.
- Copiar mensagens grandes em vez de mover/referenciar.
- Criar canal por chamada (handshake TLS repetido).

### 2.5 Streaming e flow control

- Respeite backpressure; ajuste janelas.
- Não acumule mensagens sem limite.
- Para throughput, aumente janelas com cautela (memória).

### 2.6 Benchmarks

- `ghz` (genérico), JMH (Java), `go test -bench`, criterion (Rust).
- Meça latência, throughput, alocações, CPU e banda.
- Compare cenários: unary vs streaming, compressão on/off, TLS on/off.
- Rode em CI para detectar regressões.

## 3. Alertas recomendados

- Taxa de erro (`INTERNAL`, `UNKNOWN`, `UNAVAILABLE`).
- p99 de latência por método.
- `DEADLINE_EXCEEDED` e `RESOURCE_EXHAUSTED`.
- Uso de memória/CPU; tamanho de filas.
- Reconexões/`GOAWAY` frequentes.
- Saturação de streams/conexões.

## 4. Checklist

- [ ] Interceptors de logging/tracing/métricas.
- [ ] `traceparent` propagado.
- [ ] Métricas RED por método (sem alta cardinalidade).
- [ ] Sem payload em logs.
- [ ] Canais reutilizados; deadlines definidos.
- [ ] Compressão avaliada com medição.
- [ ] TLS otimizado (1.3, resumption).
- [ ] LB configurado.
- [ ] Benchmarks/alertas em CI.
- [ ] `GRPC_TRACE` apenas em debug.
