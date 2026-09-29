# 00 — gRPC: visão geral e mapa do guia

> Versão de referência: **gRPC Core 57 / C++ 1.85-dev** (main, 2026).
> Guia denso em pt-BR, otimizado para consumo por IA, baseado no repositório
> oficial `grpc/grpc`.
> Skill irmã: [`../protobuf_guide/`](../protobuf_guide/) — contrato `.proto`,
> wire format, editions e serialização.

## 1. O que é gRPC

**gRPC** é um framework de **RPC** (Remote Procedure Call) de alto desempenho e
código aberto. O usuário descreve um serviço em um `.proto`; `protoc` (com
plugins gRPC) gera stubs de cliente e interfaces de servidor em várias
linguagens. O transporte é **HTTP/2** (e HTTP/3 em implementações modernas); as
mensagens são **protobuf** binário por padrão (o IDL é plugável).

Três camadas:

| Camada | Papel |
|---|---|
| **IDL** (`.proto`) | Define `service`, `rpc`, mensagens de request/response |
| **Codegen** | Gera stub cliente e serviço servidor por linguagem |
| **Core/transporte** | HTTP/2, framing, flow control, status, metadata, TLS |

## 2. Modelo mental

1. Uma chamada RPC = um **stream HTTP/2**.
2. O método é endereçado por `:path = /pacote.Serviço/Método`.
3. **Status** e **trailing metadata** viajam em **trailers** HTTP/2.
4. Mensagens são **frames length-prefixed** (1 byte de flag + 4 bytes de tamanho
   + payload).
5. **Streaming** é natural: 0..N mensagens em cada direção.
6. **Deadline** viaja no header `grpc-timeout` e deve ser propagado.
7. **Metadata** são headers HTTP/2 (chaves minúsculas; sufixo `-bin` = base64).
8. O mesmo `.proto` serve para gerar stubs gRPC **e** para serialização pura
   (ver skill de protobuf).

## 3. Os quatro padrões de RPC

| Padrão | Assinatura | Uso |
|---|---|---|
| **Unary** | `rpc M (Req) returns (Resp)` | RPC clássico |
| **Server streaming** | `rpc M (Req) returns (stream Resp)` | feed, eventos, download |
| **Client streaming** | `rpc M (stream Req) returns (Resp)` | upload, agregação |
| **Bidirectional** | `rpc M (stream Req) returns (stream Resp)` | chat, duplex |

## 4. Quando usar (e quando não)

**Use gRPC quando:**
- Serviços internos precisam de RPC tipado, rápido e multi-linguagem.
- Há streaming (telemetria, eventos, upload).
- Precisa de deadlines, cancelamento, retry e metadata padronizados.
- Quer contrato versionado e codegen entre times.

**Evite/Prefira outra coisa quando:**
- Ambiente sem HTTP/2 nem extensão gRPC (ex.: PHP 7.2) → protobuf sem gRPC
  (HTTP/REST/filas), ver `../protobuf_guide/10-protobuf-sem-grpc.md`.
- API pública consumida por navegadores simples → REST/JSON (ou
  grpc-gateway/Connect para transcoding).
- Necessidade de cache HTTP nativo, URLs, verbos REST → REST.
- Payloads pequenos e esporádicos onde a operação de gRPC não compensa.

## 5. Comparações

| Critério | gRPC | REST/JSON | GraphQL | Connect |
|---|---|---|---|---|
| Contrato | `.proto` forte | OpenAPI opcional | Schema SDL | `.proto` |
| Transporte | HTTP/2 (+/3) | HTTP/1.1+ | HTTP | HTTP/1.1, /2, gRPC |
| Payload | protobuf binário | JSON | JSON | protobuf/JSON |
| Streaming | 4 padrões | SSE/WebSocket | subscriptions | unary + server |
| Navegador | via proxy/gRPC-Web | nativo | nativo | nativo |
| Performance | alta | média | média | alta |
| Tooling | codegen | universal | amplo | codegen |

## 6. Ecossistema

- **Implementações**: grpc-go, grpc-java, grpc-cpp, grpc-python, grpc-csharp,
  grpc-node, grpc-dart, grpc-php, grpc-ruby.
- **Navegador**: gRPC-Web, Connect-Web.
- **Gateway**: grpc-gateway (REST/JSON via `google.api.http`), Envoy.
- **Proxyless/xDS**: controle de tráfego e LB sem sidecar.
- **Ferramentas**: `grpcurl`, `grpc_cli`, `buf`, `ghz` (load), reflection.
- **Observabilidade**: OpenTelemetry, Prometheus, logging binário.

## 7. Como este guia está organizado

| Bloco | Arquivos | Foco |
|---|---|---|
| Fundamentos | `01` | Modelo, quando usar, ecossistema |
| Protocolo | `02` | HTTP/2, framing, gRPC-Web, status mapping |
| Contrato/RPC | `03`, `05`, `06` | Serviços, metadata/deadlines, streaming |
| Erros | `04` | Status codes, erros ricos |
| Middleware | `07` | Interceptors |
| Produção | `08`–`11` | Segurança, reflection/health, compressão/keepalive, LB/xDS |
| Qualidade | `12`–`14` | Observabilidade/performance, testes, operação |
| Apoio | `15` | Referência rápida |
| Implementação | `implementacao/` | Rust, Go, Dart/Flutter, TypeScript, web, PHP 7.2/8.4 |

## 8. Fluxo recomendado para a IA

1. Defina o `.proto` (skill `protobuf`) antes de codar o serviço.
2. Leia `01` (modelo) e `03` (definição de serviço/codegen).
3. Para cada chamada: `05` (deadline/metadata) e `04` (erros).
4. Para streaming: `06`. Para middleware: `07`.
5. Antes de produção: `08` (segurança), `09` (reflection/health),
   `10` (compressão/keepalive), `11` (LB).
6. Para operar/tunar: `12`, `14`. Para testar: `13`.

## 9. Regras de ouro (resumo)

- Sempre **deadline**; retry só em códigos idempotentes com backoff+jitter.
- **TLS/mTLS**; credenciais em metadata, nunca no payload.
- Limites de mensagem e keepalive **alinhados** cliente/servidor.
- Status codes **corretos**; erros ricos em `google.rpc.Status`.
- Logs sem payload bruto; redação de PII.
- Reflection/health restritos em produção.
- Manter stub/plugin e runtime na **mesma versão**.
