# HTTP Moderno: Padrões e Práticas

## 1. Evolução do Protocolo

| Versão | Ano | Base | Características |
|--------|-----|------|----------------|
| HTTP/0.9 | 1991 | TCP | Apenas GET, sem headers, só HTML |
| HTTP/1.0 | 1996 | TCP | Versão, status codes, headers, Content-Type |
| HTTP/1.1 | 1997 | TCP | Conexões persistentes, pipelining, chunked, cache, Host obrigatório |
| HTTP/2 | 2015 | TCP | Binário, multiplexado, compressão HPACK, server push |
| HTTP/3 | 2022 | QUIC/UDP | Elimina HOL do TCP, conexão mais rápida, melhor em redes degradadas |

> "If you understand HTTP/1.1's semantics, you already have a solid foundation for grasping HTTP/2 and HTTP/3. The main difference lies in **how** these semantics are implemented at the transport level."

### HTTP/2 (RFC 9113)

**Binary Framing Layer**: mensagens são divididas em frames binários, multiplexados em streams numeradas.

**Pseudo-headers** (substituem request line e status line):
```
:method     → GET/POST/etc
:scheme     → http/https
:authority  → host:port
:path       → /path?query
:status     → código de status (response)
```

**HPACK** (RFC 7541): compressão de headers usando tabela dinâmica e estática. Elimina redundância e reduz overhead.

### HTTP/3 (RFC 9114)

Sobre **QUIC** (RFC 9000), que usa UDP em vez de TCP:
- Conexão 0-RTT (zero round trip)
- Elimina head-of-line blocking do TCP
- Migração de conexão entre redes
- Criptografia integrada (TLS 1.3 embutido)

**QPACK** (RFC 9204): compressão de headers adaptada para HTTP/3.

## 2. Caching HTTP (RFC 9111)

### Tipos de Cache

| Cache | Localização | Exemplos |
|-------|------------|----------|
| Private | Browser do cliente | Cache do navegador |
| Shared | Entre cliente e servidor | CDN, reverse proxy |

### Estados da Resposta em Cache

```
fresh → pode ser reusado sem consultar servidor
stale → expirado, precisa revalidação
```

### Diretivas Cache-Control

| Diretiva | Uso Recomendado | Comportamento |
|----------|----------------|---------------|
| `max-age=3600` | Sempre | Tempo de vida em segundos |
| `no-cache` | HTML principal | Força revalidação (não reusa sem verificar) |
| `no-store` | Dados sensíveis | Não armazena resposta |
| `private` | Conteúdo personalizado | Apenas cache do cliente |
| `public` | Assets públicos | Permite cache mesmo com Authorization |
| `immutable` | Assets versionados | Nunca revalida (reload não revalida) |
| `must-revalidate` | Crítico | Não reusa stale sem revalidação |

### Padrão Recomendado de Cache

```
Recurso HTML principal:
  Cache-Control: no-cache
  ETag: <hash>
  Last-Modified: <data>

Subrecursos (JS, CSS, imagens) versionados:
  Cache-Control: public, max-age=31536000, immutable

Conteúdo personalizado (por usuário):
  Cache-Control: no-cache, private

API responses:
  Cache-Control: no-store    (dados mutáveis)
  Cache-Control: public, max-age=60  (dados semi-estáticos)
```

### Cache Busting

Incluir hash/versão na URL do recurso:
```html
<script src="/js/app.a1b2c3d4.js"></script>
<link rel="stylesheet" href="/css/styles.e5f6g7h8.css">
```

### Vary Header

Informa caches que a resposta varia com headers específicos:
```
Vary: Accept-Encoding
Vary: Accept-Language
Vary: Accept-Encoding, Accept-Language
```

## 3. Compressão

### Três Níveis

1. **Compressão de formato**: lossless (PNG, GIF) vs lossy (JPEG, WebP, AVIF)
2. **End-to-end**: `Accept-Encoding: gzip, br` → `Content-Encoding: br`
3. **Hop-by-hop**: `TE: gzip` → `Transfer-Encoding: gzip` (raro)

### Algoritmos Modernos

