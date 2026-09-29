# 09 — Server reflection e health checking

## 1. Server reflection

Protocolo opcional (`grpc.reflection.v1`) que permite a clientes **descobrir**
serviços, métodos e mensagens em runtime, sem `.proto` pré-compilado. Base de
ferramentas como `grpcurl` e `grpc_cli`.

### 1.1 Como funciona

- O cliente pergunta: "quais serviços?", "qual o descritor do serviço X?",
  "qual o descritor do tipo Y?".
- O servidor responde com `FileDescriptorProto`/`ServiceDescriptorProto`.
- Habilita construir requests dinamicamente e converter texto↔binário.

### 1.2 Habilitar

```go
import "google.golang.org/grpc/reflection"

srv := grpc.NewServer()
pb.RegisterOrderServiceServer(srv, &server{})
reflection.Register(srv)   // habilita reflection v1
```

```python
from grpc_reflection.v1alpha import reflection
reflection.enable_server_reflection(SERVICE_NAMES, server)
```

Rust (tonic):

```rust
let reflection = tonic_reflection::server::Builder::configure()
    .register_encoded_file_descriptor_set(FILE_DESCRIPTOR_SET)
    .build_v1()?;
Server::builder().add_service(reflection)
```

> É preciso registrar o `FileDescriptorSet` (gerado com
> `protoc --descriptor_set_out --include_imports` ou pelo `build.rs`).

### 1.3 Uso com `grpcurl`

```bash
grpcurl -plaintext localhost:50051 list
grpcurl -plaintext localhost:50051 describe acme.orders.v1.OrderService
grpcurl -plaintext -d '{"id":"o-1"}' \
  localhost:50051 acme.orders.v1.OrderService/GetOrder
```

Com TLS:

```bash
grpcurl -cacert ca.pem -cert client.pem -key client.key \
  service.internal:443 list
```

Sem reflection (usar descriptor):

```bash
grpcurl -protoset bundle.desc -plaintext localhost:50051 list
```

### 1.4 Segurança

- Reflection **expõe a superfície** da API (nomes, mensagens, campos).
- Em **produção pública**, desabilite ou restrinja por rede/auth.
- Em ambientes internos de debug, habilite atrás de autenticação.
- Não exponha reflection a clientes não confiáveis.

## 2. Health checking

Serviço padrão `grpc.health.v1.Health` usado por orquestradores, load balancers
e meshes para saber se o servidor pode atender.

### 2.1 Definição

```proto
syntax = "proto3";
package grpc.health.v1;

message HealthCheckRequest  { string service = 1; }
message HealthCheckResponse {
  enum ServingStatus {
    UNKNOWN = 0;
    SERVING = 1;
    NOT_SERVING = 2;
    SERVICE_UNKNOWN = 3;  // usado pelo Watch
  }
  ServingStatus status = 1;
}

service Health {
  rpc Check (HealthCheckRequest) returns (HealthCheckResponse);
  rpc Watch (HealthCheckRequest) returns (stream HealthCheckResponse);
}
```

- `Check`: consulta pontual; `service` vazio = status geral do servidor.
- `Watch`: stream de mudanças de status.
- Status **por serviço** (ex.: `acme.orders.v1.OrderService`).

### 2.2 Habilitar (Go)

```go
import (
    health "google.golang.org/grpc/health"
    healthpb "google.golang.org/grpc/health/grpc_health_v1"
)

hs := health.NewServer()
hs.SetServingStatus("acme.orders.v1.OrderService", healthpb.HealthCheckResponse_SERVING)
healthpb.RegisterHealthServer(srv, hs)

// ao desligar:
hs.Shutdown()
hs.SetServingStatus("", healthpb.HealthCheckResponse_NOT_SERVING)
```

### 2.3 Semântica

- `SERVING`: pronto para tráfego.
- `NOT_SERVING`: não pronto (inicializando/desligando/sobrecarregado).
- `SERVICE_UNKNOWN`: serviço não registrado (só no `Watch`).
- `UNKNOWN`: estado indeterminado.
- Retorne `NOT_SERVING` durante shutdown para drenar tráfego.

### 2.4 Boas práticas

1. Registre o health service em **todo** servidor.
2. Atualize o status por serviço (não só geral).
3. Use `Watch` em load balancers para reagir rápido.
4. Combine com `SIGTERM`: marque `NOT_SERVING`, drene, encerre.
5. Não exponha detalhes internos no status.

## 3. Reflection vs health

| | Reflection | Health |
|---|---|---|
| Protocolo | `grpc.reflection.v1` | `grpc.health.v1` |
| Propósito | descoberta de schema | prontidão |
| Produção | restrito | habilitado |
| Ferramenta | `grpcurl` | LB/orquestrador |

## 4. Checklist

- [ ] Reflection habilitada em dev; restrita/desabilitada em produção pública.
- [ ] `FileDescriptorSet` registrado para reflection.
- [ ] Health service registrado com status por serviço.
- [ ] `NOT_SERVING` no shutdown (drenagem).
- [ ] `Watch` usado por LB quando suportado.
- [ ] Nenhum detalhe sensível exposto.
