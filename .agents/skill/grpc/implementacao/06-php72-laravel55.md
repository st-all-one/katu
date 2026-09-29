# 06 — PHP 7.2 + Laravel 5.5: gRPC (cliente)

> **Escopo:** PHP 7.2 (sem typed properties, arrow functions, `??=`, `match`,
> enums, attributes, union types, constructor promotion). Laravel 5.5 (LTS,
> 2017). **Nada incompatível com 7.2** pode aparecer no código.
>
> Papel típico: o Laravel atua como **cliente gRPC** de serviços externos.
> Servidor gRPC em PHP 7.2 é incomum (use um serviço Go/Java ou RoadRunner em
> PHP ≥7.4); veja a nota no fim.
>
> **Alternativa sem gRPC (recomendada quando `ext-grpc`/HTTP-2 for obstáculo):**
> use protobuf puro sobre HTTP/REST, filas ou cache, sem `ext-grpc` — ver
> [`../../protobuf_guide/10-protobuf-sem-grpc.md`](../../protobuf_guide/10-protobuf-sem-grpc.md).
> O mesmo `.proto` e o mesmo rigor de contrato (`buf lint`/`buf breaking`) valem
> nos dois casos.

## 1. Requisitos e versões

| Componente | Versão compatível |
|---|---|
| PHP | 7.2.x |
| Laravel | 5.5.x |
| `google/protobuf` (Composer) | linha **3.x** (ex.: `^3.25`) — confirme a matriz oficial |
| `grpc/grpc` (Composer) | `^1.x` |
| `ext-grpc` (PECL) | `1.3x`+ (compatível com PHP 7.2) |
| `ext-protobuf` (PECL) | opcional, para performance |
| `protoc` | compatível com a linha do runtime (3.x/4.x) |
| `grpc_php_plugin` | mesma versão do `ext-grpc` |

> **Regra:** gencode e runtime devem ser compatíveis. Para PHP 7.2, evite
> `google/protobuf` 4.x (que exige PHP 8.1+).

## 2. Instalação

```bash
# Extensões (produção)
sudo pecl install grpc
sudo pecl install protobuf
# habilite em php.ini:
#   extension=grpc.so
#   extension=protobuf.so

# Dependências PHP
composer require google/protobuf:^3.25 grpc/grpc:^1.0
```

`composer.json` da aplicação:

```json
{
  "require": {
    "php": ">=7.2",
    "laravel/framework": "5.5.*",
    "google/protobuf": "^3.25",
    "grpc/grpc": "^1.0",
    "ext-grpc": "*"
  },
  "autoload": {
    "psr-4": {
      "App\\": "app/",
      "Acme\\Catalog\\": "app/Generated/Acme/Catalog/"
    },
    "classmap": ["app/Generated/GPBMetadata/"]
  }
}
```

> O código gerado usa `GPBMetadata` (registro de descritores) e classes de
> mensagem. Inclua ambos no autoload.

## 3. Geração de código

Adicione opções ao `.proto`:

```proto
option php_namespace = "Acme\\Catalog\\V1";
option php_metadata_namespace = "Acme\\Catalog\\GPBMetadata";
```

Compile:

```bash
protoc -I proto \
  --php_out=app/Generated \
  --grpc_out=app/Generated \
  --plugin=protoc-gen-grpc=$(which grpc_php_plugin) \
  proto/acme/catalog/v1/catalog.proto
```

Resultado:
- `app/Generated/Acme/Catalog/V1/Product.php`, `Status.php`, `GetProductRequest.php`, ...
- `app/Generated/Acme/Catalog/V1/CatalogServiceClient.php` (stub gRPC).
- `app/Generated/GPBMetadata/Acme/Catalog/V1/Catalog.php` (descritores).

## 4. Uso das mensagens (PHP 7.2)

```php
<?php
declare(strict_types=1);

namespace App\Services;

use Acme\Catalog\V1\Product;
use Acme\Catalog\V1\Status;

// Construtor aceita array associativo
$product = new Product([
    'id' => 'p-1',
    'name' => 'Café',
    'price_minor' => 1999,
    'currency' => 'BRL',
    'status' => Status::STATUS_ACTIVE,
]);

$bytes = $product->serializeToString();

$parsed = new Product();
$parsed->mergeFromString($bytes);

// getters/setters
$parsed->setName('Café');
$name = $parsed->getName();
```

