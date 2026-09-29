# 03 — Serviços, contrato e geração de código

## 1. Definindo o serviço

```proto
edition = "2023";        // ou syntax = "proto3";
package acme.orders.v1;

service OrderService {
  rpc GetOrder    (GetOrderRequest)    returns (Order);
  rpc ListOrders  (ListOrdersRequest)  returns (ListOrdersResponse);
  rpc WatchOrders (WatchOrdersRequest) returns (stream Order);
  rpc Upload      (stream Chunk)       returns (UploadSummary);
  rpc Chat        (stream Message)     returns (stream Message);
}

message GetOrderRequest    { string id = 1; }
message ListOrdersRequest  { int32 page_size = 1; string page_token = 2; }
message ListOrdersResponse { repeated Order orders = 1; string next_page_token = 2; }
message WatchOrdersRequest { string filter = 1; }
message UploadSummary      { int32 accepted = 1; int32 rejected = 2; }
message Chunk              { bytes data = 1; }
message Message            { string text = 1; }
message Order              { string id = 1; int64 amount_minor = 2; }
```

- Nome totalmente qualificado: `/acme.orders.v1.OrderService/GetOrder`.
- Cada `rpc` exige request e response dedicados.
- `stream` em qualquer lado define o modo de streaming.
- `option idempotency_level = NO_SIDE_EFFECTS;` documenta que o método é seguro
  para repetir/retry.
- `option deprecated = true;` deprecia método/serviço.

## 2. Os quatro padrões

| Padrão | Assinatura | Semântica |
|---|---|---|
| Unary | `(Req) returns (Resp)` | 1 request, 1 response |
| Server streaming | `(Req) returns (stream Resp)` | 1 request, N responses |
| Client streaming | `(stream Req) returns (Resp)` | N requests, 1 response |
| Bidi | `(stream Req) returns (stream Resp)` | N × N, independentes |

Bidi: as duas direções são independentes (podem alternar); o servidor pode
responder antes de consumir tudo.

## 3. Geração de código

```bash
# Go
protoc -I. --go_out=gen --go_opt=paths=source_relative \
           --go-grpc_out=gen --go-grpc_opt=paths=source_relative \
           order.proto

# Python
protoc -I. --python_out=gen --pyi_out=gen --grpc_python_out=gen order.proto

# Java
protoc -I. --java_out=gen --grpc-java_out=gen order.proto

# C++
protoc -I. --cpp_out=gen --grpc_out=gen order.proto

# C#
protoc -I. --csharp_out=gen --grpc_out=gen order.proto

# Dart
protoc -I. --dart_out=grpc:lib/src order.proto
```

Com **buf**:

```yaml
# buf.gen.yaml
version: v2
plugins:
  - local: protoc-gen-go
    out: gen
    opt: paths=source_relative
  - local: protoc-gen-go-grpc
    out: gen
    opt: paths=source_relative
```

Artefatos gerados:

| Artefato | Conteúdo |
|---|---|
| Mensagens | classes/structs request/response |
| `*Client`/`*Stub` | cliente com métodos por RPC |
| `*Server`/base | interface/abstract para implementar |
| `Register...Server` | registro no servidor |
| `_grpc` metadata | descritores e helpers |

## 4. Boas práticas de contrato de serviço

1. **Versão no pacote** (`acme.orders.v1`), nunca no método.
2. **Request/response dedicados**; não reutilize a entidade como request.
3. **Paginação**: `page_size`/`page_token` (nunca `offset`).
4. **Updates parciais**: `google.protobuf.FieldMask`.
5. **Idempotência explícita**: `idempotency_level` + chaves de idempotência.
6. **Erros ricos**: `google.rpc.Status` + `details` (ver `04`).
7. **Nomes de recurso hierárquicos** (`projects/*/orders/*`).
8. **Add-only**: adicionar método é compatível; remover exige deprecação.
9. **Nunca reutilize número de campo** nas mensagens.
10. **Reflection/health** padronizados quando fizer sentido.

Padrões comuns (estilo AIP): `Get`, `List`, `Create`, `Update`, `Delete`,
`BatchGet`, `Search`, `Watch`; campos `name`, `parent`, `next_page_token`,
`update_mask`, `request_id`.

## 5. Anotações HTTP (grpc-gateway)

```proto
import "google/api/annotations.proto";

service OrderService {
  rpc GetOrder (GetOrderRequest) returns (Order) {
    option (google.api.http) = { get: "/v1/orders/{id}" };
  }
  rpc CreateOrder (CreateOrderRequest) returns (Order) {
    option (google.api.http) = {
      post: "/v1/orders"
      body: "*"
    };
  }
}
```

- Gera um gateway REST/JSON a partir do mesmo serviço.
- Útil para expor ao navegador/clientes HTTP sem gRPC nativo.
- `body: "*"` mapeia o corpo para a mensagem; `{id}` puxa do path.

## 6. Streaming: quando usar

| Situação | Padrão |
|---|---|
| Download de lista grande | Server streaming |
| Upload de arquivo/lote | Client streaming |
| Feed de eventos | Server streaming |
| Chat/colaboração | Bidi |
| CRUD simples | Unary |
| Lote de operações atômicas | Unary com `repeated` |

Evite streaming para lotes pequenos (overhead); prefira `repeated`.

## 7. Idempotência e retry

- Marque métodos seguros/idempotentes:

  ```proto
  rpc GetOrder (GetOrderRequest) returns (Order) {
    option idempotency_level = NO_SIDE_EFFECTS;
  }
  ```

- Para escritas, inclua `request_id`/`Idempotency-Key` e deduplique no servidor.
- Retry só em códigos idempotentes (ver `04`).

## 8. Versionamento de serviço

| Mudança | Compatível? |
|---|---|
| Adicionar método | ✅ |
| Adicionar campo a request/response | ✅ |
| Remover método | ⚠️ deprecar antes |
| Trocar tipo de campo | ❌ (regra protobuf) |
| Mudar nome do método/serviço | ❌ (quebra `:path`) |
| Novo pacote `v2` | ✅ (coexistência) |

Estratégia: `acme.orders.v1` → adiciona; quebra → `acme.orders.v2`.

## 9. Contrato rico: validação

- `protovalidate` expressa regras no `.proto` e valida no servidor:

  ```proto
  import "buf/validate/validate.proto";
  message GetOrderRequest {
    string id = 1 [(buf.validate.field).string.uuid = true];
  }
  ```

- Alternativa clássica: `protoc-gen-validate` (`validate.rules`).
- Valide **sempre** no servidor; nunca confie no cliente.

## 10. Descriptor set e reflection

```bash
protoc -I. --descriptor_set_out=bundle.desc --include_imports order.proto
```

- Necessário para `grpcurl`, gateways e reflection.
- Registre o descriptor set no servidor para habilitar reflection
  (`09-reflection-e-health.md`).

## 11. Checklist de serviço

- [ ] Pacote versionado; request/response dedicados.
- [ ] Paginação/`field_mask` onde aplicável.
- [ ] `idempotency_level` em métodos seguros.
- [ ] Erros ricos mapeados; status codes corretos.
- [ ] Streaming usado quando faz sentido.
- [ ] Compatibilidade add-only; deprecação antes de remoção.
- [ ] Descriptor set gerado para reflection/tooling.
- [ ] `buf lint`/`buf breaking` no CI.

Ver também `../protobuf_guide/02-contrato-do-proto.md` e
`../protobuf_guide/15-evolucao-de-contrato.md`.
