# 04 — TypeScript: protobuf + gRPC/Connect

## 1. Escolha de bibliotecas

| Necessidade | Pacote |
|---|---|
| Mensagens (moderno) | `@bufbuild/protobuf` (Protobuf-ES v2) |
| Codegen | `@bufbuild/protoc-gen-es` |
| RPC (recomendado) | `@connectrpc/connect` + `@connectrpc/connect-node` / `connect-web` |
| Codegen RPC | `@connectrpc/protoc-gen-connect-es` |
| Alternativa gRPC clássico | `@grpc/grpc-js` + `ts-proto`/`grpc-tools` |
| Validação | `@bufbuild/protovalidate` |

**Recomendação:** Connect + Protobuf-ES. Funciona em Node, Bun, Deno e navegador,
sobre HTTP/1.1 (JSON/binary) e HTTP/2 (gRPC), com a mesma API. É o caminho mais
simples para full-stack TS.

> Protobuf-ES v2 mudou a API em relação à v1 (`create`, `toBinary`, `fromBinary`,
> schemas por `file_*.ts`). Use a documentação da v2.

## 2. Setup

```bash
npm init -y
npm i @bufbuild/protobuf @connectrpc/connect
npm i -D @bufbuild/protoc-gen-es @connectrpc/protoc-gen-connect-es typescript tsx vitest
```

```yaml
# buf.gen.yaml
version: v2
plugins:
  - local: protoc-gen-es
    out: src/gen
    opt: target=ts
  - local: protoc-gen-connect-es
    out: src/gen
    opt: target=ts
```

```bash
buf generate
```

Gera `src/gen/acme/catalog/v1/catalog_pb.ts` e `catalog_connect.ts` (ou
`*_pb.ts` + `*_connect.ts` conforme plugin).

## 3. Mensagens (Protobuf-ES v2)

```ts
import { create, toBinary, fromBinary, toJson, fromJson } from "@bufbuild/protobuf";
import { ProductSchema, Status } from "./gen/acme/catalog/v1/catalog_pb.js";

const product = create(ProductSchema, {
  id: "p-1",
  name: "Café",
  priceMinor: 1999n,          // bigint para int64
  currency: "BRL",
  status: Status.ACTIVE,
});

const bytes = toBinary(ProductSchema, product);
const parsed = fromBinary(ProductSchema, bytes);
const json = toJson(ProductSchema, product);
```

Observações:
- `int64`/`uint64`/`fixed64` → `bigint` (não `number`).
- Campos `optional` são `undefined` quando ausentes.
- `oneof` usa um campo discriminante (`case`).
- Enums são objetos const com valores numéricos.

## 4. Servidor (Node) com Connect

```ts
import { fastify } from "fastify";
import { connectNodeAdapter } from "@connectrpc/connect-node";
import { ConnectError, Code } from "@connectrpc/connect";
import type { ConnectRouter } from "@connectrpc/connect";
import { CatalogService } from "./gen/acme/catalog/v1/catalog_connect.js";
import { ProductSchema, Status } from "./gen/acme/catalog/v1/catalog_pb.js";
import { create } from "@bufbuild/protobuf";

function routes(router: ConnectRouter) {
  router.service(CatalogService, {
    async getProduct(req) {
      if (!req.id) throw new ConnectError("id vazio", Code.InvalidArgument);
      return create(ProductSchema, {
        id: req.id, name: "Café", priceMinor: 1999n, currency: "BRL",
        status: Status.ACTIVE,
      });
    },
    async *watchProducts(req, ctx) {
      for (let i = 0; i < 3; i++) {
        if (ctx.signal.aborted) return;
        yield create(ProductSchema, { id: `p${i}` });
        await new Promise((r) => setTimeout(r, 100));
      }
    },
    async uploadProducts(reqs) {
      let accepted = 0, rejected = 0;
      for await (const p of reqs) p.id ? accepted++ : rejected++;
      return { accepted, rejected };
    },
  });
}

const app = fastify();
app.register(connectNodeAdapter({ routes }));
await app.listen({ port: 8080, host: "0.0.0.0" });
```

Alternativa HTTP nativa:

```ts
import { createServer } from "node:http";
import { connectNodeAdapter } from "@connectrpc/connect-node";

createServer(connectNodeAdapter({ routes })).listen(8080);
```

## 5. Cliente (Node / TS)

```ts
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-node";
import { CatalogService } from "./gen/acme/catalog/v1/catalog_connect.js";

const transport = createConnectTransport({
  baseUrl: "https://api.acme.com",
  httpVersion: "2",             // ou "1.1" para ambientes sem HTTP/2
  interceptors: [
    (next) => async (req) => {
      req.header.set("authorization", `Bearer ${token}`);
      return next(req);
    },
  ],
});

const client = createClient(CatalogService, transport);

const product = await client.getProduct({ id: "p-1" });

for await (const p of client.watchProducts({ filter: "active" })) {
  console.log(p.id);
}
```

