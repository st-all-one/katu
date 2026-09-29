# 02 — Protocolo gRPC: HTTP/2 e Web

> Fonte: `grpc_docs/doc/PROTOCOL-HTTP2.md`, `PROTOCOL-WEB.md`,
> `http-grpc-status-mapping.md`.

## 1. Visão geral

Uma chamada gRPC é um **stream HTTP/2**:

```
Requisição : Request-Headers  *Length-Prefixed-Message  EOS
Resposta   : ( Response-Headers  *Length-Prefixed-Message  Trailers )
             / Trailers-Only
```

- Requisição começa com **Call Header** + **Initial Metadata** + 0..N mensagens.
- Fim da requisição: `END_STREAM` no último frame DATA.
- Resposta: **Initial Metadata** + 0..N mensagens + **Status** + **Trailing
  Metadata** (trailers).
- `Trailers-Only`: quando não há mensagens (ex.: erro imediato), status vem nos
  headers iniciais.

## 2. Headers de requisição

| Header | Obrigatório | Valor |
|---|---|---|
| `:method` | ✅ | `POST` |
| `:scheme` | ✅ | `http` / `https` |
| `:path` | ✅ | `/pacote.Serviço/Método` |
| `:authority` | ✅ | host virtual |
| `te` | ✅ | `trailers` |
| `content-type` | ✅ | `application/grpc[+proto\|+json\|custom]` |
| `grpc-timeout` | opcional | inteiro + unidade (`H,M,S,m,u,n`) |
| `grpc-encoding` | opcional | `identity`,`gzip`,`deflate`,`snappy`,custom |
| `grpc-accept-encoding` | opcional | lista de codecs aceitos |
| `user-agent` | opcional | string estruturada |
| `grpc-message-type` | opcional | nome do tipo na mensagem |
| metadata custom | opcional | ASCII ou `-bin` (base64) |

## 3. Frames de mensagem

Cada mensagem é um frame **length-prefixed** (dentro do corpo HTTP/2):

```
+---------------+----------------+---------------------------+
| Flag (1 byte) | Length (4 BE)  | Message (Length bytes)    |
+---------------+----------------+---------------------------+
```

- `Flag`: bit 0 = *compressed* (a mensagem usa o codec de `grpc-encoding`);
  demais bits reservados.
- `Length`: tamanho em **big-endian** de 4 bytes.
- Requisições/respostas podem ter várias mensagens (streaming).
- Limites de tamanho aplicam-se por mensagem.

## 4. Metadata

- Pares chave/valor como headers/trailers HTTP/2.
- Chaves: ASCII minúsculas, `[0-9a-z_.-]`; valores ASCII imprimíveis.
- Chaves **`-bin`**: valor é base64 (metadata binária).
- Reservado: qualquer chave `grpc-*` é do protocolo; não use para custom.
- `:authority`, `grpc-timeout`, `grpc-status`, `grpc-message` são especiais.
- Limites práticos: header HTTP/2 ≤ 16 KiB por frame/lista (varia por
  implementação); não coloque payloads grandes em metadata.

## 5. Trailers (status)

Depois das mensagens de resposta, o servidor envia trailers:

| Trailer | Descrição |
|---|---|
| `grpc-status` | código numérico (0 = OK) |
| `grpc-message` | mensagem (percent-encoded; não vaza detalhes internos) |
| trailing metadata | pares chave/valor adicionais |

Em `Trailers-Only` (sem mensagem), `grpc-status`/`grpc-message` vêm nos
headers iniciais.

## 6. Mapeamento HTTP → status gRPC

Usado **apenas** quando a resposta **não** contém `grpc-status` (ex.: proxy
devolveu erro HTTP). Se `grpc-status` existir, ele prevalece. Servidores **não**
devem usar esta tabela para escolher o status HTTP.

