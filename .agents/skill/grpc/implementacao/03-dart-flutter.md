# 03 — Flutter / Dart: protobuf + gRPC

## 1. Pacotes

| Necessidade | Pacote pub.dev |
|---|---|
| Mensagens | `protobuf` (+ `fixnum` para `Int64`) |
| gRPC (VM/mobile/desktop) | `grpc` |
| gRPC-Web (browser) | `grpc` (canal `GrpcWebClientChannel`) |
| Codegen | `protoc_plugin` (`protoc-gen-dart`) |
| Serialização JSON | gerado (`*.pbjson.dart`) ou `protobuf` `json` |

```yaml
# pubspec.yaml
dependencies:
  protobuf: ^4.0.0
  fixnum: ^1.1.0
  grpc: ^4.0.0

dev_dependencies:
  protoc_plugin: ^22.0.0   # versão alinhada ao protoc
```

```bash
dart pub global activate protoc_plugin
export PATH="$PATH:$HOME/.pub-cache/bin"
```

> O `protoc-gen-dart` deve ser gerado por um `protoc` compatível. O runtime
> `protobuf` e o `protoc_plugin` devem estar em versões compatíveis — verifique
> a matriz oficial.

## 2. Geração de código

```bash
protoc -I proto \
  --dart_out=grpc:lib/src/generated \
  proto/acme/catalog/v1/catalog.proto
```

Gera:
- `catalog.pb.dart` — mensagens.
- `catalog.pbenum.dart` — enums.
- `catalog.pbjson.dart` — descritores/JSON.
- `catalog.pbgrpc.dart` — cliente e serviço gRPC.

Com `buf`:

```yaml
# buf.gen.yaml
version: v2
plugins:
  - local: protoc-gen-dart
    out: lib/src/generated
    opt: grpc
```

## 3. Cliente (Flutter / Dart)

```dart
import 'package:grpc/grpc.dart';
import 'package:fixnum/fixnum.dart';
import 'src/generated/catalog.pbgrpc.dart';

class CatalogApi {
  late final ClientChannel _channel;
  late final CatalogServiceClient _client;

  void connect(String host, {bool secure = true}) {
    _channel = ClientChannel(
      host,
      port: 443,
      options: ChannelOptions(
        credentials: secure
            ? ChannelCredentials.secure()
            : ChannelCredentials.insecure(),
        connectionTimeout: const Duration(seconds: 5),
        idleTimeout: const Duration(minutes: 5),
      ),
    );
    _client = CatalogServiceClient(_channel);
  }

  Future<Product> getProduct(String id) {
    return _client.getProduct(
      GetProductRequest(id: id),
      options: CallOptions(
        timeout: const Duration(seconds: 5),
        metadata: {'authorization': 'Bearer $token'},
      ),
    );
  }

  Stream<Product> watchProducts(String filter) {
    return _client.watchProducts(WatchProductsRequest(filter: filter));
  }

  Future<void> close() => _channel.shutdown();
}
```

### Integração em widget Flutter

```dart
class ProductView extends StatefulWidget {
  const ProductView({super.key, required this.api, required this.id});
  final CatalogApi api;
  final String id;
  @override
  State<ProductView> createState() => _ProductViewState();
}

class _ProductViewState extends State<ProductView> {
  late Future<Product> _future;

  @override
  void initState() {
    super.initState();
    _future = widget.api.getProduct(widget.id);
  }

  @override
  Widget build(BuildContext context) => FutureBuilder<Product>(
        future: _future,
        builder: (context, snap) {
          if (snap.hasError) return Text('Erro: ${snap.error}');
          if (!snap.hasData) return const CircularProgressIndicator();
          final p = snap.data!;
          // p.priceMinor é Int64 → use .toInt()/.toString()
          return Text('${p.name} — ${p.priceMinor} ${p.currency}');
        },
      );
}
```

### Streaming em Flutter

```dart
StreamBuilder<Product>(
  stream: widget.api.watchProducts('status=active'),
  builder: (context, snap) { /* ... */ },
);
```

Cancele a inscrição no `dispose()` (o `StreamBuilder` cuida disso).

### Interceptor

```dart
class AuthInterceptor extends ClientInterceptor {
  AuthInterceptor(this.token);
  final String token;

  @override
  ResponseFuture<R> interceptUnary<Q, R>(
    ClientMethod<Q, R> method, Q request, CallOptions options, ClientUnaryInvoker<Q, R> invoker) {
    final opts = options.mergedWith(
      CallOptions(metadata: {'authorization': 'Bearer $token'}),
    );
    return invoker(method, request, opts);
  }
}

_client = CatalogServiceClient(_channel, interceptors: [AuthInterceptor(token)]);
```

## 4. Servidor (Dart VM)

