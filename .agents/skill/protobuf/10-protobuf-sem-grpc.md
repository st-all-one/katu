# 10 — Protobuf sem gRPC (contrato rigoroso, transporte próprio)

> **Resposta curta:** sim. Protobuf é um **formato de serialização + IDL**,
> totalmente independente de gRPC. gRPC é apenas *um* transporte que usa
> protobuf. Você pode usar protobuf sobre HTTP/REST, filas, arquivos, cache,
> WebSocket, IPC ou banco de dados — mantendo schema forte, codegen e regras de
> compatibilidade.
>
> Isso resolve o caso de **PHP 7.2 / Laravel 5.5**, onde `ext-grpc`/HTTP-2 são
> o obstáculo: você usa só `protoc --php_out` + `google/protobuf` (puro ou
> `ext-protobuf`), sem `ext-grpc` e sem `grpc/grpc`.

## 1. O que você mantém sem gRPC

| Garantia | Permanece? | Como |
|---|---|---|
| Contrato versionado em `.proto` | ✅ | `protoc`, `buf` |
| Geração de código multi-linguagem | ✅ | `protoc --<lang>_out` |
| Compactação e velocidade binária | ✅ | wire format |
| Compatibilidade forward/backward | ✅ | regras de evolução (arquivo `15`) |
| Presença explícita, oneof, map, enums | ✅ | features proto3/editions |
| Detecção de breaking change | ✅ | `buf breaking` |
| Validação declarativa | ⚠️/✅ | protovalidate (verifique a linguagem) / validação manual |
| Descriptor set para reflection | ✅ | `--descriptor_set_out --include_imports` |
| ProtoJSON / TextFormat | ✅ | runtime |
| TLS | ✅ | camada HTTP/transporte |

## 2. O que gRPC adicionaria (e como compensar)

| Recurso do gRPC | Sem gRPC, você faz |
|---|---|
| RPC tipado gerado | Rotas REST + um `service` no `.proto` (ou só mensagens) |
| Status codes padronizados | HTTP status + corpo de erro protobuf/JSON |
| Streaming (4 padrões) | SSE, WebSocket, chunked JSON, filas |
| Multiplexing HTTP/2 | HTTP/1.1 keep-alive; HTTP/2 opcional |
| Deadlines/timeouts | timeout do cliente HTTP + deadline propagado |
| Interceptors | middleware HTTP |
| Retry/backoff | política no cliente/middleware |
| Metadata/trailers | headers HTTP |
| Reflection/health | endpoints `/health`, `/openapi.json` |
| Compressão por mensagem | `Content-Encoding: gzip` |

**O ponto central:** as garantias de **contrato** não vêm do gRPC — vêm do
`.proto` + `protoc` + disciplina de evolução. gRPC só padroniza o *RPC*.

## 3. Transports suportados (todos sem gRPC)

| Transporte | Formato | Uso |
|---|---|---|
| HTTP/REST (corpo binário) | `application/x-protobuf` | APIs, microserviços |
| HTTP/REST (JSON) | `application/json` (ProtoJSON) | interop web/legibilidade |
| Filas | binário protobuf no payload | Kafka/RabbitMQ/SQS/Redis Streams |
| Arquivos | binário + length-delimited | data lakes, export/import, logs |
| Cache/sessão | binário protobuf | Redis/Memcached |
| Banco de dados | `BLOB`/`BYTEA` | colunas de dados estruturados |
| WebSocket/SSE | frames protobuf | tempo real |
| IPC | binário | processos locais |
| gRPC-Web/Connect | — | opcional, se quiser RPC no browser |

O wire format é **transport-agnóstico**. Tudo o que você sabe de campo,
presença e compatibilidade continua valendo.

## 4. Padrão HTTP/REST com protobuf

### 4.1 Contrato

