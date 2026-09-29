# 00 — Implementação de gRPC por stack

Estes arquivos complementam o guia de **gRPC** com receitas **prontas de
implementação** para pilhas específicas. Cada arquivo cobre: instalação, geração
de código, cliente/servidor, integração com o framework, segurança, logs,
performance, testes e pegadinhas da versão.

> Protobuf **puro** (serialização sem gRPC, HTTP/REST, filas) está na skill irmã
> [`../../protobuf_guide/`](../../protobuf_guide/), em especial
> `10-protobuf-sem-grpc.md`. O `.proto` abaixo é o mesmo usado nas duas skills.
> Caso o ambiente não suporte gRPC (ex.: PHP 7.2 sem `ext-grpc`), veja o guia de
> protobuf sem gRPC antes de optar por gRPC.

## 1. Contrato comum de exemplo

Todos os exemplos usam este `.proto` (proto3, para máxima compatibilidade de
toolchain; onde editions é suportado, anota-se).

```proto
// proto/acme/catalog/v1/catalog.proto
syntax = "proto3";
package acme.catalog.v1;

option java_multiple_files = true;
option java_package = "com.acme.catalog.v1";
option go_package = "acme.com/catalog/v1;catalogv1";
option csharp_namespace = "Acme.Catalog.V1";

service CatalogService {
  rpc GetProduct     (GetProductRequest)     returns (Product);
  rpc ListProducts   (ListProductsRequest)   returns (ListProductsResponse);
  rpc WatchProducts  (WatchProductsRequest)  returns (stream Product);
  rpc UploadProducts (stream Product)        returns (UploadSummary);
}

message Product {
  string id = 1;
  string name = 2;
  int64 price_minor = 3;
  string currency = 4;
  Status status = 5;
}

enum Status {
  STATUS_UNSPECIFIED = 0;
  STATUS_ACTIVE = 1;
  STATUS_ARCHIVED = 2;
}

message GetProductRequest    { string id = 1; }
message ListProductsRequest  { int32 page_size = 1; string page_token = 2; }
message ListProductsResponse { repeated Product products = 1; string next_page_token = 2; }
message WatchProductsRequest { string filter = 1; }
message UploadSummary        { int32 accepted = 1; int32 rejected = 2; }
```

Layout de repositório recomendado:

```
repo/
  proto/acme/catalog/v1/catalog.proto
  buf.yaml
  buf.gen.yaml
  <código da stack>/
```

## 2. Matriz de stacks

| Stack | Mensagens | gRPC | Plugins | Observações |
|---|---|---|---|---|
| **Rust** | `prost` / `google-protobuf` | `tonic` | `prost-build`, `tonic-build` | `build.rs`; async Tokio |
| **Go** | `google.golang.org/protobuf` | `google.golang.org/grpc` | `protoc-gen-go`, `protoc-gen-go-grpc` | mainstream; `bufconn` p/ testes |
| **Dart/Flutter** | `protobuf` | `grpc` | `protoc-gen-dart` | VM/servidor; web via Connect |
| **TypeScript** | `@bufbuild/protobuf` | `@connectrpc/connect` | `@bufbuild/protoc-gen-es`, `protoc-gen-connect-es` | ESM, browser e Node |
| **Web moderna** | protobuf-es | Connect-Web / gRPC-Web | idem + bundler | browser: sem HTTP/2 cru |
| **PHP 7.2 / Laravel 5.5** | `google/protobuf` ^3 | `grpc/grpc` + ext | `protoc`, `grpc_php_plugin` | gRPC server via RoadRunner |
| **PHP 8.4 / Laravel 12** | `google/protobuf` ^4 | `grpc/grpc` + ext | idem + attributes | RoadRunner/Octane |

## 3. Como escolher o transporte

| Ambiente | Recomendação |
|---|---|
| Serviços internos de alta performance | gRPC nativo (`tonic`, `grpc-go`, `grpc/grpc`) |
| Navegador | Connect-Web ou gRPC-Web (HTTP/1.1/2 sobre fetch) |
| Node/Bun/Deno | Connect ou `@grpc/grpc-js` |
| Flutter mobile | `grpc` Dart sobre HTTP/2 |
| Laravel (cliente) | `grpc/grpc` + ext-grpc |
| Laravel (servidor) | RoadRunner gRPC / Octane |
| API pública REST | grpc-gateway/Connect transcoding |

## 4. Arquivos

| Arquivo | Stack |
|---|---|
| `01-rust.md` | Rust (prost + tonic) |
| `02-go.md` | Go (protobuf + grpc-go) |
| `03-dart-flutter.md` | Dart / Flutter |
| `04-typescript.md` | TypeScript (Protobuf-ES + Connect / grpc-js) |
| `05-web-moderna.md` | Web moderna (browser, Connect-Web/gRPC-Web) |
| `06-php72-laravel55.md` | PHP 7.2 + Laravel 5.5 |
| `07-php84-laravel12.md` | PHP 8.4 + Laravel 12 |

## 5. Checklist transversal

- [ ] `.proto` revisado e versionado; `buf lint`/`buf breaking` no CI.
- [ ] Plugins e runtime na **mesma versão**.
- [ ] TLS/mTLS em produção; credenciais em metadata.
- [ ] Deadlines em toda chamada; retry só em códigos idempotentes.
- [ ] Limites de mensagem definidos (4 MiB default).
- [ ] Logs sem payload bruto; campos sensíveis redigidos.
- [ ] Testes de round-trip e de serviço (unary + streaming).
- [ ] Observabilidade (métricas/tracing) por método.
