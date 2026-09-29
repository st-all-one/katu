---
name: protobuf
description: >
  Protocol Buffers v37 (protoc 37, editions 2023/2024/2026): full .proto
  contract, scalar types and numbers, wire format, field presence, editions
  and features, code generation (protoc/plugins/buf/Bazel/CMake), well-known
  types, ProtoJSON/TextFormat, security/threat model, performance,
  logging/redaction, tests/conformance/fuzzing, contract evolution and
  versioning, best practices and troubleshooting. Covers using protobuf
  standalone (without gRPC) over HTTP/REST, queues, files and caches —
  including PHP 7.2/Laravel 5.5. For gRPC (RPC, transport, xDS, LB, auth)
  load the companion `grpc` skill. Load when designing, writing, reviewing,
  compiling, serializing, or evolving .proto schemas and their generated code.
category: protocols
version: "37 (editions 2023/2024/2026)"
tags: [protobuf, protoc, proto2, proto3, editions, features, wire-format, serialization, protobuf-json, well-known-types, buf, codegen, security, performance, conformance, schema-evolution, no-grpc]
license: MIT
---

# Protocol Buffers v37

Language-neutral, platform-neutral binary IDL + serialization. Contract =
`.proto`; compiler = `protoc`; per-language runtimes. Current contract model:
**Editions** (`edition = "2023" | "2024" | "2026"`), replacing
`syntax = "proto2|proto3"`.

> Scope: **pure protobuf** (contract, wire format, codegen, serialization).
> RPC/transport lives in the sibling skill [`../grpc_guide/`](../grpc_guide/),
> which reuses the same `.proto` as IDL. To use protobuf without gRPC (e.g.
> PHP 7.2 without `ext-grpc`), see `10-protobuf-sem-grpc.md`.

## Use When
- Design/evolve `.proto` (messages, enums, oneof, maps).
- Pick field types/numbers, presence, packing, editions/features.
- Compile with `protoc`/plugins/Bazel/CMake/Buf; wire up runtimes.
- Serialize/parse binary, ProtoJSON or TextFormat safely.
- Use protobuf **without gRPC** (HTTP/REST, queues, files, cache, BLOB).
- Harden parsing of untrusted input; enforce limits.
- Optimize size/speed (arenas, strings, packing).
- Test (round-trip, conformance, fuzzing) and evolve contracts safely.
- Redact sensitive fields in logs.

## Avoid When
- You need ready-made typed RPC (status, streaming, deadlines) → `grpc` skill.
- Runtime-visible, human-readable schema → plain JSON.
- Large-scale columnar/tabular analytics → Parquet/Avro/Arrow.
- End-user structured document editing → YAML/JSON/XML.

## Non-negotiable rules
1. **Never reuse a field number.** `reserved` it when removing.
2. **Never rename under wire compatibility** without understanding JSON/
   TextFormat: binary ignores names, JSON/TextFormat use them as keys.
3. **Enums start at `0`**, and zero must be the default/sentinel
   (`*_UNSPECIFIED` / `*_UNKNOWN`).
4. **Do not use `required`** (proto2) in new contracts; use explicit presence
   (`optional` in proto3 / `features.field_presence = EXPLICIT`).
5. **`int32/int64` are bad for negative values** — use `sint*` (zigzag).
   Money/time → dedicated types (`int64` minor units + scale, `Timestamp`,
   `Duration`), never `float`.
6. **On the wire**: `string` requires valid UTF-8 (proto3/editions default
   `VERIFY`); arbitrary bytes → `bytes`.
7. **Absent message ≠ empty message** — distinguish `has_x`/presence from default.
8. **Never parse untrusted input without limits** (size, depth, total bytes).
   The transport (e.g. gRPC) may cap at 4 MiB.
9. **Keep `protoc`/gencode and runtime at the exact same version**; apply
   patches (security).
10. **Treat `.proto` as code** (review, lint, supply chain).
11. **Do not publish generated code** as the primary artifact — publish `.proto`.
12. **Prefer binary** for hostile input; JSON/TextFormat are secondary and
    weaker.
13. **`Any` validates `type_url`** before unpack (prevents type confusion).
14. **Sensitive fields**: mark with `debug_redact`/`[debug_redact = true]` and
    never log raw payloads.