```proto
syntax = "proto3";
package acme.catalog.v1;

message Product {
  string id = 1;
  string name = 2;
  int64 price_minor = 3;
  string currency = 4;
  Status status = 5;
}
enum Status { STATUS_UNSPECIFIED = 0; STATUS_ACTIVE = 1; STATUS_ARCHIVED = 2; }

message GetProductRequest { string id = 1; }
message ListProductsRequest { int32 page_size = 1; string page_token = 2; }
message ListProductsResponse { repeated Product products = 1; string next_page_token = 2; }

// Erro de aplicação como mensagem protobuf (opcional)
message ApiError {
  int32 http_status = 1;
  string code = 2;
  string message = 3;
  repeated FieldViolation violations = 4;
}
message FieldViolation { string field = 1; string description = 2; }
```

### 4.2 Regras REST

| Método | Rota | Corpo | Resposta |
|---|---|---|---|
| GET | `/v1/products/{id}` | — | `Product` / 404 |
| GET | `/v1/products?page_size=&page_token=` | — | `ListProductsResponse` |
| POST | `/v1/products` | `Product` | `Product` / 201 |
| PATCH | `/v1/products/{id}` | `Product` + máscara | `Product` |

- `Content-Type: application/x-protobuf` (binário) ou `application/json`
  (ProtoJSON). Negocie via `Accept`.
- Erros: HTTP status + corpo `ApiError` (binário ou JSON), com `code` estável.
- Idempotência: `Idempotency-Key` em POST/PATCH.
- Paginação: `page_size`/`page_token` (nunca `offset`).
- ETag/If-Match para concorrência otimista (hash determinístico opcional).

### 4.3 Por que isso mantém o rigor

- O corpo é validado/parseado por código gerado (tipos exatos, presença,
  campos desconhecidos rejeitados ou preservados).
- `buf breaking` impede mudanças incompatíveis no CI.
- Você pode gerar OpenAPI/JSON Schema a partir do `.proto`
  (`protoc-gen-jsonschema`, `gnostic`, `buf` + plugins) para documentar a API
  REST externa.

## 5. Exemplo: PHP 7.2 + Laravel 5.5 **sem gRPC**

### 5.1 Dependências (mínimas)

```json
{
  "require": {
    "php": ">=7.2",
    "laravel/framework": "5.5.*",
    "google/protobuf": "^3.25"
  },
  "autoload": {
    "psr-4": { "Acme\\Catalog\\": "app/Generated/Acme/Catalog/" },
    "classmap": ["app/Generated/GPBMetadata/"]
  }
}
```

> Nada de `grpc/grpc` nem `ext-grpc`. Opcional: `ext-protobuf` (C) só para
> performance.

### 5.2 Geração

```proto
option php_namespace = "Acme\\Catalog\\V1";
option php_metadata_namespace = "Acme\\Catalog\\GPBMetadata";
```

```bash
protoc -I proto --php_out=app/Generated proto/acme/catalog/v1/catalog.proto
```

### 5.3 Middleware de negociação binário/JSON

```php
<?php
namespace App\Http\Middleware;

use Closure;

class ProtobufNegotiation
{
    public function handle($request, Closure $next)
    {
        $request->headers->set('X-Proto-Format',
            strpos((string) $request->header('Content-Type'), 'application/json') !== false
                ? 'json' : 'proto');
        $response = $next($request);
        if ($request->headers->get('X-Proto-Format') === 'json') {
            $response->headers->set('Content-Type', 'application/json');
        } else {
            $response->headers->set('Content-Type', 'application/x-protobuf');
        }
        return $response;
    }
}
```

### 5.4 Controller (parsear e serializar)

