# Modelo Básico de Implementação HTTP + URI

## 1. Stack de Headers Recomendado

### 1.1 Resposta HTTP Padrão (Aplicação Web Moderna)

```http
HTTP/1.1 200 OK

# Identidade do recurso
Content-Type: text/html; charset=utf-8
Content-Language: pt-BR
Content-Length: 12345

# Cache
Cache-Control: no-cache
ETag: "abc123def456"
Last-Modified: Tue, 15 Nov 2024 12:00:00 GMT

# Segurança (obrigatório)
Content-Security-Policy: script-src 'nonce-{RANDOM}'; object-src 'none'; base-uri 'none'
Strict-Transport-Security: max-age=63072000; includeSubDomains
X-Content-Type-Options: nosniff
Referrer-Policy: strict-origin-when-cross-origin
Permissions-Policy: geolocation=(), camera=(), microphone=()

# CORS (se aplicável)
Access-Control-Allow-Origin: https://trusted-origin.com
Access-Control-Allow-Methods: GET, POST
Access-Control-Allow-Headers: Content-Type, Authorization

# Framing protection (anti-clickjacking)
X-Frame-Options: DENY
# OU via CSP: Content-Security-Policy: frame-ancestors 'none'
```

### 1.2 API REST

```http
HTTP/1.1 200 OK
Content-Type: application/json; charset=utf-8
Cache-Control: no-store
Access-Control-Allow-Origin: https://app.example.com
Access-Control-Allow-Credentials: true
Vary: Origin
```

### 1.3 Assets Versionados (JS, CSS, Imagens)

```http
HTTP/1.1 200 OK
Content-Type: text/javascript; charset=utf-8
Cache-Control: public, max-age=31536000, immutable
ETag: "a1b2c3d4"
```

## 2. Padrões de Cache por Tipo de Recurso

| Tipo | Cache-Control | Validação | Cache Busting |
|------|---------------|-----------|---------------|
| HTML (app shell) | `no-cache` | ETag + Last-Modified | Não |
| JS/CSS versionados | `public, max-age=31536000, immutable` | ETag | Hash/versão no filename |
| Imagens não versionadas | `public, max-age=86400` | ETag | Query param opcional |
| API data | `no-store` ou `private, max-age=60` | ETag | Se aplicável |
| Dados sensíveis | `no-store, private` | — | — |
| User-specific HTML | `no-cache, private` | ETag + Last-Modified | Não |

## 3. Métodos HTTP — Padrão de Uso em APIs REST

| Operação | Método | Path | Body | Idempotente | Status Codes |
|----------|--------|------|------|-------------|--------------|
| Listar | GET | `/users` | ❌ | ✅ | 200, 400 |
| Obter | GET | `/users/{id}` | ❌ | ✅ | 200, 404 |
| Criar | POST | `/users` | ✅ | ❌ | 201, 400, 409 |
| Substituir | PUT | `/users/{id}` | ✅ | ✅ | 200, 201, 404 |
| Atualizar parcial | PATCH | `/users/{id}` | ✅ | ❌ | 200, 404, 409 |
| Remover | DELETE | `/users/{id}` | ❌ | ✅ | 204, 404 |
| Opções | OPTIONS | `*` ou `/users` | ❌ | ✅ | 204, 200 |

## 4. Validação e Erros

### 4.1 Conditional Requests (Cache Update)

```http
# Client: verifica se recurso mudou
GET /page.html
If-None-Match: "abc123"

# Server: não mudou
HTTP/1.1 304 Not Modified
ETag: "abc123"

# Server: mudou
HTTP/1.1 200 OK
ETag: "def456"
Content-Type: text/html
...
```

### 4.2 Optimistic Locking via ETag

```http
# Client: atualiza com condição
PUT /users/42
If-Match: "etag-atual"
Content-Type: application/json

{"name": "Novo Nome"}

# Server: conflito (outro cliente alterou)
HTTP/1.1 412 Precondition Failed

# Server: sucesso
HTTP/1.1 200 OK
ETag: "novo-etag"
```

### 4.3 Rate Limiting

```http
HTTP/1.1 429 Too Many Requests
Retry-After: 120

Content-Type: application/json

{
  "error": "too_many_requests",
  "message": "Rate limit exceeded. Try again in 120 seconds."
}
```

## 5. Redirecionamentos — Padrão Moderno

| Cenário | Código | Comportamento do Cliente |
|---------|--------|--------------------------|
| Página movida permanentemente | 308 | Preserva método, cacheável |
| Página movida temporariamente | 307 | Preserva método |
| POST → GET (após criação) | 303 | Força GET, não cacheável |
| URL do recurso mudou | 301 | Pode mudar método, cacheável |
| Redirect após login | 302 | Pode mudar método |

**Ordem de precedência**: HTTP redirects → JS redirects → HTML `<meta>` redirects

## 6. URIs — Padrões de Construção

### 6.1 API REST — Estrutura de Path