## 6. Cliente no navegador (Connect-Web)

```ts
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import { CatalogService } from "./gen/.../catalog_connect.js";

const transport = createConnectTransport({ baseUrl: "/api" });
const client = createClient(CatalogService, transport);

const product = await client.getProduct({ id: "p-1" });
```

Use `createGrpcWebTransport` do `@connectrpc/connect-web` se precisar falar com
backend gRPC-Web.

## 7. Alternativa: gRPC clássico (`@grpc/grpc-js` + ts-proto)

```bash
npm i @grpc/grpc-js @grpc/proto-loader
npm i -D ts-proto protoc
```

```bash
protoc -I proto \
  --plugin=./node_modules/.bin/protoc-gen-ts_proto \
  --ts_proto_out=src/gen \
  --ts_proto_opt=outputServices=grpc-js,esModuleInterop=true,useExactTypes=false \
  proto/acme/catalog/v1/catalog.proto
```

```ts
import * as grpc from "@grpc/grpc-js";
import { CatalogServiceClient } from "./gen/catalog";
import { credentials } from "@grpc/grpc-js";

const client = new CatalogServiceClient("localhost:50051",
  credentials.createInsecure());

client.getProduct({ id: "p-1" }, new grpc.Metadata(), (err, res) => {
  if (err) return console.error(err.code, err.details);
  console.log(res.name);
});
```

Prefira Connect/Protobuf-ES para novos projetos (melhor DX e portabilidade).

## 8. Segurança

- TLS: `createConnectTransport({ baseUrl: "https://..." })`; nunca `http://` em
  produção (exceto `localhost`).
- Tokens via interceptor (header `authorization`).
- Valide entradas no servidor (`protovalidate` ou checagem manual).
- Defina limites de tamanho no servidor; rejeite payloads grandes.
- CORS restritivo quando o cliente é navegador.
- Não confie em `bigint`/enums vindos do cliente sem validação de faixa.

## 9. Logs e observabilidade

```ts
import { createClient, ConnectError, Code } from "@connectrpc/connect";

const loggingInterceptor = (next) => async (req) => {
  const start = performance.now();
  try {
    const res = await next(req);
    console.info("rpc", { method: req.method.name, ms: performance.now() - start });
    return res;
  } catch (e) {
    const err = ConnectError.from(e);
    console.warn("rpc erro", { method: req.method.name, code: err.code });
    throw err;
  }
};
```

- OpenTelemetry: `@opentelemetry/instrumentation-grpc` ou instrumentação Connect.
- Nunca logue mensagens cruas com PII; registre `method`, `code`, `duration`.
- Use `ConnectError`/`Code` para mapear status.

## 10. Performance

- Reutilize `transport`/`client` (não crie por chamada).
- `bigint` para 64 bits evita perda de precisão.
- Binary (`toBinary`) é mais compacto que JSON; use conforme compatibilidade.
- Streaming para grandes volumes.
- Code splitting no browser: importe só os serviços usados.
- Evite `toJson`/`fromJson` em caminho quente (mais lento que binário).
- `compression: true` no `createConnectTransport` (gzip) quando aplicável.

## 11. Testes (Vitest)

```ts
import { describe, it, expect } from "vitest";
import { create, toBinary, fromBinary } from "@bufbuild/protobuf";
import { ProductSchema } from "../src/gen/acme/catalog/v1/catalog_pb.js";

describe("Product", () => {
  it("roundtrip binário", () => {
    const p = create(ProductSchema, { id: "x", priceMinor: 10n });
    const p2 = fromBinary(ProductSchema, toBinary(ProductSchema, p));
    expect(p2).toEqual(p);
  });
});
```

- Teste serviços com `connectNodeAdapter` em servidor efêmero e client Connect.
- Teste streaming (unary, server, client, bidi) e erros.
- `buf breaking` no CI.
- Property-based/fuzzing do `fromBinary`.

## 12. Pegadinhas

- v1 → v2 do Protobuf-ES: API incompatível; migre mensagens e schemas.
- Importar `*_connect.js` sem extensão `.js` no ESM/NodeNext gera erro.
- `int64` como `number` trunca; use `bigint`.
- `optional` é `undefined`, não `null`.
- `oneof` exige tratar o discriminante.
- Enums no wire são números; valide a faixa.
- `createConnectTransport` precisa de `httpVersion` correto em Node.
- CORS bloqueia browser sem cabeçalhos adequados.
- Não misture `@grpc/grpc-js` e Connect no mesmo client sem adapter.

## 13. Referências

- `protobuf_docs` (formato), `grpc_docs/doc/PROTOCOL-HTTP2.md`.
- Conectividade no browser: `grpc_docs/doc/PROTOCOL-WEB.md`.
