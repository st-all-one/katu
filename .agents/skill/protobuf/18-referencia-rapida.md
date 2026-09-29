# 18 — Referência rápida (cheatsheet)

## 1. Esqueleto de arquivo

```proto
edition = "2023";                 // ou syntax = "proto2"|"proto3"
package acme.orders.v1;

import "google/protobuf/timestamp.proto";
option java_multiple_files = true;
option java_package = "com.acme.orders.v1";
option go_package = "acme.com/orders/v1;ordersv1";

message Order {
  reserved 7, 9 to 11;
  reserved "legacy_status";

  string id = 1;
  optional string coupon = 2;                  // presença explícita
  repeated LineItem items = 3;
  map<string, string> labels = 4;
  Status status = 5;
  google.protobuf.Timestamp created_at = 6;

  oneof payment {
    string card_token = 8;
    string pix_key = 12;
  }
}

enum Status {
  STATUS_UNSPECIFIED = 0;
  STATUS_PENDING = 1;
  STATUS_PAID = 2;
}

service OrderService {
  rpc GetOrder (GetOrderRequest) returns (Order);
  rpc Watch (WatchRequest) returns (stream Order);
}

message GetOrderRequest { string id = 1; }
```

## 2. Tipos escalares

| Tipo | Custo negativo | Uso típico |
|---|---|---|
| `int32`/`int64` | 10 bytes | positivos |
| `sint32`/`sint64` | zigzag | negativos |
| `uint32`/`uint64` | varint | não-negativos |
| `fixed32`/`fixed64` | 4/8 bytes | IDs/hashes grandes |
| `sfixed32`/`sfixed64` | 4/8 bytes | inteiros fixos com sinal |
| `float`/`double` | 4/8 bytes | decimais (nunca dinheiro) |
| `bool` | 1 byte | flags |
| `string` | UI | UTF-8 |
| `bytes` | UI | binário |

## 3. Wire types

| Código | Nome | Tipos |
|---|---|---|
| 0 | VARINT | int/uint/sint/bool/enum |
| 1 | I64 | fixed64/sfixed64/double |
| 2 | LEN | string/bytes/msg/repeated packed/map |
| 3 | SGROUP | grupo início (legado) |
| 4 | EGROUP | grupo fim (legado) |
| 5 | I32 | fixed32/sfixed32/float |

`tag = (field_number << 3) | wire_type`.

## 4. Faixas de números de campo

| Faixa | Nota |
|---|---|
| 1–15 | Tag 1 byte (use para os quentes) |
| 16–2047 | Tag 2 bytes |
| 2048–536870911 | Tag 3–5 bytes |
| 19000–19999 | Reservado pelo protobuf |
| Máximo | 536.870.911 (2²⁹−1) |

## 5. Presença

| Declaração | Presença |
|---|---|
| `int32 x = 1;` (editions/proto3+) | explícita (editions) / implícita (proto3) |
| `optional int32 x = 1;` (proto3) | explícita |
| `int32 x = 1 [features.field_presence = IMPLICIT];` | sem |
| `int32 x = 1 [features.field_presence = EXPLICIT];` | com |
| campo em `oneof` | explícita |
| `repeated`/`map` | nenhuma |

## 6. Features core

| Feature | Valores | Default 2023/2024/2026 |
|---|---|---|
| `field_presence` | EXPLICIT/IMPLICIT/LEGACY_REQUIRED | EXPLICIT |
| `enum_type` | OPEN/CLOSED | OPEN |
| `repeated_field_encoding` | PACKED/EXPANDED | PACKED |
| `utf8_validation` | VERIFY/NONE | VERIFY |
| `message_encoding` | LENGTH_PREFIXED/DELIMITED | LENGTH_PREFIXED |
| `json_format` | ALLOW/LEGACY_BEST_EFFORT | ALLOW |
| `enforce_naming_style` | STYLE_LEGACY/STYLE2024/STYLE2026 | 2024/2026 |
| `default_symbol_visibility` | EXPORT_ALL/EXPORT_TOP_LEVEL/LOCAL_ALL/STRICT | 2024→TOP_LEVEL, 2026→STRICT |
| `enforce_proto_limits` | LEGACY_/PROTO_LIMITS2026 | 2026→PROTO_LIMITS2026 |

```proto
option features.field_presence = EXPLICIT;
option features.enum_type = CLOSED;
option features.repeated_field_encoding = EXPANDED;
option features.(pb.cpp).string_type = VIEW;
```

