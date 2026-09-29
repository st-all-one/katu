# 13 — Testes e interoperabilidade

> Fontes: `grpc_docs/doc/unit_testing.md`, `interop-test-descriptions.md`,
> `http2-interop-test-descriptions.md`, `connection-backoff-interop-test-description.md`.

## 1. Camadas de teste

| Camada | O que valida | Como |
|---|---|---|
| Unit (in-process) | handler, mapeamento de erro | servidor em memória |
| Contrato | `buf lint`/`buf breaking` | CI |
| Integração | cliente↔servidor reais | servidor efêmero/docker |
| Interop | compatibilidade entre linguagens | test suite oficial |
| HTTP/2 interop | conformidade de transporte | http2-interop |
| Carga/estresse | limites e latência | `ghz`, k6 |
| Caos/fault injection | resiliência | matar/atrasar backends |
| Segurança | TLS/auth/limites | testes dedicados |

## 2. Testes in-process

Suba o servidor em memória e conecte com o mesmo runtime, sem rede.

```go
// Go: bufconn
lis := bufconn.Listen(1 << 20)
srv := grpc.NewServer()
pb.RegisterOrderServiceServer(srv, &server{})
go srv.Serve(lis)
defer srv.Stop()

conn, _ := grpc.NewClient("passthrough:///bufnet",
    grpc.WithContextDialer(func(ctx context.Context, _ string) (net.Conn, error) {
        return lis.DialContext(ctx)
    }),
    grpc.WithTransportCredentials(insecure.NewCredentials()),
)
client := pb.NewOrderServiceClient(conn)
order, err := client.GetOrder(ctx, &pb.GetOrderRequest{Id: "o-1"})
```

```python
# Python: servidor em porta efêmera
server = grpc.server(futures.ThreadPoolExecutor())
add_OrderServiceServicer_to_server(Servicer(), server)
port = server.add_insecure_port("localhost:0")
server.start()
channel = grpc.insecure_channel(f"localhost:{port}")
```

```java
// Java: InProcessServer
InProcessServerBuilder.forName("test").directExecutor()
    .addService(new OrderServiceImpl()).build().start();
ManagedChannel ch = InProcessChannelBuilder.forName("test").build();
```

```rust
// Rust: tonic + TcpListener efêmero
let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
let addr = listener.local_addr()?;
tokio::spawn(Server::builder().add_service(svc).serve_with_incoming(...));
```

```dart
// Dart: Server na VM + ClientChannel em 127.0.0.1:0
final server = await Server.create(services: [Svc()]);
await server.serve(port: 0);
```

## 3. O que testar

- **Unary**: sucesso, campos vazios, validação, erro.
- **Streaming**: server/client/bidi; ordem, cancelamento, encerramento, erro
  no meio.
- **Status codes**: cada código relevante; `DEADLINE_EXCEEDED`,
  `RESOURCE_EXHAUSTED`, `UNAVAILABLE`.
- **Metadata**: auth, tenant, trace propagados.
- **Deadlines**: servidor lento → deadline estoura.
- **Cancelamento**: cliente cancela; servidor libera recursos.
- **Erros ricos**: `google.rpc.Status.details` parseáveis.
- **Limites**: mensagem grande, muitos streams.
- **Reflection/health**: descoberta e status.

## 4. Interoperabilidade

O repositório oficial mantém **interop tests** que rodam pares
cliente/servidor em linguagens diferentes sobre cenários padronizados
(`interop-test-descriptions.md`):

- unary, streaming, metadata, deadlines, compressão, cancelamento, auth, erro;
- grandes mensagens, mensagens vazias, múltiplas mensagens;
- HTTP/2 interop (`http2-interop-test-descriptions.md`);
- connection backoff interop.

Use o interop como referência para validar um runtime/implementação. Em
sistemas multi-linguagem, rode um teste cruzado (ex.: cliente Go ↔ servidor
Java).

## 5. Teste de contrato (CI)

```bash
buf lint
buf breaking --against '.git#branch=main'
```

Bloqueie merge em breaking changes. Combine com testes de round-trip protobuf
(ver `../protobuf_guide/14-testes.md`).

## 6. Testes com `grpcurl`/`grpc_cli`

```bash
grpcurl -plaintext -d '{"id":"o-1"}' localhost:50051 \
  acme.orders.v1.OrderService/GetOrder
grpcurl -plaintext localhost:50051 list
```

Úteis para smoke tests, depuração manual e validação de reflection.

## 7. Testes de carga

```bash
ghz --insecure --proto order.proto \
  --call acme.orders.v1.OrderService.GetOrder \
  -d '{"id":"o-1"}' -c 50 -n 100000 localhost:50051
```

- Meça p50/p95/p99, throughput e erros.
- Teste com/sem compressão, TLS on/off, unary vs streaming.
- Validar limites de mensagem e backpressure.

## 8. Fault injection / caos

- Derrubar/reiniciar backends → validar LB/retry/backoff.
- Atraso artificial → validar deadlines.
- Erros intermitentes → validar retry/idempotência.
- Partição de rede → validar `UNAVAILABLE`/reconexão.
- Corromper payload → validar `INTERNAL`/`DATA_LOSS`.

## 9. Testes de segurança

- TLS/mTLS: certificados válidos/inválidos/expired.
- AuthN/AuthZ: sem token, token inválido, sem permissão.
- Limites: mensagem acima do máximo, profundidade excessiva.
- Reflection restrita em produção.
- Metadata maliciosa (tenant forjado).
- Compressão + entrada hostil (CRIME/BEAST).

## 10. Checklist

- [ ] Testes in-process por handler.
- [ ] Sucesso, validação, erros e status codes.
- [ ] Unary + 3 modos de streaming.
- [ ] Deadlines, cancelamento, metadata.
- [ ] `buf lint`/`breaking` no CI.
- [ ] Teste de interop entre linguagens.
- [ ] Carga com `ghz` em CI/nightly.
- [ ] Fault injection para LB/retry.
- [ ] Testes de TLS/auth/limites.
