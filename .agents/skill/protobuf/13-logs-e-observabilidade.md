# 13 — Logs e observabilidade

Protobuf binário não é legível; o logging precisa de **conversão consciente**,
redação de dados sensíveis e contexto. Este arquivo cobre o que logar, como
redigir e como observar sistemas baseados em protobuf/gRPC.

## 1. Regras fundamentais

1. **Nunca logue payload bruto sem redação** — PII, segredos e tokens podem
   estar em `string`, `bytes`, `Struct`, `Any`.
2. **Prefira mensagens estruturadas** (JSON de log) a texto livre.
3. **Correlacione** chamadas com IDs (trace/span, request ID, tenant).
4. **Logue o suficiente para diagnosticar**: método, status, duração, tamanho,
   peer, código de erro.
5. **Não dependa do binário** para debug: converta para texto/JSON com redação.

## 2. Redação de campos sensíveis

Protobuf suporta marcação de campos para redação:

```proto
message User {
  string id = 1;
  string email = 2 [debug_redact = true];
  string password_hash = 3 [debug_redact = true];
  bytes token = 4 [debug_redact = true];
}
```

Runtimes oferecem impressão com redação:

```cpp
google::protobuf::TextFormat::Printer printer;
printer.SetRedact(true);              // ou via opções equivalentes
std::string out;
printer.PrintToString(user, &out);
```

```python
from google.protobuf import text_format
print(text_format.MessageToString(user, redact=True))
```

```java
TextFormat.printer().redactSensitiveFields(true).printToString(user);
```

Boas práticas:
- Marque **todos** os campos sensíveis (PII, credenciais, tokens, chaves).
- Redija **antes** de logar, não depois.
- Em `Any`/`Struct`, não confie na redação automática: evite logar.
- Considere truncar strings longas/base64.

## 3. O que logar em RPC (gRPC)

Um log de chamada útil:

| Campo | Exemplo |
|---|---|
| `trace_id`/`span_id` | correlação distribuída |
| `method` | `/acme.orders.v1.OrderService/GetOrder` |
| `peer` | `10.0.0.5:443` |
| `status_code` | `OK`, `NOT_FOUND` |
| `duration_ms` | `12.4` |
| `request_bytes` / `response_bytes` | tamanho |
| `user`/`tenant` | identidade (sem PII bruta) |
| `error` | mensagem sem payload sensível |

Exemplo (Go, interceptor):

```go
func loggingInterceptor(ctx context.Context, req any, info *grpc.UnaryServerInfo,
    handler grpc.UnaryHandler) (any, error) {
    start := time.Now()
    resp, err := handler(ctx, req)
    code := status.Code(err)
    slog.Info("grpc",
        "method", info.FullMethod,
        "code", code.String(),
        "duration_ms", time.Since(start).Milliseconds(),
        "trace_id", traceIDFrom(ctx),
    )
    return resp, err
}
```

Nunca inclua `req`/`resp` crus no log.

## 4. Níveis de log

| Nível | Uso |
|---|---|
| `DEBUG` | payload redigido, diffs de contrato, detalhes de parse |
| `INFO` | chamadas RPC, mudanças de estado, início/fim |
| `WARN` | retries, deadlines próximos, campos desconhecidos |
| `ERROR` | falhas de RPC, parse, validação; inclua status/código |
| `FATAL` | corrupção irrecuperável |

Evite logar em caminho quente com payload grande (custo de formatação/serialização).

## 5. Observabilidade (métricas e tracing)

### 5.1 Métricas

- **RED**: Rate, Errors, Duration por método gRPC.
- **USE**: Utilization, Saturation, Errors de recursos.
- Contagem por status code, tamanho de mensagem, retries, timeouts.
- Labels de baixa cardinalidade (método, serviço, status) — nunca IDs de usuário.

### 5.2 Tracing

- OpenTelemetry: propague `traceparent` via metadata gRPC.
- Spans por RPC e por operações internas (parse, DB, downstream).
- Registre atributos: método, peer, tamanho, status — sem payload.

### 5.3 Correlação

```text
traceparent: 00-<trace-id>-<span-id>-01
x-request-id: <uuid>
x-tenant-id: <id>
```

## 6. Logging de erros

- Mapeie exceções/exceções de parse para status gRPC adequados
  (`INVALID_ARGUMENT`, `INTERNAL`, `DATA_LOSS`).
- Logue a **causa** com contexto (campo, offset, tipo esperado), não o payload.
- Erros de validação: registre campo/caminho e regra violada.
- Não vaze detalhes internos para clientes (mensagens de erro genéricas;
  detalhes no log/`google.rpc.Status.details` controlado).

## 7. Debug de wire sem vazar

```bash
# Inspeciona estrutura sem schema (números de campo, não valores legíveis)
protoc --decode_raw < payload.bin

# Com schema, aplicando redação via runtime/tooling (não via protoc)
```

Para produção, exponha endpoints de debug **protegidos** e nunca retorne
payloads brutos.

## 8. JSON de log

Se o pipeline de logs usa JSON, mapeie mensagens protobuf com ProtoJSON e
redação — nunca TextFormat cru com PII. Cuidado com:
- int64 → string (perda de precisão em JS);
- bytes → base64;
- `NaN`/`Infinity`;
- profundidade/tamanho (trunque).

## 9. Conformidade (LGPD/GDPR/PCI)

- Classifique os campos por sensibilidade no `.proto` (custom option ou
  `debug_redact`).
- Garanta que logs não contenham dados pessoais não redigidos.
- Defina retenção e acesso a logs.
- Audite quem pode ler logs de payload.
- Prefira logar **identificadores**, não conteúdo sensível.

## 10. Checklist de observabilidade

- [ ] Campos sensíveis com `debug_redact = true`.
- [ ] Redação aplicada antes de logar.
- [ ] Logs estruturados com trace/request ID.
- [ ] Métricas RED/USE por método gRPC.
- [ ] Tracing OTel com propagação de contexto.
- [ ] Nunca logar payload bruto.
- [ ] Erros mapeados para status gRPC; detalhes internos não vazam.
- [ ] Endpoints de debug restritos.
- [ ] Retenção e acesso a logs conforme compliance.