| Algoritmo | Prioridade | Eficiência | Suporte |
|-----------|-----------|------------|---------|
| `br` (Brotli) | 1ª | ~20% melhor que gzip | Todos browsers modernos |
| `gzip` | 2ª | Bom | Universal |
| `deflate` | 3ª | Inferior | Legado |

### Compression Dictionary Transport (RFC 9842)

Usa recurso existente como dicionário para compressão delta de nova versão.

**Headers**:
- Request: `Available-Dictionary: <SHA-256 do dicionário>`
- Response: `Use-As-Dictionary: <definição>`, `Content-Encoding: dcb` (ou `dcz`)
- Response: `Dictionary-ID: <identificador>`

**Restrições**: dicionários same-origin; recursos comprimidos seguem CORS.

## 4. Negociação de Conteúdo

### Server-Driven (Proativo)

Cliente envia preferências → servidor escolhe representação:

| Header | Propósito | Exemplo |
|--------|-----------|---------|
| `Accept` | MIME types preferidos | `Accept: text/html, application/json` |
| `Accept-Language` | Idioma | `Accept-Language: pt-BR, en` |
| `Accept-Encoding` | Compressão | `Accept-Encoding: gzip, br` |

Servidor indica que resposta variou via `Vary`.

### Agent-Driven (Reativo)

Servidor retorna 300 Multiple Choices ou 406 Not Acceptable com links para o cliente escolher.

## 5. Range Requests

Permite solicitar partes parciais de um recurso.

```
Request:  GET /video.mp4
          Range: bytes=0-1023

Response: 206 Partial Content
          Content-Range: bytes 0-1023/1000000
          Content-Length: 1024
```

**Casos de uso**: mídia com seek aleatório, download managers (pause/resume).

**Headers**:
- Request: `Range: bytes=<start>-<end>`
- Response: `Accept-Ranges: bytes` (indica suporte)
- Response: `Content-Range: bytes <start>-<end>/<total>`
- Erro: `416 Range Not Satisfiable`

## 6. Conditional Requests

Usam validators para evitar transferência desnecessária ou detectar conflitos.

### Validators

| Validator | Fraco | Forte | Exemplo |
|-----------|-------|-------|---------|
| `Last-Modified` | ✅ (1s resolução) | ❌ | `Last-Modified: Tue, 15 Nov 2024 12:00:00 GMT` |
| `ETag` | `W/"hash"` | `"hash"` | `ETag: "abc123"` |

### Headers Condicionais

| Header | Validator | Uso |
|--------|-----------|-----|
| `If-None-Match` | ETag | Cache: 304 se não modificado |
| `If-Modified-Since` | Last-Modified | Cache: 304 se não modificado |
| `If-Match` | ETag | Optimistic locking: 412 se conflito |
| `If-Unmodified-Since` | Last-Modified | Upload condicional |
| `If-Range` | ETag/Date | Range condicional |

## 7. Conexão: HTTP/1.1 vs HTTP/2 vs HTTP/3

| Aspecto | HTTP/1.1 | HTTP/2 | HTTP/3 |
|---------|----------|--------|--------|
| Transporte | TCP | TCP | QUIC (UDP) |
| Multiplexação | ❌ (pipelining limitado) | ✅ (streams) | ✅ (streams) |
| HOL Blocking | ✅ (TCP) | ❌ (dentro de conexão) | ❌ (QUIC elimina) |
| Compressão Headers | ❌ | HPACK | QPACK |
| Server Push | ❌ | ✅ (deprecated) | ❌ (removido) |
| Conexão 0-RTT | ❌ | ❌ | ✅ |
| Upgrade (WebSocket) | ✅ | ❌ | ❌ |

### Domain Sharding

Técnica obsoleta de dividir recursos entre múltiplos domínios para contornar limite de conexões HTTP/1.1.

> "Unless you have a very specific immediate need, don't use this deprecated technique; switch to HTTP/2 instead."

## 8. Protocol Upgrade (HTTP/1.1)

Mecanismo para upgrade de conexão para outro protocolo (ex.: WebSocket).

```
Request:  GET /chat HTTP/1.1
          Upgrade: websocket
          Connection: Upgrade
          Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==

Response: 101 Switching Protocols
          Upgrade: websocket
          Connection: Upgrade
          Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=
```

**HTTP/2 não permite** este mecanismo.