## API quick map
| Need | Construct |
|---|---|
| File | `edition = "2023"` (or `syntax = "proto2"`/`"proto3"`) + `package` |
| Import | `import "google/protobuf/timestamp.proto";` (`public`/`weak` legacy) |
| Message | `message Foo { ... }` |
| Singular field | `int32 id = 1;` |
| Explicit presence | `optional int32 id = 1;` (proto3) / `[features.field_presence = EXPLICIT]` |
| Repeated | `repeated string tags = 2;` (packed by default in proto3/editions) |
| Map | `map<string, int64> counts = 3;` |
| Union | `oneof choice { string a = 4; int32 b = 5; }` |
| Enum | `enum E { E_UNSPECIFIED = 0; A = 1; }` |
| Nested | `message Inner {}` inside a message |
| Reserve | `reserved 6, 15 to 20; reserved "old_name";` |
| Option | `option java_package = "com.x";` / `[deprecated = true]` |
| Feature | `option features.field_presence = EXPLICIT;` |
| Service (gRPC) | `service S { rpc M (Req) returns (Resp); }` → `grpc` skill |

## Commands
```bash
protoc --proto_path=. --cpp_out=./gen foo.proto
protoc -I. --python_out=gen --pyi_out=gen foo.proto
protoc -I. --descriptor_set_out=foo.desc --include_imports foo.proto
buf lint
buf breaking --against '.git#branch=main'
buf generate
# protobuf without gRPC, PHP 7.2 (no ext-grpc)
protoc -I proto --php_out=app/Generated proto/acme/catalog/v1/catalog.proto
```

## Modern contract template
```proto
edition = "2023";
package acme.orders.v1;

import "google/protobuf/timestamp.proto";

option java_multiple_files = true;
option java_package = "com.acme.orders.v1";
option go_package = "acme.com/orders/v1;ordersv1";

message Order {
  reserved 7, 9 to 11;
  reserved "legacy_status";

  string id = 1;
  int64 amount_minor = 2;              // money in minor units
  string currency = 3;                 // ISO-4217
  Status status = 4;
  repeated LineItem items = 5;
  google.protobuf.Timestamp created_at = 6;
  optional string coupon_code = 12;    // explicit presence
}

enum Status {
  STATUS_UNSPECIFIED = 0;
  STATUS_PENDING = 1;
  STATUS_PAID = 2;
}

message LineItem { string sku = 1; uint32 quantity = 2; }
```

## File Map
| File | Content |
|---|---|
| `00-indice.md` | Overview, architecture, map, read order |
| `01-fundamentos.md` | What it is, when to use, comparisons, ecosystem, install |
| `02-contrato-do-proto.md` | **Full `.proto` contract**: grammar, messages, fields, enums, oneof, maps, services, extensions, options, reserved |
| `03-editions-e-features.md` | **Modern model**: editions 2023/2024/2026, FeatureSet, inheritance, per-language features, migration |
| `04-tipos-e-numeros.md` | Scalar types, ranges, defaults, selection, field numbers |
| `05-wire-format.md` | TLV, varint, zigzag, packing, unknown fields, maps, limits |
| `06-presenca-de-campos.md` | No/explicit/legacy presence, proto3 optional, oneof, merging |
| `07-geracao-de-codigo.md` | `protoc`, plugins, Buf, Bazel/CMake, per language, descriptors |
| `08-tipos-bem-conhecidos.md` | Any, Timestamp, Duration, Struct, Wrappers, FieldMask, Empty |
| `09-json-e-textformat.md` | ProtoJSON mapping, options, TextFormat, safe parsing |
| `10-protobuf-sem-grpc.md` | **Protobuf without gRPC**: HTTP/REST, queues, files, cache; PHP 7.2/Laravel 5.5 |
| `11-seguranca.md` | Threat model, defensive limits, supply chain, Any, UTF-8, fuzzing |
| `12-performance.md` | Arenas, strings, packing, lazy, compression, tuning |
| `13-logs-e-observabilidade.md` | Redaction (`debug_redact`), safe logging, OTel, metrics |
| `14-testes.md` | Unit/round-trip, golden, conformance, fuzzing, interop, breaking change |
| `15-evolucao-de-contrato.md` | Compatibility rules, reserved, versioning, migration |
| `16-boas-praticas.md` | Naming, layout, style, defaults, codegen hygiene |
| `17-troubleshooting.md` | Common compile/runtime/JSON errors |
| `18-referencia-rapida.md` | Cheatsheet: syntax, types, wire types, features, commands |
| `../grpc_guide/` | Sibling skill: RPC with gRPC + per-stack implementation guides |

## Read Order (token-efficient)
Always `00` → `01`. Contract: `02`+`03`+`04`. Serialization: `05`+`06`+`09`.
Build: `07`. Production: `11`+`12`+`13`. Quality: `14`+`15`+`16`.
No-gRPC: `10`. Errors: `17`. Lookup: `18`.

## Prereqs
None. JSON/HTTP familiarity and any protobuf runtime (C++, Java/Kotlin, Python,
Go, C#, Rust, PHP, Ruby, JS/TS) help.