| HTTP | gRPC |
|---|---|
| 400 Bad Request | `INTERNAL` |
| 401 Unauthorized | `UNAUTHENTICATED` |
| 403 Forbidden | `PERMISSION_DENIED` |
| 404 Not Found | `UNIMPLEMENTED` |
| 429 Too Many Requests | `UNAVAILABLE` |
| 502 Bad Gateway | `UNAVAILABLE` |
| 503 Service Unavailable | `UNAVAILABLE` |
| 504 Gateway Timeout | `UNAVAILABLE` |
| outros | `UNKNOWN` |

> 200 vira `UNKNOWN` porque, em sucesso real, deve haver `grpc-status`.

## 7. Flow control

- Herdado do HTTP/2: janelas de fluxo por stream e por conexão.
- Controla buffering de mensagens; evita estouro de memória.
- Backpressure natural: produtor bloqueia quando a janela zera.
- Ajuste de janela (`initial_window_size`, `connection_window_size`) para
  throughput alto.

## 8. Conexões, keepalive e GOAWAY

- Canais HTTP/2 multiplexam chamadas concorrentes.
- **PING** (keepalive) detecta conexões mortas; `GOAWAY` encerra a conexão
  (servidor pode drenar streams ativos).
- `GOAWAY with "too_many_pings"` indica keepalive agressivo do cliente.
- Após GOAWAY, o canal re-resolve e reconecta.

Ver `10-compressao-keepalive-backoff.md`.

## 9. gRPC-Web

No navegador não há HTTP/2 cru com trailers. O **gRPC-Web** adapta o protocolo:

- `content-type: application/grpc-web+proto` (ou `+text`).
- Mensagens em frames **length-prefixed**; a resposta inclui um **trailer frame**
  especial (bit 7 do flag) com `grpc-status`/`grpc-message` codificados em ASCII
  no corpo (não em trailers HTTP).
- Base64 opcional (`application/grpc-web-text`) para compatibilidade.
- Streaming suportado: **unary + server-streaming**. Client-streaming e bidi
  não são suportados pelo transporte do navegador.
- Requer **proxy/transcoding**: Envoy (filtro gRPC-Web), grpc-gateway, Connect,
  ou servidor que fale gRPC-Web.
- CORS e TLS são obrigatórios.

Alternativa recomendada: **Connect** (`@connectrpc/connect-web`), que usa
`fetch` e funciona com HTTP/1.1 e HTTP/2, sem proxy dedicado.

## 10. Connect

- Protocolo RPC que fala gRPC, gRPC-Web e um modo próprio sobre HTTP/1.1/2.
- `POST /pacote.Serviço/Método`, `content-type` binário (`application/proto`) ou
  JSON; erros como JSON `{code, message, details}`.
- Suporta unary e server-streaming no navegador; bidi só em ambientes
  full-duplex.
- Mesma API no servidor e no cliente; ótimo para full-stack TS.

## 11. Segurança de transporte

- **ALPN** negocia `h2` no TLS.
- `:authority` valida host; use SNI/certificados corretos.
- Proxies/load balancers precisam suportar HTTP/2 fim-a-fim ou fazer
  bridging; gRPC sobre HTTP/1.1 só via gRPC-Web/Connect.

## 12. Checklists

**Servidor**
- [ ] `content-type` válido; rejeitar outros.
- [ ] Limite de tamanho e de headers.
- [ ] `grpc-status` em trailers (ou trailers-only).
- [ ] Não vazar detalhes em `grpc-message`.

**Cliente**
- [ ] Deadline (`grpc-timeout`).
- [ ] `grpc-accept-encoding` correto.
- [ ] Tratar `Trailers-Only` e ausência de `grpc-status` (tabela HTTP→gRPC).
- [ ] Reconectar após GOAWAY; keepalive alinhado.

**Navegador**
- [ ] gRPC-Web/Connect com proxy.
- [ ] CORS/TLS.
- [ ] Streaming suportado (sem bidi).