```dart
import 'package:grpc/grpc.dart';
import 'src/generated/catalog.pbgrpc.dart';

class CatalogService extends CatalogServiceBase {
  @override
  Future<Product> getProduct(ServiceCall call, GetProductRequest request) async {
    if (request.id.isEmpty) {
      throw GrpcError.invalidArgument('id vazio');
    }
    return Product(
      id: request.id,
      name: 'Café',
      priceMinor: Int64(1999),
      currency: 'BRL',
      status: Status.STATUS_ACTIVE,
    );
  }

  @override
  Stream<Product> watchProducts(ServiceCall call, WatchProductsRequest request) async* {
    for (var i = 0; i < 3; i++) {
      yield Product(id: 'p$i');
      await Future<void>.delayed(const Duration(milliseconds: 100));
    }
  }

  @override
  Future<UploadSummary> uploadProducts(ServiceCall call, Stream<Product> request) async {
    var accepted = 0, rejected = 0;
    await for (final item in request) {
      if (item.id.isEmpty) {
        rejected++;
      } else {
        accepted++;
      }
    }
    return UploadSummary(accepted: accepted, rejected: rejected);
  }
}

Future<void> main() async {
  final server = Server.create(
    services: [CatalogService()],
    codecRegistry: CodecRegistry(codecs: const [GzipCodec(), IdentityCodec()]),
  );
  await server.serve(port: 50051);
  print('servindo em ${server.address}:${server.port}');
}
```

Para TLS no servidor, use `Server.create(services: ..., security: ServerSecurity(...))`
com certificados PEM.

## 5. Web (Flutter Web)

No navegador não há HTTP/2 cru; use **gRPC-Web**:

```dart
import 'package:grpc/grpc_web.dart';

final channel = GrpcWebClientChannel.xhr(Uri.parse('https://api.acme.com'));
final client = CatalogServiceClient(channel);
```

Limitações:
- Streaming só *server-streaming* (client/bidi limitado pelo navegador).
- Requer proxy/backend compatível com gRPC-Web (Envoy, Connect, grpc-gateway).
- CORS e TLS são obrigatórios.

Alternativa recomendada: **Connect** com `@bufbuild/protobuf` (ver `05-web-moderna.md`),
que funciona com `fetch` e suporta unary + server streaming.

## 6. Segurança

- Sempre TLS (`ChannelCredentials.secure()`), nunca `insecure` em produção.
- Credenciais em `metadata`; nunca no payload.
- Valide entradas no servidor; `GrpcError` com `StatusCode.invalidArgument`.
- Limite tamanho de mensagens no servidor (`Server.create` com opções de canal).
- Pinning de certificado no mobile quando aplicável.
- Não embarque segredos no app Flutter (o binário é inspecionável); use tokens
  obtidos em runtime via auth flow.

## 7. Logs e observabilidade

```dart
import 'package:logging/logging.dart';

final _log = Logger('catalog');
...
_log.info('getProduct id=$id');
```

- Nunca logue payloads com PII; registre IDs e status.
- `grpc` expõe `ClientInterceptor` para logging de método/status/duração.
- No servidor, `ServiceCall` tem `clientMetadata` e `cancelled`.
- Métricas/tracing: OpenTelemetry Dart (`opentelemetry`/`dartastic_opentelemetry`)
  e instrumentação de canal.

## 8. Performance

- Reutilize `ClientChannel` e clientes (singleton por app).
- `Int64` (fixnum) para campos de 64 bits; `int` em Dart VM é 64-bit, mas o
  gencode usa `Int64` por portabilidade.
- Evite `toProto`/`fromProto` excessivos em listas grandes.
- Use streaming para volumes grandes; evite mensagens >4 MiB.
- `keepalive`/`idleTimeout` configurados.
- No Flutter, faça chamadas fora do build; use `FutureBuilder`/`StreamBuilder`
  e isolate (`compute`) para parse pesado.
- Bundling: `--dart_out=grpc` gera só o necessário; tree-shaking cuida do resto.

## 9. Testes

```dart
import 'package:test/test.dart';
import 'src/generated/catalog.pb.dart';

void main() {
  test('roundtrip', () {
    final p = Product(id: 'x', priceMinor: Int64(10));
    final bytes = p.writeToBuffer();
    final p2 = Product.fromBuffer(bytes);
    expect(p2.id, 'x');
    expect(p2.priceMinor, Int64(10));
  });
}
```

- Teste serviços no VM subindo `Server` efêmero e conectando com
  `ClientChannel` em `127.0.0.1:0`.
- Teste streaming (completa, cancela, erro).
- Teste `writeToBuffer`/`fromBuffer` e `fromJson`/`toJson`.
- Fuzzing de `fromBuffer` com bytes aleatórios.

## 10. Pegadinhas

- `protoc-gen-dart` desatualizado em relação ao `protoc`/`protobuf` → erros.
- `Int64` não é `int`: use `.toInt()`/`.toString()` e compare com `Int64`.
- Enums gerados são classes com `.value`; use `Status.STATUS_ACTIVE`.
- `oneof` em Dart é um campo `whichX()` + getters; trate o case.
- Flutter Web: `grpc` nativo não funciona; use gRPC-Web/Connect.
- Não recrie o `ClientChannel` a cada chamada (custo de handshake).
- `Timestamp`/`Duration` vêm dos well-known types gerados; converta para
  `DateTime`/`Duration` com helpers.
- Em servidor Dart, não bloqueie o event loop com trabalho CPU-bound; use isolate.

## 11. Referências

- `protobuf_docs/examples/add_person.dart`, `list_people.dart`.
- `protobuf_docs/examples/pubspec.yaml`.
- `grpc_docs/doc/PROTOCOL-WEB.md`, `doc/statuscodes.md`.
