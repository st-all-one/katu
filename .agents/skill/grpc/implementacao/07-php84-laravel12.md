# 07 — PHP 8.4 + Laravel 12: protobuf + gRPC

> **Escopo:** PHP 8.4 e Laravel 12 (requer PHP ≥8.2). Use todos os recursos
> modernos: typed properties, constructor promotion, enums, readonly, attributes,
> `match`, nullsafe, first-class callables, union/intersection/DNF e (8.4)
> property hooks, visibilidade assimétrica, `array_find`/`array_any`/`array_all`,
> lazy objects.
>
> Papel: Laravel pode ser **cliente** e **servidor** gRPC (servidor via
> RoadRunner/Octane ou `ext-grpc`).

## 1. Requisitos e versões

| Componente | Versão |
|---|---|
| PHP | 8.4 |
| Laravel | 12.x |
| `google/protobuf` | linha **4.x** (`^4.29`) |
| `grpc/grpc` | `^1.6x` |
| `ext-grpc` | `^1.6x` |
| `ext-protobuf` | recomendado (performance) |
| Servidor gRPC | `spiral/roadrunner-grpc` (`^4`) ou `ext-grpc` (`Grpc\RpcServer`) |
| Runtime de app | Laravel Octane (RoadRunner/Swoole/FrankenPHP) |

## 2. Instalação

```bash
sudo pecl install grpc protobuf
# php.ini:
#   extension=grpc.so
#   extension=protobuf.so

composer require google/protobuf:^4.29 grpc/grpc:^1.6
composer require spiral/roadrunner-grpc:^4 spiral/roadrunner-cli:^2 --dev
```

`composer.json` (trecho):

```json
{
  "require": {
    "php": "^8.4",
    "laravel/framework": "^12.0",
    "google/protobuf": "^4.29",
    "grpc/grpc": "^1.6",
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

## 3. Geração de código

```proto
option php_namespace = "Acme\\Catalog\\V1";
option php_metadata_namespace = "Acme\\Catalog\\GPBMetadata";
```

```bash
protoc -I proto \
  --php_out=app/Generated \
  --grpc_out=app/Generated \
  --plugin=protoc-gen-grpc=$(which grpc_php_plugin) \
  proto/acme/catalog/v1/catalog.proto
```

Opcionalmente use `buf generate` com um plugin local/remoto de PHP.

## 4. Mensagens com PHP moderno

O gencode continua expondo setters/getters e enums como **constantes de classe**
(não enums nativos). Isole-o em DTOs modernos na borda:

```php
<?php
declare(strict_types=1);

namespace App\Domain\Catalog;

use Acme\Catalog\V1\Product as ProductProto;
use Acme\Catalog\V1\Status as StatusProto;

enum Status: int
{
    case Unspecified = StatusProto::STATUS_UNSPECIFIED;
    case Active      = StatusProto::STATUS_ACTIVE;
    case Archived    = StatusProto::STATUS_ARCHIVED;

    public static function fromProto(int $value): self
    {
        return self::tryFrom($value) ?? self::Unspecified;
    }
}

final readonly class Product
{
    public function __construct(
        public string $id,
        public string $name,
        public int $priceMinor,
        public string $currency,
        public Status $status,
    ) {}

    public static function fromProto(ProductProto $p): self
    {
        return new self(
            id: $p->getId(),
            name: $p->getName(),
            priceMinor: $p->getPriceMinor(),
            currency: $p->getCurrency(),
            status: Status::fromProto($p->getStatus()),
        );
    }

    public function toProto(): ProductProto
    {
        return new ProductProto([
            'id' => $this->id,
            'name' => $this->name,
            'price_minor' => $this->priceMinor,
            'currency' => $this->currency,
            'status' => $this->status->value,
        ]);
    }
}
```

Serialização:

```php
$bytes = $product->serializeToString();
$proto = new ProductProto();
$proto->mergeFromString($bytes);
```

## 5. Cliente gRPC (Laravel 12)

```php
<?php
declare(strict_types=1);

namespace App\Infrastructure\Grpc;

use Acme\Catalog\V1\CatalogServiceClient;
use Acme\Catalog\V1\GetProductRequest;
use Acme\Catalog\V1\Product;
use Grpc\ChannelCredentials;
use RuntimeException;

final class CatalogClient
{
    private readonly CatalogServiceClient $client;

    public function __construct(private readonly string $token, string $host)
    {
        $this->client = new CatalogServiceClient($host, [
            'credentials' => app()->isProduction()
                ? ChannelCredentials::createSsl(file_get_contents(config('grpc.ca')))
                : ChannelCredentials::createInsecure(),
            'update_metadata' => function (array $meta): array {
                $meta['authorization'] = ['Bearer ' . $this->token];
                return $meta;
            },
        ]);
    }