```
GET    /api/v1/users                      # Listar
GET    /api/v1/users/{id}                 # Obter
POST   /api/v1/users                      # Criar
PUT    /api/v1/users/{id}                 # Substituir
PATCH  /api/v1/users/{id}                 # Atualizar
DELETE /api/v1/users/{id}                 # Remover

# Filtros e paginação
GET    /api/v1/users?role=admin&page=1&limit=20

# Relacionamentos
GET    /api/v1/users/{id}/orders
GET    /api/v1/users/{id}/orders/{orderId}
```

### 6.2 Fragment vs Query — Quando Usar

| Use Query (`?`) | Use Fragment (`#`) |
|-----------------|-------------------|
| Dados que o servidor precisa processar | Âncoras de navegação na página |
| Parâmetros de busca/filtro/paginação | Text fragments para destaque |
| Parâmetros de tracking/analytics | Media fragments (tempo, área) |
| Tokens de autenticação (cuidado!) | Estado de UI (se não enviado ao servidor) |
| Versionamento de API | |

### 6.3 Text Fragments (Links para Conteúdo)

```html
<!-- Link para texto específico -->
<a href="https://example.com/page#:~:text=palavra-chave">
  Ver detalhes
</a>

<!-- Link com prefixo para match específico -->
<a href="https://example.com/page#:~:text=secao-,texto%20alvo,-sufixo"
   rel="noopener">
  Ver detalhes
</a>
```

### 6.4 Media Fragments

```html
<!-- Vídeo: trecho de 2s a 4s -->
<video controls>
  <source src="/video.mp4#t=2,4" type="video/mp4">
</video>

<!-- Áudio: apenas primeiros 5 segundos -->
<audio controls src="/audio.mp3#t=,5"></audio>

<!-- SVG: área específica -->
<img src="/diagram.svg#xywh=100,100,400,400" width="200" height="200">
```

## 7. Compressão — Configuração

### 7.1 Server-Side (Nginx)

```nginx
gzip on;
gzip_types text/plain text/css application/json application/javascript text/xml application/xml text/javascript image/svg+xml;
gzip_min_length 256;
gzip_comp_level 6;
gzip_vary on;

brotli on;
brotli_types text/plain text/css application/json application/javascript text/xml application/xml text/javascript image/svg+xml;
brotli_comp_level 4;
```

### 7.2 Client-Side (Fetch API)

```js
// O navegador gerencia Accept-Encoding automaticamente
const response = await fetch('/api/data', {
  headers: {
    'Accept': 'application/json',
    'Accept-Language': 'pt-BR,en;q=0.5'
  }
});
```

## 8. Conexão — Configuração de Servidor

### 8.1 HTTP/2 e HTTP/3

```nginx
server {
    listen 443 ssl http2;
    listen [::]:443 ssl http2;
    listen 443 quic reuseport;  # HTTP/3 (QUIC)

    http2 on;
    http3 on;
    http3_hq on;  # QPACK

    ssl_certificate /path/to/cert.pem;
    ssl_certificate_key /path/to/key.pem;
    ssl_protocols TLSv1.2 TLSv1.3;
}
```

## 9. Checklist de Headers Essenciais

### Obrigatórios (Todas Respostas)

| Header | Valor Recomendado | Propósito |
|--------|-------------------|-----------|
| `Content-Type` | Correto para o recurso | MIME type |
| `Content-Length` | Tamanho em bytes | Framing |
| `X-Content-Type-Options` | `nosniff` | Anti-MIME sniffing |

### Recomendados (Segurança)

| Header | Valor Recomendado | Propósito |
|--------|-------------------|-----------|
| `Strict-Transport-Security` | `max-age=63072000; includeSubDomains` | Força HTTPS |
| `Content-Security-Policy` | strict CSP com nonces | Anti-XSS |
| `Referrer-Policy` | `strict-origin-when-cross-origin` | Privacidade |
| `Permissions-Policy` | Restritivo (só o necessário) | API control |
| `X-Frame-Options` | `DENY` | Anti-clickjacking |

### Condicionais (Cache)

| Header | Quando Usar | Propósito |
|--------|-------------|-----------|
| `Cache-Control` | Sempre | Política de cache |
| `ETag` | Recursos cacheados | Validação forte |
| `Last-Modified` | Recursos cacheados | Validação fraca |
| `Vary` | Quando resposta varia | Informar caches |

### Específicos (CORS)

| Header | Quando Usar | Propósito |
|--------|-------------|-----------|
| `Access-Control-Allow-Origin` | API acessada cross-origin | CORS |
| `Access-Control-Allow-Methods` | Preflight | CORS |
| `Access-Control-Allow-Headers` | Preflight | CORS |
| `Access-Control-Max-Age` | Preflight | Cache de preflight |
| `Access-Control-Allow-Credentials` | Com cookies/auth | CORS authenticated |
| `Access-Control-Expose-Headers` | Headers custom expostos | CORS |
