---
name: grpc
description: >
  gRPC (Core 57 / C++ 1.85-dev, main 2026): RPC framework over HTTP/2 (and
  HTTP/3) using protobuf as IDL. Covers the RPC model and four streaming
  patterns, the HTTP/2 wire protocol and gRPC-Web, service/contract design and
  code generation, status codes and rich errors, metadata/deadlines/
  cancellation/wait-for-ready, flow control, interceptors, TLS/mTLS and
  authentication, server reflection and health checking, compression,
  keepalive and connection backoff, load balancing/naming/service config/xDS,
  observability and performance, testing and interoperability, versioning and
  troubleshooting, plus per-stack implementation guides (Rust, Go,
  Dart/Flutter, TypeScript, web, PHP 7.2/Laravel 5.5, PHP 8.4/Laravel 12).
  Load when designing, implementing, securing, operating, tuning or debugging
  gRPC services. For the `.proto` contract and serialization itself, load the
  companion `protobuf` skill.
category: protocols
version: "Core 57 / C++ 1.85-dev (main, 2026)"
tags: [grpc, rpc, http2, http3, protobuf, streaming, status-codes, metadata, deadlines, interceptors, tls, mtls, reflection, health-check, compression, keepalive, load-balancing, xds, service-config, observability, opentelemetry, connect, grpc-web, interop]
license: MIT
---

# gRPC

High-performance RPC framework: client and server generated from a `.proto`,
carried over **HTTP/2** (or HTTP/3), messages in **binary protobuf**. One call =
one HTTP/2 stream; status and metadata ride in headers and trailers.

> Scope: **gRPC** (transport, RPC, operations). The `.proto` contract and
> serialization live in the sibling skill
> [`../protobuf_guide/`](../protobuf_guide/). The same `.proto` serves both.

## Use When
- Define/consume RPC services (unary, server/client/bidi streaming).
- Generate client/server stubs (`protoc-gen-*-grpc`, buf, Bazel).
- Map errors and status codes, rich errors (`google.rpc.Status`).
- Propagate metadata, deadlines, cancellation, `wait-for-ready`.
- Configure TLS/mTLS, authentication and authorization.
- Enable reflection, health checking, compression, keepalive.
- Do load balancing, service config, xDS, proxyless.
- Observe (binary logging, traces, metrics) and tune performance.
- Test/interoperate across languages; operate and debug (GOAWAY, limits).

## Avoid When
- You only need serialization, not typed RPC → `protobuf` skill, or
  `../protobuf_guide/10-protobuf-sem-grpc.md`.
- Environment lacks HTTP/2 or gRPC support (e.g. PHP 7.2 without `ext-grpc`)
  → use protobuf without gRPC (HTTP/REST/queues).
- Public REST/JSON-only API → prefer grpc-gateway/Connect or plain REST.
- Browser without proxy → use Connect-Web/gRPC-Web (covered here).

## Non-negotiable rules
1. **Always set a deadline/timeout.** Calls without one can hang.
2. **Retry only idempotent codes** (e.g. `UNAVAILABLE`) with backoff+jitter;
   `ABORTED` → retry at a higher level. Never blind-retry.
3. **Credentials in metadata**, never in the message payload.
4. **TLS/mTLS required** in production; use mTLS between internal services.
5. **Never log raw payloads**; redact sensitive fields.
6. **Respect message limits** (default 4 MiB); align client and server.
7. **Align keepalive** on client and server (avoids `GOAWAY too_many_pings`).
8. **Deadlines propagate** downstream; release resources on completion.
9. **Reflection/health** only in controlled environments (they expose surface).
10. **Error responses must not leak internals** to clients.
11. **Keep stub/plugin and runtime at the same version.**
12. **Correct status codes**: `INVALID_ARGUMENT` ≠ `FAILED_PRECONDITION`;
    `PERMISSION_DENIED` ≠ `UNAUTHENTICATED`; `RESOURCE_EXHAUSTED` for resources.

## API quick map
| Need | Construct |
|---|---|
| Unary method | `rpc M (Req) returns (Resp);` |
| Server streaming | `rpc M (Req) returns (stream Resp);` |
| Client streaming | `rpc M (stream Req) returns (Resp);` |
| Bidi | `rpc M (stream Req) returns (stream Resp);` |
| Idempotency | `option idempotency_level = NO_SIDE_EFFECTS;` |
| Rich error | `google.rpc.Status` + `details` (`BadRequest`, `ErrorInfo`) |
| Metadata | HTTP/2 headers; lowercase keys; `-bin` (base64) |
| Deadline | `grpc-timeout` header / `CallOptions.timeout` |
| Cancellation | `context`/`CancellationToken`/`AbortSignal` |
| Wait for connection | `wait_for_ready` |
| Fail fast | `fail_fast` |
| Reflection | `grpc.reflection.v1` |
| Health | `grpc.health.v1.Health` (`Check`/`Watch`) |
| Compression | `identity`/`gzip`/`deflate`/`snappy`/custom |
| Load balancing | `pick_first`/`round_robin`/`least_request`/`ring_hash`/xDS |
| REST transcoding | `google.api.http` + grpc-gateway |