    public function getProduct(string $id, int $timeoutMicros = 2_000_000): Product
    {
        $request = new GetProductRequest(['id' => $id]);

        /** @var Product $response */
        /** @var \Grpc\Status $status */
        [$response, $status] = $this->client
            ->GetProduct($request, [], ['timeout' => $timeoutMicros])
            ->wait();

        return match ($status->code) {
            \Grpc\STATUS_OK => $response,
            \Grpc\STATUS_DEADLINE_EXCEEDED, \Grpc\STATUS_UNAVAILABLE =>
                throw new RuntimeException('catalog indisponível', 503),
            \Grpc\STATUS_NOT_FOUND => throw new RuntimeException('produto não encontrado', 404),
            default => throw new RuntimeException("gRPC code={$status->code}: {$status->details}"),
        };
    }
}
```

> `->wait()` retorna `[$response, $status]`. Use desestruturação e `match` para
> mapear códigos. O `timeout` vai no terceiro argumento (opções), em µs.

### Provider (Laravel 12)

`app/Providers/GrpcServiceProvider.php`:

```php
namespace App\Providers;

use App\Infrastructure\Grpc\CatalogClient;
use Illuminate\Support\ServiceProvider;

final class GrpcServiceProvider extends ServiceProvider
{
    public function register(): void
    {
        $this->mergeConfigFrom(__DIR__.'/../../config/grpc.php', 'grpc');

        $this->app->singleton(CatalogClient::class, fn () => new CatalogClient(
            token: config('grpc.catalog.token'),
            host: config('grpc.catalog.host'),
        ));
    }
}
```

Registre em `bootstrap/providers.php`:

```php
return [
    App\Providers\AppServiceProvider::class,
    App\Providers\GrpcServiceProvider::class,
];
```

`config/grpc.php`:

```php
return [
    'ca' => env('GRPC_CA'),
    'catalog' => [
        'host' => env('CATALOG_GRPC_HOST', 'catalog:50051'),
        'token' => env('CATALOG_GRPC_TOKEN'),
    ],
];
```

## 6. Servidor gRPC com RoadRunner

`rr.yaml` (trecho):

```yaml
version: "3"
rpc:
  listen: tcp://127.0.0.1:6001
server:
  command: "php vendor/bin/rr-worker"
grpc:
  listen: "tcp://0.0.0.0:50051"
  proto:
    - "proto/acme/catalog/v1/catalog.proto"
  tls:
    key: "server.key"
    cert: "server.crt"
```

Serviço:

> O código do servidor usa a interface gerada pelo **plugin PHP do RoadRunner**
> (`protoc-gen-php-grpc`, do pacote `spiral/roadrunner-grpc`), que é diferente do
> cliente gerado pelo `grpc_php_plugin` padrão. Gere as interfaces de servidor
> com o plugin do RoadRunner (`rr grpc:generate` ou `protoc` + plugin) e os
> clientes/stubs com o `grpc_php_plugin`.

```php
namespace App\Grpc;

use Acme\Catalog\V1\CatalogServiceInterface;
use Acme\Catalog\V1\GetProductRequest;
use Acme\Catalog\V1\Product;
use Acme\Catalog\V1\Status;
use Spiral\RoadRunner\GRPC\ContextInterface;
use Spiral\RoadRunner\GRPC\Exception\NotFoundException;
use Spiral\RoadRunner\GRPC\Exception\InvalidArgumentException;

final class CatalogService implements CatalogServiceInterface
{
    public function GetProduct(ContextInterface $ctx, GetProductRequest $in): Product
    {
        if ($in->getId() === '') {
            throw new InvalidArgumentException('id vazio');
        }
        // ... buscar no banco via Laravel
        return new Product([
            'id' => $in->getId(),
            'name' => 'Café',
            'price_minor' => 1999,
            'currency' => 'BRL',
            'status' => Status::STATUS_ACTIVE,
        ]);
    }