```php
<?php
declare(strict_types=1);

namespace App\Http\Controllers;

use Acme\Catalog\V1\Product;
use Acme\Catalog\V1\ApiError;
use Illuminate\Http\Request;

class ProductController extends Controller
{
    /** POST /v1/products */
    public function store(Request $request)
    {
        $product = new Product();

        if ($request->headers->get('X-Proto-Format') === 'json') {
            $product->mergeFromJsonString($request->getContent(), true);
        } else {
            // limite de recursão defensivo (parâmetro 2)
            $product->mergeFromString($request->getContent(), 64);
        }

        // validação manual (PHP 7.2; sem protovalidate oficial)
        $errors = array();
        if ($product->getId() === '') {
            $errors[] = array('field' => 'id', 'description' => 'obrigatório');
        }
        if ($product->getPriceMinor() < 0) {
            $errors[] = array('field' => 'price_minor', 'description' => 'deve ser >= 0');
        }
        if (!in_array($product->getStatus(), array(
            Product::STATUS_ACTIVE, Product::STATUS_ARCHIVED
        ), true)) {
            $errors[] = array('field' => 'status', 'description' => 'valor inválido');
        }

        if (count($errors) > 0) {
            $err = new ApiError();
            $err->setHttpStatus(422);
            $err->setCode('INVALID_ARGUMENT');
            $err->setMessage('produto inválido');
            // mapear violations...

            return response(
                $request->headers->get('X-Proto-Format') === 'json'
                    ? $err->serializeToJsonString()
                    : $err->serializeToString(),
                422
            );
        }

        // ... persistir (Eloquent)
        $product->setStatus(Product::STATUS_ACTIVE);

        return response(
            $request->headers->get('X-Proto-Format') === 'json'
                ? $product->serializeToJsonString()
                : $product->serializeToString(),
            201
        );
    }
}
```

### 5.5 Rota e serialização de listas

```php
Route::post('/v1/products', 'ProductController@store');
Route::get('/v1/products/{id}', 'ProductController@show');
```

```php
use Acme\Catalog\V1\ListProductsResponse;

$response = new ListProductsResponse();
foreach ($produtos as $p) {
    $proto = new Product();
    $proto->setId((string) $p->id);
    $proto->setName($p->name);
    $proto->setPriceMinor((int) $p->price_minor);
    $proto->setCurrency($p->currency);
    $response->setProducts(array_merge($response->getProducts(), array($proto)));
}
return response($response->serializeToString())
    ->header('Content-Type', 'application/x-protobuf');
```

### 5.6 Filas sem gRPC

```php
// Job: serialize para binário e guarde no payload da fila
$payload = $product->serializeToString();
\Queue::push(new ProcessProduct(base64_encode($payload)));

// Worker
$product = new Product();
$product->mergeFromString(base64_decode($data));
```

> Evite serializar a mensagem PHP diretamente em `SerializesModels`; converta
> para binário ou DTO.

### 5.7 Vantagens no PHP 7.2

- Sem `ext-grpc`, sem HTTP/2, sem `grpc_php_plugin`.
- Funciona em qualquer PHP-FPM 7.2.
- Usa a mesma validação de tipo/campos do protobuf.
- `buf lint`/`buf breaking` continuam protegendo o contrato.

## 6. Exemplos rápidos em outras stacks

### 6.1 Rust (Axum + protobuf)

```rust
use axum::{body::Bytes, extract::Path, http::HeaderMap, routing::{get, post}, Router};
use prost::Message;

async fn get_product(Path(id): Path<String>) -> Result<Vec<u8>, (axum::http::StatusCode, String)> {
    let p = Product { id, name: "Café".into(), price_minor: 1999, currency: "BRL".into(), ..Default::default() };
    Ok(p.encode_to_vec())
}

let app = Router::new()
    .route("/v1/products/:id", get(get_product));
```

### 6.2 Go (net/http)

```go
func getProduct(w http.ResponseWriter, r *http.Request) {
    p := &pb.Product{Id: "p-1", Name: "Café", PriceMinor: 1999, Currency: "BRL"}
    b, _ := proto.Marshal(p)
    w.Header().Set("Content-Type", "application/x-protobuf")
    w.Write(b)
}
```

### 6.3 TypeScript (fetch + Protobuf-ES)

```ts
const bytes = toBinary(ProductSchema, product);
await fetch("/v1/products", {
  method: "POST",
  headers: { "Content-Type": "application/x-protobuf" },
  body: bytes,
});
```

### 6.4 Dart/Flutter

