# 01 — Fundamentos de gRPC

## 1. Definição

gRPC é um framework RPC que combina:

- **IDL**: por padrão `.proto` (protobuf). O `service` define métodos e tipos de
  entrada/saída.
- **Codegen**: `protoc` + plugins geram stub de cliente e interface de servidor.
- **Transporte**: HTTP/2 (multiplexação, headers binários, flow control,
  trailers). HTTP/3 (QUIC) em implementações modernas.
- **Serialização**: protobuf binário por padrão; plugável (ex.: FlatBuffers,
  MessagePack) via codecs.

Diferente de REST, o foco é **chamada de procedimento tipada**, não recursos/URLs.

## 2. Modelo RPC

```proto
syntax = "proto3";
package acme.orders.v1;

service OrderService {
  rpc GetOrder    (GetOrderRequest)    returns (Order);
  rpc ListOrders  (ListOrdersRequest)  returns (ListOrdersResponse);
  rpc WatchOrders (WatchOrdersRequest) returns (stream Order);
  rpc Upload      (stream Chunk)       returns (UploadSummary);
}

message GetOrderRequest { string id = 1; }
message WatchOrdersRequest { string filter = 1; }
message Order { string id = 1; int64 amount_minor = 2; }
```

- O **cliente** chama um método aparentemente local; o stub serializa, envia,
  aguarda e desserializa.
- O **servidor** implementa a interface; o runtime recebe, desserializa e
  invoca.
- Modos **síncrono** (bloqueante) e **assíncrono** (Future/Promise/async) na
  maioria das linguagens.

## 3. Anatomia de uma chamada

```
Cliente                                    Servidor
  |-- Request-Headers (call definition, metadata)
  |-- Length-Prefixed-Message (request)
  |-- END_STREAM ----------------------------------->
  |                                    Response-Headers + metadata
  |<-- Length-Prefixed-Message (response)
  |<-- Trailers (grpc-status, grpc-message, trailing metadata)
```

- `:method POST`, `:path /pacote.Serviço/Método`.
- `te: trailers` (detecta proxies incompatíveis).
- `content-type: application/grpc[+proto]`.
- `grpc-timeout`, `grpc-encoding`, `grpc-accept-encoding`.
- Status em trailers (ou `Trailers-Only` em erro imediato).

Detalhes completos em `02-protocolo-http2-e-web.md`.

## 4. Serviço, método e nomes

- Nome totalmente qualificado do método:
  `/pacote.Serviço/Método` (case-sensitive).
- Convenção: pacote com versão (`acme.orders.v1`); `PascalCase` em
  serviço/método; request/response dedicados (`GetOrderRequest`/`Order`).
- Nunca reutilize número de campo das mensagens (regra do protobuf).
- Adicionar método/serviço é compatível; remover exige deprecação.

## 5. Geração de código

```bash
# Go
protoc -I. --go_out=gen --go-grpc_out=gen order.proto
# Python
protoc -I. --python_out=gen --grpc_python_out=gen order.proto
# Java
protoc -I. --java_out=gen --grpc-java_out=gen order.proto
# C++
protoc -I. --cpp_out=gen --grpc_out=gen order.proto
# Rust (tonic) — via build.rs/prost
# Dart
protoc -I. --dart_out=grpc:lib/src order.proto
```

Cada geração produz:

| Artefato | Conteúdo |
|---|---|
| Mensagens | request/response |
| `*Client`/`*Stub` | cliente com métodos |
| `*Server`/`Service` | interface de servidor a implementar |
| Registro | função `Register<Serviço>Server` e descritores |

O arquivo gerado **não** deve ser editado à mão. Ver `03-servicos-e-contrato.md`
e `implementacao/`.

## 6. Deadline, cancelamento e metadata (visão geral)

- **Deadline**: cada chamada deve ter timeout; o servidor recebe `grpc-timeout`
  e deve cancelar ao estourar; propague para dependências.
- **Cancelamento**: cliente/servidor cancelam via contexto; libere recursos.
- **Metadata**: pares chave/valor em headers/trailers (auth, tracing, tenant).
  Chaves ASCII minúsculas; valores binários em `-bin` (base64).
- **wait_for_ready**: aguardar canal ficar pronto em vez de falhar na hora.
- **fail_fast**: falhar imediatamente (default em muitas linguagens).

Ver `05-metadata-deadlines-cancelamento.md`.

## 7. Quando usar gRPC

| Cenário | Recomendação |
|---|---|
| Serviços internos, alto volume, multi-linguagem | ✅ gRPC |
| Streaming de eventos/telemetria | ✅ gRPC |
| Mobile/desktop com contrato forte | ✅ gRPC |
| Navegador | gRPC-Web/Connect (proxy/transcoding) |
| API pública REST | grpc-gateway ou REST puro |
| Ambiente sem HTTP/2/extensão (PHP 7.2) | protobuf sem gRPC |
| Cache HTTP, CDN, URLs amigáveis | REST |

## 8. Comparação rápida com REST

| Aspecto | gRPC | REST |
|---|---|---|
| Estilo | procedimentos | recursos |
| Contrato | `.proto` | OpenAPI (opcional) |
| Payload | binário | JSON (texto) |
| Verbos | método RPC | GET/POST/PUT/DELETE |
| Streaming | nativo (4 modos) | SSE/WebSocket |
| Browsers | via proxy | nativo |
| Debug | grpcurl/reflection | curl/DevTools |
| Tooling | codegen | universal |

Ambos podem coexistir: o mesmo serviço pode ser exposto via gRPC (interno) e
REST/JSON (externo) com grpc-gateway.

## 9. Ecossistema e ferramentas

| Ferramenta | Uso |
|---|---|
| `protoc` + `protoc-gen-*-grpc` | codegen |
| `buf` | lint, breaking, geração |
| `grpcurl` | cliente por reflection/descriptor |
| `grpc_cli` | cliente oficial (C++) |
| `ghz` | testes de carga |
| Envoy / grpc-gateway | proxy, transcoding, LB |
| OTel / Prometheus | observabilidade |

## 10. Versões e compatibilidade

- O wire protocol gRPC é estável; implementações evoluem de forma compatível
  (ver `14-operacao-versionamento-troubleshooting.md`).
- Mantenha o **plugin de codegen** e o **runtime** na mesma versão.
- Alterações no `.proto` seguem as regras do protobuf (nunca reutilizar número).

## 11. Primeiro exemplo (Go, resumo)

```go
// protoc -I. --go_out=gen --go-grpc_out=gen order.proto
type server struct{ pb.UnimplementedOrderServiceServer }

func (s *server) GetOrder(ctx context.Context, req *pb.GetOrderRequest) (*pb.Order, error) {
    return &pb.Order{Id: req.GetId()}, nil
}

func main() {
    lis, _ := net.Listen("tcp", ":50051")
    srv := grpc.NewServer()
    pb.RegisterOrderServiceServer(srv, &server{})
    srv.Serve(lis)
}
```

```go
conn, _ := grpc.NewClient("dns:///localhost:50051",
    grpc.WithTransportCredentials(insecure.NewCredentials()))
client := pb.NewOrderServiceClient(conn)
ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
defer cancel()
order, err := client.GetOrder(ctx, &pb.GetOrderRequest{Id: "o-1"})
```

## 12. Próximos passos

- Protocolo no fio: `02-protocolo-http2-e-web.md`.
- Definir serviço/contrato: `03-servicos-e-contrato.md`.
- Erros: `04-erros-e-status.md`. Chamadas: `05`.
- Implementação por stack: `implementacao/`.