    // streaming: assinaturas geradas com ServerStreaming/ClientStreaming interfaces
}
```

> RoadRunner fornece o servidor HTTP/2/gRPC e mantém o Laravel "quente" entre
> requests (sem bootstrap por requisição), o que resolve o custo de canal.

### Alternativa com `ext-grpc` (`Grpc\RpcServer`)

```php
$server = new \Grpc\RpcServer();
$server->addHttp2Port('0.0.0.0:50051');
$server->handle(new CatalogServer());
$server->run();
```

Adequado para serviços simples; RoadRunner escala melhor com Laravel.

## 7. Laravel 12: filas, eventos e Octane

- **Filas (Horizon)**: envie DTOs serializáveis; nunca passe mensagens protobuf
  cruas. Serialize com `serializeToString()` e converta para DTO.
- **Eventos**: `ProductFetched`, `ProductSyncFailed` para desacoplar.
- **Octane**: rode o app em RoadRunner/Swoole/FrankenPHP para reutilizar
  conexões gRPC e reduzir latência. Cuidado com estado em memória entre
  requests (singletons precisam ser stateless ou resetados).
- **Config cache**: `config('grpc.*')` funciona com `php artisan config:cache`.

## 8. Segurança

- **TLS/mTLS** sempre em produção (`createSsl`, `rr.yaml` tls).
- Tokens em metadata (via `update_metadata`); nunca no payload nem no log.
- Valide entradas com Form Requests/`protovalidate` e checagens de faixa.
- Deadlines em toda chamada (`timeout`).
- Limite `max_receive_message_length` no RoadRunner/servidor.
- Reflection gRPC apenas em ambientes internos.
- Não vaze detalhes de erro (`details` internos) para clientes.
- Use `readonly`/enums para reduzir estados inválidos.

## 9. Logs e observabilidade

```php
use Illuminate\Support\Facades\Log;

$start = hrtime(true);
try {
    $product = $this->client->getProduct($id);
    Log::info('grpc.getProduct', [
        'id' => $id,
        'duration_ms' => (int) ((hrtime(true) - $start) / 1_000_000),
    ]);
} catch (\Throwable $e) {
    Log::error('grpc.getProduct.failed', ['id' => $id, 'error' => $e->getMessage()]);
    throw $e;
}
```

- Canais Monolog estruturados (`config/logging.php`), JSON em produção.
- Nunca logue `$request->serializeToString()`/payload.
- OpenTelemetry (`open-telemetry/opentelemetry-php`) com propagação
  `traceparent` em metadata.
- Middleware para request ID; correlacione logs e spans.

## 10. Performance

- **Octane/RoadRunner**: elimina o bootstrap por request e reutiliza canais.
- `ext-protobuf` (C) em vez do runtime puro-PHP.
- Reutilize clientes via singleton.
- Streaming para volumes grandes; paginação.
- Deadline agressivo + retry com backoff apenas para códigos transientes.
- Evite converter proto↔DTO em loops quentes de forma ingênua.
- Compressão gRPC quando o payload justificar.
- Cacheie respostas imutáveis.

## 11. Testes (Pest/PHPUnit)

```php
use App\Domain\Catalog\Product;
use App\Domain\Catalog\Status;

it('faz round-trip de produto', function () {
    $p = new Product('p-1', 'Café', 1999, 'BRL', Status::Active);
    $bytes = $p->toProto()->serializeToString();

    $proto = new \Acme\Catalog\V1\Product();
    $proto->mergeFromString($bytes);

    expect(Product::fromProto($proto))->toEqual($p);
});

it('mapeia NOT_FOUND para 404', function () {
    // mock do CatalogClient retornando status code
});
```

- Mock o `CatalogClient` via container (`$this->mock(...)`/`instance`).
- Teste o serviço RoadRunner chamando métodos diretamente.
- Teste erros e streaming.
- `buf breaking` e `buf lint` no CI.

## 12. Pegadinhas

- `google/protobuf` 4.x exige PHP ≥8.1; não misture com a linha 3.x.
- Enums gerados são **constantes de classe**; converta para enums nativos nos
  DTOs.
- `mergeFromString` exige que o descritor em `GPBMetadata` esteja no autoload.
- `oneof`/`optional`: cheque o getter/case antes de usar.
- Em Octane, não guarde estado mutável em singletons entre requests.
- RoadRunner exige reiniciar o worker ao mudar o `.proto`/gencode.
- `createInsecure()` nunca em produção.
- Cuidado com `array_find`/lazy objects (8.4) em código que precisa rodar
  também em versões anteriores, se o projeto tiver mais de um alvo.
- Mensagens PHP não têm redação nativa; selecione campos ao logar.

## 13. Referências

- `protobuf_docs/php/README.md`, `php/REFCOUNTING.md`.
- `grpc_docs/examples/php/*`, `grpc_docs/composer.json`.
- `grpc_docs/doc/statuscodes.md`, `keepalive.md`, `compression.md`,
  `health-checking.md`, `server-reflection.md`.
- Guia do repositório: `languages/php84_guide`, `frameworks/laravel55_guide`
  (para contraste), `runtimes`/Octane.