- Mensagens em PHP são mutáveis (setters).
- Enums são constantes de classe (`Status::STATUS_ACTIVE`).
- `oneof`: somente um campo setado; use os getters/setters correspondentes.
- `map`: `$msg->getLabels()` retorna array associativo; `$msg->getLabels()['k']`.
- Campos `repeated`: `$msg->getItems()` e `$msg->setItems([...])`.
- `Timestamp`: `$msg->getCreatedAt()` retorna `Google\Protobuf\Timestamp`.

## 5. Cliente gRPC (Serviço Laravel)

```php
<?php
declare(strict_types=1);

namespace App\Services;

use Acme\Catalog\V1\CatalogServiceClient;
use Acme\Catalog\V1\GetProductRequest;
use Acme\Catalog\V1\Product;
use Grpc\ChannelCredentials;

class CatalogClient
{
    /** @var CatalogServiceClient */
    private $client;

    public function __construct(string $host)
    {
        $this->client = new CatalogServiceClient($host, [
            'credentials' => ChannelCredentials::createInsecure(),
            // produção:
            // 'credentials' => ChannelCredentials::createSsl(file_get_contents('ca.pem')),
            'update_metadata' => function ($meta) {
                $meta['authorization'] = array('Bearer ' . config('services.catalog.token'));
                return $meta;
            },
        ]);
    }

    public function getProduct(string $id): Product
    {
        $request = new GetProductRequest();
        $request->setId($id);

        // [response, status] = ...
        /** @var Product $response */
        /** @var \Grpc\Status $status */
        list($response, $status) = $this->client->GetProduct($request)->wait();

        if ($status->code !== \Grpc\STATUS_OK) {
            throw new \RuntimeException(sprintf(
                'gRPC GetProduct falhou: code=%d details=%s',
                $status->code,
                $status->details
            ));
        }

        return $response;
    }

    public function close(): void
    {
        $this->client->close();
    }
}
```

### Provider e singleton (Laravel 5.5)

```php
<?php
namespace App\Providers;

use App\Services\CatalogClient;
use Illuminate\Support\ServiceProvider;

class GrpcServiceProvider extends ServiceProvider
{
    public function register()
    {
        $this->app->singleton(CatalogClient::class, function ($app) {
            return new CatalogClient(config('grpc.catalog.host'));
        });
    }
}
```

Registre em `config/app.php` → `providers`:

```php
App\Providers\GrpcServiceProvider::class,
```

`config/grpc.php`:

```php
<?php
return [
    'catalog' => [
        'host' => env('CATALOG_GRPC_HOST', 'catalog:50051'),
    ],
];
```

Uso no controller:

```php
public function show(CatalogClient $client, string $id)
{
    $product = $client->getProduct($id);
    return response()->json([
        'id' => $product->getId(),
        'name' => $product->getName(),
        'price_minor' => $product->getPriceMinor(),
        'currency' => $product->getCurrency(),
    ]);
}
```

## 6. Domínio vs DTOs

Não espalhe as classes geradas pela aplicação. Converta para DTOs/eloquentes na
borda:

```php
final class ProductDto
{
    /** @var string */
    public $id;
    /** @var string */
    public $name;
    /** @var int */
    public $priceMinor;
    /** @var string */
    public $currency;

    public static function fromProto(Product $p): self
    {
        $dto = new self();
        $dto->id = $p->getId();
        $dto->name = $p->getName();
        $dto->priceMinor = $p->getPriceMinor();
        $dto->currency = $p->getCurrency();
        return $dto;
    }
}
```

## 7. Jobs assíncronos e retry

```php
class SyncProduct implements ShouldQueue
{
    use Dispatchable, InteractsWithQueue, Queueable, SerializesModels;

    public $tries = 5;
    public $backoff = 10;

    /** @var string */
    private $productId;

    public function __construct(string $productId)
    {
        $this->productId = $productId;
    }

    public function handle(CatalogClient $client)
    {
        $product = $client->getProduct($this->productId);
        // ... persistir
    }
}
```

- Retry apenas em falhas transientes (`UNAVAILABLE`, `DEADLINE_EXCEEDED`).
- Nunca serialize mensagens protobuf em fila sem string binária
  (`serializeToString()`/`mergeFromString`).

## 8. Segurança

