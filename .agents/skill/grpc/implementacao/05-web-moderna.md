# 05 — Web moderna (navegador): protobuf + gRPC

O navegador **não** fala gRPC nativo (HTTP/2 com trailers não é exposto ao
JavaScript). As opções são:

| Abordagem | Transporte | Streaming | Observações |
|---|---|---|---|
| **Connect-Web** | `fetch` (HTTP/1.1 ou /2) + JSON/binário | unary + server-stream | Recomendado; mesma API do servidor |
| **gRPC-Web** | `fetch`/XHR | unary + server-stream | Requer proxy (Envoy/gateway) |
| **grpc-gateway / REST** | HTTP/JSON | polling/SSE | Interop máxima |
| **WebTransport/WebSocket** | próprio | bidi | Alternativa para tempo real |

**Recomendação:** Connect + Protobuf-ES (`@connectrpc/connect-web`). Funciona com
`fetch`, sem proxy especial, e é compatível com servidores Connect/gRPC.

## 1. Dependências

```bash
npm i @bufbuild/protobuf @connectrpc/connect @connectrpc/connect-web
npm i -D @bufbuild/protoc-gen-es @connectrpc/protoc-gen-connect-es
```

Geração via `buf generate` (ver `04-typescript.md`) com `target=ts`.

## 2. Cliente no navegador

```ts
import { createClient } from "@connectrpc/connect";
import { createConnectTransport } from "@connectrpc/connect-web";
import { CatalogService } from "./gen/acme/catalog/v1/catalog_connect.js";

const transport = createConnectTransport({
  baseUrl: "/api",              // proxy no mesmo domínio evita CORS
  useBinaryFormat: true,        // binário (compacto) ou false p/ JSON
  interceptors: [
    (next) => async (req) => {
      req.header.set("authorization", `Bearer ${await getToken()}`);
      return next(req);
    },
  ],
  // compressão:
  // compression: "gzip" (quando suportado)
});

export const catalog = createClient(CatalogService, transport);
```

Chamadas:

```ts
const product = await catalog.getProduct({ id: "p-1" });

// server-streaming
const ac = new AbortController();
for await (const p of catalog.watchProducts({ filter: "active" }, { signal: ac.signal })) {
  render(p);
  if (done) ac.abort();
}
```

## 3. Streaming no navegador

| Tipo | Suporte no browser |
|---|---|
| Unary | ✅ |
| Server-streaming | ✅ (fetch streaming / gRPC-Web) |
| Client-streaming | ⚠️ limitado (gRPC-Web não suporta bem) |
| Bidi | ❌ (gRPC-Web); use WebSocket/WebTransport |

Para bidi em tempo real, use WebSocket/WebTransport com mensagens protobuf
serializadas por conta própria, ou aguarde suporte a `fetch` full-duplex.

## 4. Framing e transporte

- **Connect**: usa `POST /<service>/<method>` com `Content-Type`
  `application/proto` (binário) ou `application/json`; erros em JSON com
  `code`/`message`/`details`.
- **gRPC-Web**: `application/grpc-web+proto`; mensagens em frames
  length-prefixed (1 byte flag + 4 bytes tamanho). Requer proxy que traduza
  trailers HTTP/2 para o trailer do gRPC-Web.
- **CORS**: exponha os cabeçalhos de trailer/protocolo necessários e permita o
  método/headers usados. Prefira servir cliente e API no **mesmo domínio** (proxy
  reverso) para evitar CORS.
- Ambos exigem **HTTPS** em produção.

## 5. Integração com bundlers

- **Vite/Rollup/esbuild**: importar os `.ts` gerados; sem configuração especial.
- **Tree-shaking**: importe apenas os serviços usados.
- **WASM/Worker**: protobuf roda em main thread; para parse pesado, mova para
  Web Worker.
- **Code splitting**: `import()` dinâmico por rota para reduzir bundle inicial.

```ts
const { CatalogService } = await import("./gen/.../catalog_connect.js");
```

## 6. Segurança no navegador