```dart
final resp = await http.post(
  Uri.parse('$base/v1/products'),
  headers: {'Content-Type': 'application/x-protobuf'},
  body: product.writeToBuffer(),
);
final product = Product.fromBuffer(resp.bodyBytes);
```

## 7. Rigor de contrato sem gRPC (tooling)

1. `buf lint` e `buf breaking --against main` no CI — **isto é o que garante a
   evolução segura**, independente do transporte.
2. `protoc --descriptor_set_out --include_imports` como artefato para
   validação/tooling.
3. Gere OpenAPI/JSON Schema a partir do `.proto` para a API REST externa.
4. Testes de round-trip binário e JSON.
5. Regras de evolução: nunca reutilizar número, `reserved`, enum `0` estável,
   presença explícita onde "ausente ≠ default" (ver arquivos `06` e `15`).

## 8. Segurança (idêntica ao guia principal)

- TLS no transporte (HTTPS).
- Limites de tamanho e **recursão** no parse
  (`mergeFromString($data, 64)` no PHP; `RecursionLimit` em Go; limites em
  C++/Java).
- Rejeitar campos desconhecidos por padrão (JSON) e preservá-los no binário
  quando necessário.
- `Any.type_url` validado por allowlist.
- Autenticação/autorização no middleware HTTP (tokens/Passport), não no payload.
- Não logar payload bruto; selecionar campos.
- Em PHP 7.2, `google/protobuf` e `ext-protobuf` mantidos atualizados (dentro
  da linha 3.x compatível).

## 9. Performance sem gRPC

- Binário > JSON em banda, mas o servidor HTTP/FPM domina o custo.
- Reutilize HTTP keep-alive; evite abrir conexão por request.
- `ext-protobuf` acelera serialize/parse no PHP.
- Compressão `gzip` via HTTP quando o payload compensar.
- Paginação e streaming (SSE/chunked) para volumes grandes.
- Em PHP moderno, RoadRunner/Octane; em 7.2, FPM com cache.

## 10. Testes

- Round-trip binário/JSON por mensagem.
- Contrato HTTP: código de status + corpo protobuf esperado.
- Compatibilidade: ler dados "antigos" com schema novo.
- `buf breaking` no pipeline.
- Teste de robustez: payload truncado, profundidade excessiva, UTF-8 inválido.

## 11. Quando ainda assim usar gRPC

| Use gRPC quando | Prefira protobuf sem gRPC quando |
|---|---|
| Muitos serviços internos com RPC tipado | Ambiente sem HTTP/2/ext-grpc (PHP 7.2) |
| Streaming bidi de alta performance | API pública REST obrigatória |
| Necessidade de deadlines/retry padronizados | Integração com clientes HTTP/web simples |
| Ecossistema já em gRPC | Simplicidade operacional e portabilidade |
| Observabilidade padronizada por método | Filas/arquivos/cache como transporte |

Nada impede **usar os dois**: o mesmo `.proto` pode gerar stubs gRPC e também
ser usado em REST/filas. Migrar para gRPC depois é incremental — o schema não
muda.

## 12. Migração futura para gRPC

1. Mantenha o `.proto` único (mensagens e, se desejar, um `service`).
2. Hoje: exponha REST/filas usando as mensagens.
3. Amanhã: adicione `service` e gere stubs gRPC no **mesmo** schema.
4. Como o wire format não muda, os dados trafegados continuam compatíveis.

## 13. Resumo

- **Protobuf ≠ gRPC.** O contrato e o wire format são independentes do
  transporte.
- Sem gRPC você mantém schema forte, codegen, presença, compatibilidade e
  compactação.
- Você perde apenas o *RPC padronizado* (status, streaming, deadlines,
  interceptors) — recomponível com HTTP/REST, filas e middleware.
- No **PHP 7.2/Laravel 5.5**, isso significa: `protoc --php_out` +
  `google/protobuf`, **sem `ext-grpc`**, com `serializeToString`/
  `mergeFromString` (binário) ou `serializeToJsonString`/`mergeFromJsonString`
  (JSON).