- **TLS obrigatório** em produção: `ChannelCredentials::createSsl(...)`.
  `createInsecure()` só para desenvolvimento.
- Tokens em metadata via `update_metadata`; nunca no payload.
- Valide entradas (ids, faixas, UTF-8) antes de chamar e após receber.
- Limite de tempo: `$client->GetProduct($req, [], ['timeout' => 2000000])` (µs).
- Não logue payloads com PII; mensagens protobuf em PHP não têm redação
  automática — selecione campos ao logar.
- Mantenha `ext-grpc`/`ext-protobuf` atualizados (segurança).

## 9. Logs e observabilidade (Monolog)

```php
use Illuminate\Support\Facades\Log;

Log::info('grpc.getProduct', [
    'id' => $id,
    // NÃO inclua $product->serializeToString()
    'status' => $status->code,
    'duration_ms' => (int) ((microtime(true) - $start) * 1000),
]);
```

- Laravel 5.5 usa Monolog; configure canais em `config/logging.php`.
- Registre `method`, `code`, `duration`, `host` — sem payload.
- Correlação: gere `x-request-id` e propague via metadata.
- OpenTelemetry PHP (compatível com 7.2? verifique) para traces.

## 10. Performance

- Reutilize o **singleton** do client (não crie por request).
- Use `ext-protobuf` (C) em vez do runtime puro-PHP para velocidade.
- `ext-grpc` é obrigatório; sem ele não há canal nativo.
- Evite serializar mensagens gigantes; use paginação/streaming no serviço.
- PHP-FPM tradicional recria o processo; o canal gRPC por request tem custo —
  considere RoadRunner/Swoole apenas em PHP ≥7.4.
- Cacheie respostas quando o domínio permitir.

## 11. Testes

```php
class CatalogClientTest extends TestCase
{
    public function testGetProduct()
    {
        $client = Mockery::mock(CatalogClient::class);
        $client->shouldReceive('getProduct')->with('p-1')->andReturn(
            (new Product())->setId('p-1')->setName('Café')->setPriceMinor(1999)
        );
        $this->app->instance(CatalogClient::class, $client);

        $this->get('/products/p-1')->assertJson(['id' => 'p-1']);
    }
}
```

- Teste serialização: `mergeFromString(serializeToString())` igual.
- Teste mapeamento de erro (`STATUS_*` → exceção/HTTP).
- Teste integração com um servidor gRPC real quando possível (docker-compose).
- Use `phpunit` do Laravel 5.5.

## 12. Pegadinhas

- `google/protobuf` 4.x **não** roda em PHP 7.2 — pine 3.x.
- Sem `ext-grpc`, o client não funciona (classes `Grpc\*`).
- `list($response, $status) = ...->wait()` é o padrão do stub; `$status->code`.
- Mensagens geradas registram descritores em `GPBMetadata`; o autoload precisa
  incluí-los, senão `mergeFromString` falha.
- `oneof`/`optional`: verifique qual campo está setado; PHP não tem `has` para
  todos os casos (getters retornam null/default).
- Não use features de PHP 7.4+/8.x (typed properties, arrow fns, `match`,
  enums, attributes, constructor promotion, union types).
- Nomes de classe/namespace do `php_namespace` precisam bater com o PSR-4.
- PHP 7.2 é **EOL**: sem patches de segurança — planeje upgrade.

## 13. gRPC server em PHP 7.2 (nota)

O `ext-grpc` moderno expõe `Grpc\RpcServer` (exemplo oficial
`examples/php/greeter_server.php`), mas:
- versões antigas do `ext-grpc` (compatíveis com 7.2) podem **não** tê-lo;
- rodar servidor concorrente em PHP-FPM não é viável.

Alternativas:
- Implemente o servidor em Go/Java/Rust e o Laravel como cliente.
- Use `spiral/roadrunner-grpc` (requer PHP ≥7.4) com Laravel quando puder
  atualizar o PHP.
- Exponha REST/JSON via `grpc-gateway` e consuma com HTTP no Laravel.

## 14. Referências

- `protobuf_docs/php/README.md`, `php/REFCOUNTING.md`.
- `grpc_docs/examples/php/greeter_client.php`,
  `grpc_docs/examples/php/greeter_server.php`, `composer.json` (`grpc/grpc`).
- `grpc_docs/doc/statuscodes.md`, `keepalive.md`, `compression.md`.