## 7. `protoc`

```bash
protoc --version
protoc -I. --cpp_out=gen order.proto
protoc -I. --python_out=gen --pyi_out=gen --grpc_python_out=gen order.proto
protoc -I. --java_out=gen --grpc-java_out=gen order.proto
protoc -I. --go_out=gen --go-grpc_out=gen order.proto
protoc -I. --descriptor_set_out=bundle.desc --include_imports order.proto
protoc -I. --decode=acme.orders.v1.Order order.proto < data.bin
protoc -I. --encode=acme.orders.v1.Order order.proto < data.txt > data.bin
protoc --decode_raw < data.bin
protoc --print_free_field_numbers order.proto
protoc --help
```

## 8. Buf

```bash
buf lint
buf format -w
buf breaking --against '.git#branch=main'
buf generate
buf build -o image.bin
buf push
grpcurl -plaintext localhost:50051 list
grpcurl -plaintext -d '{"id":"o-1"}' localhost:50051 acme.orders.v1.OrderService/GetOrder
```

## 9. gRPC status codes

| Nº | Código | Retry? |
|---|---|---|
| 0 | OK | — |
| 1 | CANCELLED | não |
| 2 | UNKNOWN | não |
| 3 | INVALID_ARGUMENT | não |
| 4 | DEADLINE_EXCEEDED | talvez |
| 5 | NOT_FOUND | não |
| 6 | ALREADY_EXISTS | não |
| 7 | PERMISSION_DENIED | não |
| 8 | RESOURCE_EXHAUSTED | backoff |
| 9 | FAILED_PRECONDITION | não |
| 10 | ABORTED | em nível superior |
| 11 | OUT_OF_RANGE | não |
| 12 | UNIMPLEMENTED | não |
| 13 | INTERNAL | não |
| 14 | UNAVAILABLE | sim, backoff |
| 15 | DATA_LOSS | não |
| 16 | UNAUTHENTICATED | não |

## 10. ProtoJSON

| Proto | JSON |
|---|---|
| int64/uint64/fixed64 | string |
| bytes | base64 |
| enum | nome |
| Timestamp | RFC 3339 string |
| Duration | string `"3s"` |
| FieldMask | string camelCase com vírgulas |
| Struct/Value | JSON nativo |
| Any | `@type` + campos |
| campo no default | omitido (salvo opção) |

## 11. Comandos de debug

```bash
protoc --decode_raw < payload.bin        # sem schema
protoc --decode=PKG.Msg x.proto < p.bin  # com schema
protoc --encode=PKG.Msg x.proto < p.txt  # texto→binário
```

## 12. Regras de ouro

1. Nunca reutilize número de campo — `reserved`.
2. Enum `0 = *_UNSPECIFIED`.
3. `string` = UTF-8; binário = `bytes`.
4. Dinheiro = inteiro em unidade mínima; tempo = `Timestamp`.
5. Negativos = `sint*`; IDs grandes = `fixed*`.
6. Campos 1–15 para os quentes.
7. Presença explícita quando "ausente ≠ default".
8. Sem `required`/`group`/extensions de dados.
9. Versão no pacote (`v1`), nunca no campo.
10. Runtime = gencode (mesma versão).
11. Entrada não-confiável: binário + limites.
12. `Any.type_url` validado.
13. Sensíveis com `debug_redact`.
14. `buf breaking` no CI.
15. Streaming para volumes grandes; limites/keepalive alinhados.

## 13. Matriz de compatibilidade (resumo)

| Mudança | OK? |
|---|---|
| add campo | ✅ |
| remove + reserved | ✅ |
| rename (binário) | ✅ (JSON ❌) |
| int32↔int64/uint/bool | ✅ |
| int32→sint32/fixed32 | ⛔ |
| string↔bytes | ⚠️ |
| add valor enum | ✅ |
| repeated→singular | ⛔ |
| add a oneof | ✅ |
| reutilizar número | ⛔ |
| trocar pacote | ⛔ |

## 14. Arquivos deste guia

`00` visão geral · `01` fundamentos · `02` contrato `.proto` ·
`03` editions/features · `04` tipos · `05` wire format · `06` presença ·
`07` geração de código · `08` WKT · `09` JSON/TextFormat · `10` gRPC ·
`11` segurança · `12` performance · `13` logs · `14` testes ·
`15` evolução · `16` boas práticas · `17` troubleshooting.
