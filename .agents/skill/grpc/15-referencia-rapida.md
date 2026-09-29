# 15 — Referência rápida (cheatsheet gRPC)

## 1. Definição de serviço

```proto
syntax = "proto3";
package acme.orders.v1;

service OrderService {
  rpc GetOrder    (GetOrderRequest)    returns (Order);
  rpc WatchOrders (WatchOrdersRequest) returns (stream Order);
  rpc Upload      (stream Chunk)       returns (UploadSummary);
  rpc Chat        (stream Message)     returns (stream Message);
}
```

| Padrão | Assinatura |
|---|---|
| Unary | `(Req) returns (Resp)` |
| Server streaming | `(Req) returns (stream Resp)` |
| Client streaming | `(stream Req) returns (Resp)` |
| Bidi | `(stream Req) returns (stream Resp)` |

## 2. Caminho do método

```
:path = /pacote.Serviço/Método
ex.:  /acme.orders.v1.OrderService/GetOrder
```

## 3. Status codes

| Nº | Código | Nº | Código |
|---|---|---|---|
| 0 | OK | 8 | RESOURCE_EXHAUSTED |
| 1 | CANCELLED | 9 | FAILED_PRECONDITION |
| 2 | UNKNOWN | 10 | ABORTED |
| 3 | INVALID_ARGUMENT | 11 | OUT_OF_RANGE |
| 4 | DEADLINE_EXCEEDED | 12 | UNIMPLEMENTED |
| 5 | NOT_FOUND | 13 | INTERNAL |
| 6 | ALREADY_EXISTS | 14 | UNAVAILABLE |
| 7 | PERMISSION_DENIED | 15 | DATA_LOSS |
| | | 16 | UNAUTHENTICATED |

Mapeamento HTTP (sem `grpc-status`): 400→INTERNAL, 401→UNAUTHENTICATED,
403→PERMISSION_DENIED, 404→UNIMPLEMENTED, 429/502/503/504→UNAVAILABLE,
outros→UNKNOWN.

## 4. Headers e trailers

| Item | Valor |
|---|---|
| `:method` | `POST` |
| `:path` | `/pkg.Svc/Method` |
| `te` | `trailers` |
| `content-type` | `application/grpc+proto` |
| `grpc-timeout` | `2S`, `500m`, etc. |
| `grpc-encoding` | `gzip`/`identity`/... |
| `grpc-accept-encoding` | lista |
| trailer `grpc-status` | código |
| trailer `grpc-message` | mensagem (percent-encoded) |

Frame de mensagem: `[flag 1B][length 4B BE][payload]`.

Metadata binária: chave com sufixo `-bin` (base64).

## 5. Comandos

```bash
# gerar stubs
protoc -I. --go_out=gen --go-grpc_out=gen order.proto
protoc -I. --python_out=gen --grpc_python_out=gen order.proto
protoc -I. --java_out=gen --grpc-java_out=gen order.proto
protoc -I. --dart_out=grpc:lib/src order.proto

# descriptor set
protoc -I. --descriptor_set_out=bundle.desc --include_imports order.proto

# debug via reflection
grpcurl -plaintext localhost:50051 list
grpcurl -plaintext localhost:50051 describe acme.orders.v1.OrderService
grpcurl -plaintext -d '{"id":"o-1"}' localhost:50051 \
  acme.orders.v1.OrderService/GetOrder
grpc_cli ls localhost:50051

# carga
ghz --insecure --proto order.proto \
  --call acme.orders.v1.OrderService.GetOrder \
  -d '{"id":"o-1"}' -c 50 -n 100000 localhost:50051
```

## 6. Erro rico (Go)

```go
st := status.New(codes.InvalidArgument, "inválido")
st, _ = st.WithDetails(&errdetails.BadRequest{
    FieldViolations: []*errdetails.BadRequest_FieldViolation{
        {Field: "id", Description: "obrigatório"},
    },
})
return nil, st.Err()
```

## 7. Metadata/deadline (Go)

```go
ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
defer cancel()
ctx = metadata.AppendToOutgoingContext(ctx, "authorization", "Bearer "+tok)
resp, err := client.GetOrder(ctx, req)
```

## 8. Servidor (Go)

```go
srv := grpc.NewServer(
    grpc.Creds(tlsCreds),
    grpc.MaxRecvMsgSize(4<<20),
    grpc.ChainUnaryInterceptor(auth, logging),
)
pb.RegisterOrderServiceServer(srv, &server{})
healthpb.RegisterHealthServer(srv, health.NewServer())
reflection.Register(srv) // ambientes controlados
srv.Serve(lis)
```

## 9. Service config (resumo)

```json
{
  "loadBalancingConfig": [{ "round_robin": {} }],
  "methodConfig": [{
    "name": [{ "service": "acme.orders.v1.OrderService" }],
    "timeout": "2s",
    "retryPolicy": {
      "maxAttempts": 5,
      "initialBackoff": "0.1s",
      "maxBackoff": "2s",
      "backoffMultiplier": 1.5,
      "retryableStatusCodes": ["UNAVAILABLE"]
    }
  }]
}
```

## 10. Políticas de LB

`pick_first` (default), `round_robin`, `least_request`, `ring_hash`, `grpclb`,
xDS (`weighted_target`, `priority`, ...).

## 11. Variáveis de ambiente (C-core)

`grpc_proxy`/`https_proxy`, `no_grpc_proxy`, `GRPC_SSL_CIPHER_SUITES`,
`GRPC_DEFAULT_SSL_ROOTS_FILE_PATH`, `GRPC_POLL_STRATEGY`, `GRPC_TRACE`,
`GRPC_ENABLE_FORK_SUPPORT`.

## 12. Conexão — estados

`IDLE → CONNECTING → READY`; `TRANSIENT_FAILURE`; `SHUTDOWN`.

## 13. Regras de ouro

1. Deadline sempre.
2. Retry só idempotente, com backoff+jitter.
3. Credenciais em metadata; TLS/mTLS.
4. Limites e keepalive alinhados.
5. Status codes corretos; erros ricos.
6. Sem payload em logs.
7. Reflection/health restritos.
8. Canais reutilizados; LB configurado.
9. Shutdown gracioso (health + drenagem).
10. Stub e runtime na mesma versão.

## 14. Arquivos deste guia

`00` visão geral · `01` fundamentos · `02` protocolo HTTP/2 e web ·
`03` serviços/contrato · `04` erros/status · `05` metadata/deadlines ·
`06` streaming · `07` interceptors · `08` segurança · `09` reflection/health ·
`10` compressão/keepalive/backoff · `11` LB/naming/xDS ·
`12` observabilidade/performance · `13` testes/interop ·
`14` operação/troubleshooting · `implementacao/` por stack.

Contrato `.proto` e serialização: skill irmã
[`../protobuf_guide/`](../protobuf_guide/).
