# HTTP: Fundamentos

## 1. Definição

HTTP (Hypertext Transfer Protocol) é um protocolo de camada de aplicação para transmissão de documentos hipermídia. Opera no modelo **cliente-servidor** e é **stateless** por natureza.

> "HTTP is an application-layer protocol for transmitting hypermedia documents, such as HTML. It was designed for communication between web browsers and web servers, but it can also be used for other purposes, such as machine-to-machine communication, programmatic access to APIs, and more."

## 2. Estrutura de Mensagens

Toda mensagem HTTP (request ou response) tem 4 partes:

```
┌──────────────────────┐
│   Start-line         │  ← request-line ou status-line
├──────────────────────┤
│   Headers            │  ← metadados (opcionais)
├──────────────────────┤
│   Empty line         │  ← CRLF (\r\n) — delimita headers do body
├──────────────────────┤
│   Body               │  ← dados (opcional)
└──────────────────────┘
```

### Request

```
GET /path HTTP/1.1
Host: example.com
Accept: text/html
User-Agent: Mozilla/5.0
```

**Formas de request-target**:
- **Origin form**: `GET /path` (uso padrão)
- **Absolute form**: `GET http://example.com/path` (para proxies)
- **Authority form**: `host:port` (apenas CONNECT)
- **Asterisk form**: `*` (apenas OPTIONS)

### Response

```
HTTP/1.1 200 OK
Content-Type: text/html
Content-Length: 1234
Cache-Control: max-age=3600

<html>...
```

## 3. Métodos HTTP

9 métodos definidos, com propriedades semânticas:

| Método | SAFE | Idempotent | Cacheable | Body Request | Body Response | Semântica |
|--------|------|-----------|-----------|-------------|--------------|-----------|
| **GET** | ✅ | ✅ | ✅ | ❌ | ✅ | Recuperar representação do recurso |
| **HEAD** | ✅ | ✅ | ✅ | ❌ | ❌ | Como GET, sem body |
| **OPTIONS** | ✅ | ✅ | ❌ | ❌ | ✅ | Descrever opções de comunicação |
| **TRACE** | ✅ | ✅ | ❌ | ❌ | ✅ | Loop-back test |
| **PUT** | ❌ | ✅ | ❌ | ✅ | ✅ | Substituir representação do recurso |
| **DELETE** | ❌ | ✅ | ❌ | ❌ | ✅ | Remover recurso |
| **POST** | ❌ | ❌ | ⚠️ | ✅ | ✅ | Submeter entidade, causar mudança de estado |
| **PATCH** | ❌ | ❌ | ⚠️ | ✅ | ✅ | Modificação parcial do recurso |
| **CONNECT** | ❌ | ❌ | ❌ | ❌ | ✅ | Estabelecer túnel TCP |

> ⚠️ POST e PATCH são cacheáveis APENAS quando a resposta inclui informação de freshness e header `Content-Location`.

**Safe**: não altera estado do servidor (GET, HEAD, OPTIONS, TRACE).
**Idempotent**: múltiplas requisições idênticas produzem o mesmo efeito (todos exceto POST e PATCH).

## 4. Status Codes

### 1xx — Informacionais

| Code | Significado | Uso |
|------|------------|-----|
| 100 | Continue | Cliente deve continuar enviando body |
| 101 | Switching Protocols | Upgrade de protocolo (WebSocket) |
| 103 | Early Hints | Preload/preconnect antes da resposta final |

### 2xx — Sucesso

| Code | Significado | Uso |
|------|------------|-----|
| 200 | OK | Sucesso genérico |
| 201 | Created | Recurso criado (POST/PUT) |
| 202 | Accepted | Requisição aceita para processamento assíncrono |
| 204 | No Content | Sucesso sem body |
| 206 | Partial Content | Range request bem-sucedido |
| 226 | IM Used | Delta encoding (RFC 3229) |

### 3xx — Redirecionamento

| Code | Significado | Preserva Método? | Cacheable? |
|------|------------|-----------------|------------|
| 301 | Moved Permanently | ❌ (pode mudar para GET) | ✅ |
| 302 | Found | ❌ (pode mudar para GET) | ❌ (default) |
| 303 | See Other | ❌ (força GET) | ❌ |
| 304 | Not Modified | — | ✅ |
| 307 | Temporary Redirect | ✅ | ❌ |
| 308 | Permanent Redirect | ✅ | ✅ |