- **Nunca** coloque segredos no bundle; tokens obtidos via fluxo de auth
  (cookies `HttpOnly` ou memória).
- **CSRF**: se usar cookies, proteja com `SameSite=Lax/Strict` + token CSRF.
  Prefira `Authorization: Bearer` em memória com refresh rotativo.
- **CSP**: evite `unsafe-eval`; use nonces/hashes; restrinja `connect-src` ao
  seu domínio de API.
- **TLS** obrigatório; HSTS.
- **CORS** restritivo (origens explícitas; nunca `*` com credenciais).
- **Validação** de `bigint`, enums e strings no cliente (defesa em profundidade),
  além do servidor.
- **Redação**: não renderize/logue PII; cuidado com erros que ecoam payloads.

## 7. Logs e observabilidade

```ts
const observability = (next) => async (req) => {
  const start = performance.now();
  try {
    const res = await next(req);
    report({ method: req.method.name, ms: performance.now() - start, code: "OK" });
    return res;
  } catch (e) {
    const err = ConnectError.from(e);
    report({ method: req.method.name, code: err.code });
    throw e;
  }
};
```

- Envie eventos para backend (OTel collector) — não logue payload.
- Correlacione com `traceparent`/`x-request-id`.
- Core Web Vitals não devem ser afetados por chamadas RPC bloqueantes; use
  indicadores de carregamento e não bloqueie o main thread.

## 8. Performance

- Prefira `useBinaryFormat: true` (menor payload); use JSON para depuração.
- Reutilize `transport`/`client` (singleton).
- Cache de respostas quando aplicável (`Cache-Control`) — RPC normalmente é
  `no-store`; cache só de dados públicos.
- Streaming para feeds; evite polling agressivo.
- Web Worker para parse/transformação pesada.
- Compressão (`gzip`) para payloads grandes; meça CPU vs banda.
- Lazy-load de serviços por rota; monitore bundle size.
- Conexões HTTP/2 multiplexadas reaproveitam o canal.

## 9. Service Worker e offline

- Intercepte chamadas para cache de assets e respostas públicas.
- Fila de requisições para retry em reconexão (Background Sync), com cuidado
  para não reenviar mutações não idempotentes.
- Nunca cacheie dados sensíveis sem controle.

## 10. Testes

- **Unit**: mensagens (`toBinary`/`fromBinary`), interceptors.
- **Component**: mock do transport (`createConnectTransport` → handler mock) ou
  MSW.
- **E2E**: Playwright (ver guia de Playwright) com API real ou stub.
- **Contrato**: `buf breaking` no CI.

```ts
import { ConnectError, Code } from "@connectrpc/connect";
import { createRouterTransport } from "@connectrpc/connect";

const mockTransport = createRouterTransport(({ service }) => {
  service(CatalogService, { getProduct: () => ({ id: "p-1" }) });
});
```

## 11. Pegadinhas

- gRPC binário nativo não funciona no browser — use Connect-Web/gRPC-Web.
- Bidi/client-streaming não são suportados pelo gRPC-Web.
- CORS: erros de trailer/headers costumam quebrar o streaming; verifique
  `Access-Control-Expose-Headers`.
- docker/proxy precisa suportar HTTP/2 para gRPC-Web (Envoy com
  `grpc_web` filter) ou HTTP/1.1 para Connect.
- `bigint` quebra `JSON.stringify` — converta para string ao serializar em JSON.
- Não exponha tokens em `localStorage` (XSS); prefira memória + cookie HttpOnly.
- `performance.now()` não é relógio de parede; para tracing use `traceparent`.

## 12. Referências

- `grpc_docs/doc/PROTOCOL-WEB.md` (gRPC-Web).
- `grpc_docs/doc/PROTOCOL-HTTP2.md` (transporte).
- `grpc_docs/doc/statuscodes.md` (mapeamento de erros).
- Guias do repositório: `web/web_security_guide`, `web/web_performance_guide`,
  `protocols/http_uri_guide`.
