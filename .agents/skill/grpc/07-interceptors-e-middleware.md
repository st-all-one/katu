# 07 — Interceptors e middleware

Interceptors (middleware) aplicam lógica transversal a chamadas gRPC:
autenticação, autorização, logging, métricas, tracing, timeout, retry,
validação e tratamento de erros.

## 1. Tipos

| Tipo | Aplica-se a |
|---|---|
| Unary server interceptor | métodos unary no servidor |
| Stream server interceptor | streams no servidor |
| Unary client interceptor | chamadas unary no cliente |
| Stream client interceptor | streams no cliente |

Cada runtime expõe nomes próprios: `grpc.UnaryServerInterceptor` (Go),
`ClientInterceptor`/`ServerInterceptor` (Java), `Interceptor` (Rust/tonic),
`ClientInterceptor` (Dart), `Interceptor` (Connect/TS).

## 2. Ordem

```
cliente:  auth → tracing → logging → retry → transporte
servidor: auth → tenant → rate-limit → tracing → logging → validação → handler
```

- Interceptors são encadeados; a ordem define quem envolve quem.
- Coloque **auth primeiro** no servidor e **tracing/logging** cedo para capturar
  falhas de auth.
- Evite interceptor que engole erros silenciosamente.

## 3. Exemplos

### 3.1 Go — logging + auth

```go
func logging(ctx context.Context, req any, info *grpc.UnaryServerInfo,
    handler grpc.UnaryHandler) (any, error) {
    start := time.Now()
    resp, err := handler(ctx, req)
    slog.Info("grpc",
        "method", info.FullMethod,
        "code", status.Code(err).String(),
        "duration_ms", time.Since(start).Milliseconds(),
    )
    return resp, err
}

func auth(ctx context.Context, req any, info *grpc.UnaryServerInfo,
    handler grpc.UnaryHandler) (any, error) {
    md, _ := metadata.FromIncomingContext(ctx)
    if len(md.Get("authorization")) == 0 {
        return nil, status.Error(codes.Unauthenticated, "sem credencial")
    }
    return handler(ctx, req)
}

srv := grpc.NewServer(grpc.ChainUnaryInterceptor(auth, logging))
```

### 3.2 Rust (tonic)

```rust
#[derive(Clone)]
struct AuthInterceptor { token: String }

impl tonic::service::Interceptor for AuthInterceptor {
    fn call(&mut self, mut req: tonic::Request<()>)
        -> Result<tonic::Request<()>, tonic::Status> {
        let v = format!("Bearer {}", self.token).parse()
            .map_err(|_| tonic::Status::internal("token inválido"))?;
        req.metadata_mut().insert("authorization", v);
        Ok(req)
    }
}
let client = CatalogServiceClient::with_interceptor(channel, AuthInterceptor { token });
```

### 3.3 TypeScript (Connect)

```ts
const auth = (next) => async (req) => {
  req.header.set("authorization", `Bearer ${await getToken()}`);
  return next(req);
};
const transport = createConnectTransport({ baseUrl, interceptors: [auth] });
```

### 3.4 Dart

```dart
class AuthInterceptor extends ClientInterceptor {
  @override
  ResponseFuture<R> interceptUnary<Q, R>(method, request, options, invoker) =>
      invoker(method, request, options.mergedWith(
          CallOptions(metadata: {'authorization': 'Bearer $token'})));
}
```

### 3.5 Java

```java
public class AuthInterceptor implements ServerInterceptor {
  public <ReqT, RespT> ServerCall.Listener<ReqT> interceptCall(
      ServerCall<ReqT, RespT> call, Metadata headers, ServerCallHandler<ReqT, RespT> next) {
    // validar headers
    return next.startCall(call, headers);
  }
}
```

## 4. Casos de uso

| Interceptor | Responsabilidade |
|---|---|
| AuthN | validar token/credencial |
| AuthZ | checar permissão por método/recurso |
| Tenant | extrair/validar tenant |
| Rate limit | quotas por identidade |
| Tracing | iniciar span, propagar `traceparent` |
| Logging | método, status, duração (sem payload) |
| Métricas | contadores/histogramas |
| Timeout | aplicar deadline default |
| Validação | `protovalidate`/regras |
| Recovery | converter panics em `INTERNAL` |
| Retry | no cliente, com backoff+jitter |
| Idempotência | deduplicar por `request_id` |

## 5. Boas práticas

1. **Não logue payload**; registre método/status/duração.
2. Converta exceções/panics em status adequado (evita `UNKNOWN`).
3. Propague contexto (deadline, cancelamento, trace).
4. Mantenha interceptors idempotentes e rápidos.
5. Evite efeitos colaterais inesperados (ex.: escrever em DB).
6. Teste interceptors isoladamente.
7. Cuidado com ordem e duplicação (ex.: dois timeouts conflitantes).
8. Em streams, trate mensagens e encerramento, não só o início.

## 6. Segurança

- Auth/authz **sempre** no servidor; nunca confie no cliente.
- Compare tokens em tempo constante.
- Não inclua segredos em logs/métricas.
- Rate limit por identidade/IP/tenant.
- Valide `tenant`/`user` de metadata.

## 7. Checklist

- [ ] Auth e authz no pipeline do servidor.
- [ ] Tracing/logging cedo e sem payload.
- [ ] Panics/exceções convertidos em status.
- [ ] Timeout/deadline aplicado.
- [ ] Retry só no cliente e em códigos idempotentes.
- [ ] Interceptors testados e ordenados corretamente.
- [ ] Rate limiting e quotas.