## Commands
```bash
protoc -I. --go_out=gen --go-grpc_out=gen order.proto
protoc -I. --python_out=gen --grpc_python_out=gen order.proto
protoc -I. --java_out=gen --grpc-java_out=gen order.proto
buf generate

grpcurl -plaintext localhost:50051 list
grpcurl -plaintext -d '{"id":"o-1"}' localhost:50051 acme.orders.v1.OrderService/GetOrder
grpc_cli ls localhost:50051
ghz --insecure --proto order.proto --call acme.orders.v1.OrderService.GetOrder \
  -d '{"id":"o-1"}' -c 50 -n 100000 localhost:50051
```

## Minimal pattern
```proto
syntax = "proto3";
package acme.orders.v1;

service OrderService {
  rpc GetOrder (GetOrderRequest) returns (Order);
  rpc WatchOrders (WatchOrdersRequest) returns (stream Order);
}
message GetOrderRequest { string id = 1; }
message WatchOrdersRequest { string filter = 1; }
message Order { string id = 1; int64 amount_minor = 2; }
```
```go
srv := grpc.NewServer(grpc.Creds(tlsCreds), grpc.MaxRecvMsgSize(4<<20))
pb.RegisterOrderServiceServer(srv, &server{})
srv.Serve(lis)
```

## File Map
| File | Content |
|---|---|
| `00-indice.md` | Overview, mental model, map, read order |
| `01-fundamentos.md` | What it is, RPC model, when to use, ecosystem, versions |
| `02-protocolo-http2-e-web.md` | Wire protocol, framing, headers/trailers, gRPC-Web, HTTP↔status mapping |
| `03-servicos-e-contrato.md` | `service`/`rpc`, 4 patterns, codegen, pagination, `field_mask`, idempotency, versioning |
| `04-erros-e-status.md` | Status table, rich errors, mapping, ordering, retry |
| `05-metadata-deadlines-cancelamento.md` | Metadata, deadlines, cancellation, `wait_for_ready`, `fail_fast` |
| `06-streaming-e-flow-control.md` | Streaming patterns, flow control, backpressure, half-close |
| `07-interceptors-e-middleware.md` | Unary/stream interceptors, order, auth/log/tracing/retry |
| `08-seguranca.md` | TLS/mTLS, credentials, authN/authZ, isolation, audit |
| `09-reflection-e-health.md` | Server reflection, health checking, `grpcurl` |
| `10-compressao-keepalive-backoff.md` | Compression, keepalive, connection backoff |
| `11-balanceamento-naming-service-config-xds.md` | Connectivity, LB, naming, service config, xDS/Envoy |
| `12-observabilidade-performance.md` | Binary logging, traces, metrics, tuning, TLS perf |
| `13-testes-e-interop.md` | Unit/in-process, interop, HTTP/2 interop, fault injection |
| `14-operacao-versionamento-troubleshooting.md` | Versioning, env vars, operations, GOAWAY, limits |
| `15-referencia-rapida.md` | Cheatsheet: status, headers, flags, snippets |
| `implementacao/00-indice.md` | Per-stack recipe index |
| `implementacao/01-rust.md` | Rust: `tonic` (+ `prost`) |
| `implementacao/02-go.md` | Go: `grpc-go` |
| `implementacao/03-dart-flutter.md` | Dart/Flutter: `grpc` |
| `implementacao/04-typescript.md` | TypeScript: Connect / `@grpc/grpc-js` |
| `implementacao/05-web-moderna.md` | Browser: Connect-Web/gRPC-Web |
| `implementacao/06-php72-laravel55.md` | PHP 7.2 + Laravel 5.5 (client) |
| `implementacao/07-php84-laravel12.md` | PHP 8.4 + Laravel 12 (RoadRunner/Octane) |

## Read Order (token-efficient)
Always `00` → `01` → `02`. Define service: `03`. Errors: `04`.
Calls: `05`+`06`. Interceptors: `07`. Production: `08`+`09`+`10`+`11`.
Observe/tune: `12`. Quality: `13`+`14`. Lookup: `15`.

## Prereqs
Basic HTTP/2 and a gRPC runtime (Go, Java/Kotlin, C++, Python, C#, Rust, Dart,
PHP, Node). Read the `protobuf` skill for the `.proto` contract.