### 4xx — Erro do Cliente

| Code | Significado | Notas |
|------|------------|-------|
| 400 | Bad Request | Erro de sintaxe no request |
| 401 | Unauthorized | Autenticação necessária |
| 403 | Forbidden | Autenticado mas sem permissão |
| 404 | Not Found | Recurso não encontrado |
| 405 | Method Not Allowed | Método não suportado |
| 406 | Not Acceptable | Negociação de conteúdo sem match |
| 408 | Request Timeout | Conexão ociosa |
| 409 | Conflict | Estado atual conflita com request |
| 410 | Gone | Recurso removido permanentemente |
| 412 | Precondition Failed | Conditional request falhou |
| 415 | Unsupported Media Type | Content-Type não suportado |
| 416 | Range Not Satisfiable | Range inválido |
| 418 | I'm a Teapot | HTCPCP (April Fools RFC 2324) |
| 422 | Unprocessable Content | Semântica inválida (WebDAV) |
| 429 | Too Many Requests | Rate limiting |
| 451 | Unavailable For Legal Reasons | Censura/governo |

### 5xx — Erro do Servidor

| Code | Significado | Notas |
|------|------------|-------|
| 500 | Internal Server Error | Erro genérico do servidor |
| 501 | Not Implemented | Método não suportado pelo servidor |
| 502 | Bad Gateway | Gateway/proxy recebeu resposta inválida |
| 503 | Service Unavailable | Sobrecarga/manutenção |
| 504 | Gateway Timeout | Gateway sem resposta a tempo |
| 505 | HTTP Version Not Supported | Versão HTTP não suportada |

## 5. Sessão HTTP

Uma sessão HTTP tem 3 fases:

1. **Estabelecimento de conexão** (TCP, porta 80/443)
2. **Requisição**: método + path + versão → headers → body
3. **Resposta**: status line + headers + body

> "As of HTTP/1.1, the connection is no longer closed after completing the third phase, and the client is now granted a further request."

**Stateless**: cada par request-response é independente. Estado é adicionado via cookies, tokens, ou outros mecanismos.

## 6. Headers por Categoria

### Autenticação e Identidade
- `WWW-Authenticate`, `Authorization`, `Proxy-Authenticate`, `Proxy-Authorization`
- `Cookie`, `Set-Cookie`

### Cache
- `Cache-Control`, `Expires`, `Pragma`, `Age`
- `ETag`, `Last-Modified`, `If-None-Match`, `If-Modified-Since`
- `Vary`

### CORS
- `Origin`, `Access-Control-Allow-Origin`, `Access-Control-Allow-Methods`
- `Access-Control-Allow-Headers`, `Access-Control-Max-Age`, `Access-Control-Allow-Credentials`
- `Access-Control-Expose-Headers`, `Access-Control-Request-Method`, `Access-Control-Request-Headers`

### Segurança
- `Content-Security-Policy`, `Content-Security-Policy-Report-Only`
- `Cross-Origin-Resource-Policy`, `Cross-Origin-Embedder-Policy`, `Cross-Origin-Opener-Policy`
- `Strict-Transport-Security`, `X-Content-Type-Options`, `X-Frame-Options`
- `Permissions-Policy`, `Referrer-Policy`

### Conteúdo e Negociação
- `Content-Type`, `Content-Length`, `Content-Encoding`, `Content-Language`, `Content-Location`
- `Accept`, `Accept-Encoding`, `Accept-Language`, `Accept-CH`
- `Transfer-Encoding`

### Condicionais e Range
- `If-Match`, `If-None-Match`, `If-Modified-Since`, `If-Unmodified-Since`, `If-Range`
- `Range`, `Accept-Ranges`, `Content-Range`

### Fetch Metadata (Sec-*)
- `Sec-Fetch-Site`, `Sec-Fetch-Mode`, `Sec-Fetch-User`, `Sec-Fetch-Dest`
- `Sec-CH-UA`, `Sec-CH-UA-Platform`, `Sec-CH-UA-Mobile`

### Conexão e Upgrade
- `Connection`, `Upgrade`, `Keep-Alive`
- `Sec-WebSocket-Key`, `Sec-WebSocket-Accept`, `Sec-WebSocket-Version`
