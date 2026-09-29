# Exemplo de Implementação Moderna HTTP + URI

## Sumário

1. [Stack Completo de Headers](#1-stack-completo-de-headers)
2. [Configuração de Servidor (Nginx)](#2-configuração-de-servidor-nginx)
3. [Aplicação Web (Node.js + Express)](#3-aplicação-web-nodejs--express)
4. [API REST Segura](#4-api-rest-segura)
5. [Frontend HTML + JS Moderno](#5-frontend-html--js-moderno)
6. [Autenticação com Bearer + Refresh Token](#6-autenticação-com-bearer--refresh-token)
7. [Cookies Seguros](#7-cookies-seguros)
8. [Cache Estratégico](#8-cache-estratégico)
9. [Client Hints para Adaptação](#9-client-hints-para-adaptação)
10. [Compressão e Otimização](#10-compressão-e-otimização)
11. [Tratamento de Erros e Rate Limiting](#11-tratamento-de-erros-e-rate-limiting)
12. [Segurança em Camadas](#12-segurança-em-camadas)
13. [URI Schemes e Construção de URLs](#13-uri-schemes-e-construção-de-urls)
14. [Progressive Enhancement e Feature Detection](#14-progressive-enhancement-e-feature-detection)
15. [Monitoramento (NEL)](#15-monitoramento-nel)
16. [Checklist de Verificação](#16-checklist-de-verificação)

---

## 1. Stack Completo de Headers

### 1.1 Resposta HTML (Documento Principal)

```http
HTTP/1.1 200 OK
Content-Type: text/html; charset=utf-8
Content-Language: pt-BR
Content-Length: 28473
Cache-Control: no-cache
ETag: "7c8a3f9b1e2d4a6c"
Last-Modified: Tue, 21 Jul 2026 14:30:00 GMT
Strict-Transport-Security: max-age=63072000; includeSubDomains; preload
Content-Security-Policy: script-src 'nonce-a1b2c3d4e5f6'; object-src 'none'; base-uri 'none'
X-Content-Type-Options: nosniff
Referrer-Policy: strict-origin-when-cross-origin
Permissions-Policy: geolocation=(), camera=(), microphone=(), payment=(), usb=()
X-Frame-Options: DENY
Cross-Origin-Resource-Policy: same-origin
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
Reporting-Endpoints: nel="https://example.com/nel-reports"
NEL: {"report_to":"nel","max_age":31556952}
```

### 1.2 Resposta JSON (API)

```http
HTTP/1.1 200 OK
Content-Type: application/json; charset=utf-8
Cache-Control: private, max-age=60
ETag: "a1b2c3d4e5f6"
Vary: Accept-Encoding, Origin
Access-Control-Allow-Origin: https://app.example.com
Access-Control-Allow-Credentials: true
Access-Control-Expose-Headers: X-Request-Id
X-Request-Id: 7c8a3f9b-1e2d-4a6c-b3f5-8e7d1c2a4b6f
Strict-Transport-Security: max-age=63072000; includeSubDomains; preload
X-Content-Type-Options: nosniff
```

### 1.3 Resposta de Asset Versionado (JS/CSS/Imagem)

```http
HTTP/1.1 200 OK
Content-Type: text/javascript; charset=utf-8
Cache-Control: public, max-age=31536000, immutable
ETag: "d4e5f6a7b8c9"
Content-Encoding: br
Vary: Accept-Encoding
Strict-Transport-Security: max-age=63072000; includeSubDomains; preload
X-Content-Type-Options: nosniff
Cross-Origin-Resource-Policy: cross-origin
```

---

## 2. Configuração de Servidor (Nginx)

```nginx
# nginx.conf — Configuração moderna HTTP/2 + HTTP/3

server {
    listen 443 ssl http2;
    listen [::]:443 ssl http2;
    listen 443 quic reuseport;
    listen [::]:443 quic reuseport;
    http2 on;
    http3 on;

    server_name example.com;

    # TLS moderno
    ssl_certificate     /etc/ssl/certs/example.com.pem;
    ssl_certificate_key /etc/ssl/private/example.com.key;
    ssl_protocols       TLSv1.2 TLSv1.3;
    ssl_ciphers         ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256;
    ssl_prefer_server_ciphers off;
    ssl_session_cache   shared:SSL:10m;
    ssl_session_timeout 1d;
    ssl_session_tickets off;

    # HSTS
    add_header Strict-Transport-Security "max-age=63072000; includeSubDomains; preload" always;

    # Segurança
    add_header X-Content-Type-Options nosniff always;
    add_header X-Frame-Options DENY always;
    add_header Referrer-Policy "strict-origin-when-cross-origin" always;
    add_header Permissions-Policy "geolocation=(), camera=(), microphone=()" always;

    # Compressão Brotli + Gzip
    brotli on;
    brotli_types text/plain text/css application/json application/javascript text/xml
                 application/xml text/javascript image/svg+xml application/manifest+json;
    brotli_comp_level 6;
    gzip on;
    gzip_types text/plain text/css application/json application/javascript text/xml
               application/xml text/javascript image/svg+xml;
    gzip_min_length 256;
    gzip_comp_level 6;
    gzip_vary on;

    # Assets versionados (cache imutável)
    location /assets/ {
        expires 1y;
        add_header Cache-Control "public, max-age=31536000, immutable";
        add_header Cross-Origin-Resource-Policy cross-origin;
        try_files $uri =404;
    }

    # HTML principal (sempre validar)
    location / {
        expires -1;
        add_header Cache-Control "no-cache";
        add_header Cross-Origin-Resource-Policy same-origin;

        # CSP com nonce (gerado dinamicamente)
        set $csp_nonce "a1b2c3d4e5f6";
        add_header Content-Security-Policy "script-src 'nonce-$csp_nonce'; object-src 'none'; base-uri 'none'" always;

        try_files $uri $uri/ /index.html;
    }

    # API
    location /api/ {
        expires -1;
        add_header Cache-Control "no-store";
        add_header Access-Control-Allow-Origin "https://app.example.com" always;
        add_header Access-Control-Allow-Credentials "true" always;
        add_header Access-Control-Allow-Methods "GET, POST, PUT, PATCH, DELETE, OPTIONS" always;
        add_header Access-Control-Allow-Headers "Content-Type, Authorization, If-Match" always;
        add_header Vary "Origin" always;

        if ($request_method = OPTIONS) {
            add_header Content-Length 0;
            add_header Content-Type text/plain;
            return 204;
        }

        proxy_pass http://backend:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }

    # Redirecionamento HTTP → HTTPS
    error_page 497 =301 https://$host$request_uri;
}

# HTTP → HTTPS redirect
server {
    listen 80;
    listen [::]:80;
    server_name example.com www.example.com;
    return 301 https://example.com$request_uri;
}

# www → non-www
server {
    listen 443 ssl http2;
    server_name www.example.com;
    return 301 https://example.com$request_uri;
}
```

---

## 3. Aplicação Web (Node.js + Express)

```js
import express from 'express';
import crypto from 'node:crypto';
import { rateLimit } from 'express-rate-limit';

const app = express();

app.disable('x-powered-by');

// Geração de nonce CSP por request
app.use((req, res, next) => {
  const nonce = crypto.randomBytes(16).toString('base64url');
  res.locals.cspNonce = nonce;

  res.setHeader(
    'Content-Security-Policy',
    `script-src 'nonce-${nonce}'; object-src 'none'; base-uri 'none'`
  );
  next();
});

// Headers de segurança globais
app.use((req, res, next) => {
  res.setHeader('Strict-Transport-Security', 'max-age=63072000; includeSubDomains; preload');
  res.setHeader('X-Content-Type-Options', 'nosniff');
  res.setHeader('X-Frame-Options', 'DENY');
  res.setHeader('Referrer-Policy', 'strict-origin-when-cross-origin');
  res.setHeader('Permissions-Policy', 'geolocation=(), camera=(), microphone=()');
  res.setHeader('Cross-Origin-Resource-Policy', 'same-origin');
  res.setHeader('Cross-Origin-Opener-Policy', 'same-origin');
  next();
});

// Client Hints opt-in
app.use((req, res, next) => {
  res.setHeader(
    'Accept-CH',
    'Sec-CH-UA-Model, Sec-CH-UA-Form-Factors, Sec-CH-Prefers-Color-Scheme, Downlink'
  );
  next();
});

// Rate limiting
const limiter = rateLimit({
  windowMs: 60 * 1000,
  max: 100,
  standardHeaders: true,
  legacyHeaders: false,
  handler: (req, res) => {
    res.status(429).json({
      error: 'too_many_requests',
      message: 'Rate limit exceeded',
      retryAfter: res.getHeader('Retry-After')
    });
  }
});

app.use('/api/', limiter);

// Parse condicional com ETag
app.use(express.json());
app.use(express.urlencoded({ extended: true }));

// Rota com ETag e validação condicional
app.get('/api/users/:id', (req, res) => {
  const user = { id: req.params.id, name: 'Alice', email: 'alice@example.com' };
  const etag = crypto.createHash('sha256').update(JSON.stringify(user)).digest('hex');

  res.setHeader('ETag', `"${etag}"`);
  res.setHeader('Cache-Control', 'private, max-age=60');
  res.setHeader('Vary', 'Accept-Encoding, Origin');

  // 304 Not Modified
  if (req.headers['if-none-match'] === `"${etag}"`) {
    return res.status(304).end();
  }

  res.json(user);
});

// PUT com optimistic locking
app.put('/api/users/:id', (req, res) => {
  const etag = req.headers['if-match'];

  if (!etag) {
    return res.status(428).json({
      error: 'precondition_required',
      message: 'Send If-Match header with current ETag'
    });
  }

  // Simula conflito
  const currentEtag = '"current-etag-value"';
  if (etag !== currentEtag) {
    return res.status(412).json({
      error: 'precondition_failed',
      message: 'Resource modified by another client'
    });
  }

  const newEtag = crypto.randomBytes(16).toString('hex');
  res.setHeader('ETag', `"${newEtag}"`);
  res.status(200).json({ updated: true, etag: newEtag });
});

// POST com 201 Created + Location
app.post('/api/users', (req, res) => {
  const newUser = { id: crypto.randomUUID(), ...req.body };
  res.setHeader('Location', `/api/users/${newUser.id}`);
  res.status(201).json(newUser);
});

// DELETE com 204 No Content
app.delete('/api/users/:id', (req, res) => {
  res.status(204).end();
});

// Tratamento de erros
app.use((err, req, res, next) => {
  const requestId = crypto.randomUUID();
  res.setHeader('X-Request-Id', requestId);

  const status = err.status || 500;
  res.status(status).json({
    error: status === 500 ? 'internal_error' : 'request_error',
    message: status === 500 ? 'An unexpected error occurred' : err.message,
    requestId
  });

  console.error(`[${requestId}] ${err.stack || err}`);
});

app.listen(3000);
```

---

## 4. API REST Segura

### 4.1 Estrutura de Endpoints

| Método | Path                      | Body | Cache           | Auth     | Idempotente |
|--------|---------------------------|------|-----------------|----------|-------------|
| GET    | `/api/v1/users`           | ❌   | `max-age=60`    | Bearer   | ✅          |
| GET    | `/api/v1/users/{id}`      | ❌   | `max-age=60`    | Bearer   | ✅          |
| POST   | `/api/v1/users`           | ✅   | `no-store`      | Bearer   | ❌          |
| PUT    | `/api/v1/users/{id}`      | ✅   | `no-store`      | Bearer+ETag | ✅       |
| PATCH  | `/api/v1/users/{id}`      | ✅   | `no-store`      | Bearer+ETag | ❌       |
| DELETE | `/api/v1/users/{id}`      | ❌   | `no-store`      | Bearer   | ✅          |

### 4.2 CORS Configurado por Origem

```js
const ALLOWED_ORIGINS = [
  'https://app.example.com',
  'https://admin.example.com'
];

app.use('/api/', (req, res, next) => {
  const origin = req.headers.origin;
  if (ALLOWED_ORIGINS.includes(origin)) {
    res.setHeader('Access-Control-Allow-Origin', origin);
    res.setHeader('Access-Control-Allow-Credentials', 'true');
    res.setHeader('Vary', 'Origin');
  }

  if (req.method === 'OPTIONS') {
    res.setHeader('Access-Control-Allow-Methods', 'GET, POST, PUT, PATCH, DELETE');
    res.setHeader('Access-Control-Allow-Headers', 'Content-Type, Authorization, If-Match, If-None-Match');
    res.setHeader('Access-Control-Max-Age', '86400');
    return res.status(204).end();
  }

  next();
});
```

### 4.3 Paginação e Filtros via Query

```
GET /api/v1/users?role=admin&status=active&page=2&limit=20&sort=created_at:desc

Response:
{
  "data": [...],
  "pagination": {
    "page": 2,
    "limit": 20,
    "total": 156,
    "totalPages": 8,
    "next": "/api/v1/users?role=admin&page=3&limit=20",
    "prev": "/api/v1/users?role=admin&page=1&limit=20"
  }
}
```

---

## 5. Frontend HTML + JS Moderno

```html
<!DOCTYPE html>
<html lang="pt-BR">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Aplicação Moderna</title>

  <!-- DNS Prefetch + Preconnect -->
  <link rel="dns-prefetch" href="https://api.example.com">
  <link rel="preconnect" href="https://api.example.com" crossorigin>

  <!-- CSS versionado (cache imutável) -->
  <link rel="stylesheet" href="/assets/styles.a1b2c3d4.css" nonce="a1b2c3d4e5f6">
</head>
<body>
  <div id="app"></div>

  <!-- JS versionado com nonce CSP -->
  <script src="/assets/app.a1b2c3d4.js" type="module" nonce="a1b2c3d4e5f6"></script>
</body>
</html>
```

```js
// app.js — Frontend moderno

// Feature detection em vez de browser detection
const supportsWebP = () => {
  const canvas = document.createElement('canvas');
  return canvas.toDataURL('image/webp').startsWith('data:image/webp');
};

// Client Hints via JS (onde disponível)
const getClientHints = async () => {
  if (navigator.userAgentData) {
    const hints = await navigator.userAgentData.getHighEntropyValues([
      'platform', 'platformVersion', 'model', 'uaFullVersion'
    ]);
    return hints;
  }
  return null;
};

// Fetch com credentials e conditional request
const apiFetch = async (url, options = {}) => {
  const token = sessionStorage.getItem('access_token');

  const headers = {
    'Content-Type': 'application/json',
    ...(token && { Authorization: `Bearer ${token}` }),
    ...options.headers
  };

  const response = await fetch(url, {
    ...options,
    headers,
    credentials: 'include'
  });

  if (response.status === 401) {
    // Refresh token flow
    const refreshed = await refreshToken();
    if (refreshed) return apiFetch(url, options);
    window.location.href = '/login';
  }

  return response;
};

// Text fragments para links internos
const link = document.createElement('a');
link.href = 'https://example.com/page#:~:text=palavra-chave';
link.rel = 'noopener';
link.textContent = 'Ver detalhes';
document.body.appendChild(link);

// Media fragments para vídeo
const video = document.createElement('video');
video.controls = true;
video.src = '/video.mp4#t=30,60'; // Trecho 30s-60s
document.body.appendChild(video);

// Blob URL com gerenciamento de memória
const loadImage = async (url) => {
  const response = await fetch(url);
  const blob = await response.blob();
  const blobUrl = URL.createObjectURL(blob);

  const img = new Image();
  img.src = blobUrl;
  img.onload = () => URL.revokeObjectURL(blobUrl);
  document.body.appendChild(img);
};
```

---

## 6. Autenticação com Bearer + Refresh Token

```http
### Login Request
POST /api/auth/login HTTP/1.1
Content-Type: application/json

{
  "email": "user@example.com",
  "password": "secure-password"
}

### Login Response (201 Created)
HTTP/1.1 201 Created
Content-Type: application/json
Cache-Control: no-store
Set-Cookie: __Host-refresh_token=eyJhbGci...; Secure; HttpOnly; SameSite=Strict; Path=/api/auth; Max-Age=2592000

{
  "access_token": "eyJhbGciOiJIUzI1NiIs...",
  "expires_in": 900,
  "token_type": "Bearer"
}

### API Request com Bearer
GET /api/v1/users HTTP/1.1
Authorization: Bearer eyJhbGciOiJIUzI1NiIs...

### Refresh Token Request
POST /api/auth/refresh HTTP/1.1
Cookie: __Host-refresh_token=eyJhbGci...

### Refresh Response
HTTP/1.1 200 OK
Set-Cookie: __Host-refresh_token=eyJnew...; Secure; HttpOnly; SameSite=Strict; Path=/api/auth; Max-Age=2592000

{
  "access_token": "eyJnew...",
  "expires_in": 900
}
```

```js
// Servidor — Refresh Token
app.post('/api/auth/login', async (req, res) => {
  const { email, password } = req.body;

  // Valida credenciais...
  const accessToken = generateAccessToken({ sub: userId });
  const refreshToken = generateRefreshToken({ sub: userId });

  res.setHeader('Cache-Control', 'no-store');
  res.cookie('__Host-refresh_token', refreshToken, {
    secure: true,
    httpOnly: true,
    sameSite: 'strict',
    path: '/api/auth',
    maxAge: 30 * 24 * 60 * 60 * 1000 // 30 days
  });

  res.status(201).json({
    access_token: accessToken,
    expires_in: 900,
    token_type: 'Bearer'
  });
});

app.post('/api/auth/refresh', (req, res) => {
  const refreshToken = req.cookies?.__Host-refresh_token;
  if (!refreshToken) return res.sendStatus(401);

  try {
    const payload = verifyRefreshToken(refreshToken);
    const newAccess = generateAccessToken({ sub: payload.sub });
    const newRefresh = generateRefreshToken({ sub: payload.sub });

    // Rotate refresh token
    res.cookie('__Host-refresh_token', newRefresh, {
      secure: true,
      httpOnly: true,
      sameSite: 'strict',
      path: '/api/auth',
      maxAge: 30 * 24 * 60 * 60 * 1000
    });

    res.json({ access_token: newAccess, expires_in: 900 });
  } catch {
    res.status(401).json({ error: 'invalid_token' });
  }
});
```

---

## 7. Cookies Seguros

```http
# Cookie de sessão (máxima segurança)
Set-Cookie: __Host-session=abc123; Secure; HttpOnly; SameSite=Strict; Path=/; Max-Age=86400

# Cookie de preferência (Lax, permitido em navegação GET)
Set-Cookie: lang=pt-BR; Secure; SameSite=Lax; Path=/; Max-Age=31536000

# Cookie third-party (analytics) — evitar quando possível
Set-Cookie: _ga=GA1.2.123456789; Secure; SameSite=None; Path=/; Max-Age=63072000

# Remoção de cookie
Set-Cookie: __Host-session=; Secure; HttpOnly; SameSite=Strict; Path=/; Max-Age=0

# Remoção em massa
Clear-Site-Data: "cookies"
```

---

## 8. Cache Estratégico

| Tipo de Recurso         | Cache-Control                          | Validação         | Cache Busting       |
|-------------------------|----------------------------------------|-------------------|---------------------|
| HTML (app shell)        | `no-cache`                             | ETag + Last-Mod   | ❌                  |
| JS/CSS versionado       | `public, max-age=31536000, immutable`  | ETag              | Hash no filename    |
| Imagens não versionadas | `public, max-age=86400`                | ETag              | ?v=hash opcional    |
| Favicon                 | `public, max-age=604800`               | ETag              | ❌                  |
| Web Font (WOFF2)        | `public, max-age=31536000, immutable`  | ETag              | Hash no filename    |
| API (dados mutáveis)    | `no-store`                             | —                 | ❌                  |
| API (dados semi-estáticos) | `public, max-age=60`               | ETag              | ❌                  |
| Dados sensíveis         | `no-store, private`                    | —                 | ❌                  |
| Conteúdo personalizado  | `no-cache, private`                    | ETag + Last-Mod   | ❌                  |

### HTML com Cache Busting via Assets

```html
<link rel="stylesheet" href="/assets/app.a1b2c3d4.css">
<script src="/assets/vendor.e5f6g7h8.js" type="module"></script>
<script src="/assets/main.i9j0k1l2.js" type="module"></script>
```

---

## 9. Client Hints para Adaptação

```http
# Servidor solicita hints específicos
Accept-CH: Sec-CH-UA-Model, Sec-CH-UA-Form-Factors, Sec-CH-Prefers-Color-Scheme, Downlink

# Browser envia low entropy (sempre):
Sec-CH-UA: "Chrome";v="143", "Chromium";v="143"
Sec-CH-UA-Platform: "Android"
Sec-CH-UA-Mobile: ?1

# Browser envia high entropy (após opt-in):
Sec-CH-UA-Model: "Pixel 9"
Sec-CH-UA-Form-Factors: "Mobile"
Sec-CH-Prefers-Color-Scheme: "dark"
Downlink: "5.0"

# Cache varia conforme hints
Vary: Sec-CH-UA-Model, Sec-CH-UA-Form-Factors, Sec-CH-Prefers-Color-Scheme
```

### Uso no Servidor (Node.js)

```js
app.get('/api/adaptive-content', (req, res) => {
  const prefersDark = req.headers['sec-ch-prefers-color-scheme'] === 'dark';
  const isMobile = req.headers['sec-ch-ua-mobile'] === '?1';
  const downlink = parseFloat(req.headers['downlink'] || '4');

  const quality = downlink >= 4 ? 'high' : downlink >= 1 ? 'medium' : 'low';

  res.setHeader('Vary', 'Sec-CH-Prefers-Color-Scheme, Sec-CH-UA-Mobile, Downlink');
  res.json({
    theme: prefersDark ? 'dark' : 'light',
    layout: isMobile ? 'mobile' : 'desktop',
    quality
  });
});
```

### Uso no Frontend

```js
const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;
const prefersReducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
const connection = navigator.connection?.effectiveType || '4g';

if (prefersDark) document.documentElement.classList.add('dark-theme');
if (prefersReducedMotion) document.documentElement.classList.add('reduce-motion');
if (connection === 'slow-2g' || connection === '2g') {
  // Modo de dados reduzidos
}
```

---

## 10. Compressão e Otimização

### 10.1 Server-Side (Nginx)

```nginx
# Prioridade: Brotli > Gzip
brotli on;
brotli_comp_level 6;
brotli_types text/plain text/css application/json application/javascript
             image/svg+xml application/manifest+json font/woff2;

gzip on;
gzip_comp_level 6;
gzip_min_length 256;
gzip_types text/plain text/css application/json application/javascript
           image/svg+xml;
gzip_vary on;
```

### 10.2 Compression Dictionary Transport (RFC 9842)

```http
# Request (cliente oferece dicionário)
Available-Dictionary: <sha256-of-dictionary>

# Response (servidor usa dicionário)
Use-As-Dictionary: match="/app/*"
Content-Encoding: dcb
Dictionary-ID: <identifier>
```

### 10.3 Server Timing

```http
Server-Timing: db;dur=23, cache;desc="hit", api;dur=150
```

---

## 11. Tratamento de Erros e Rate Limiting

```http
### 400 Bad Request
HTTP/1.1 400 Bad Request
Content-Type: application/json
Cache-Control: no-store

{
  "error": "validation_error",
  "message": "Invalid email format",
  "details": [
    { "field": "email", "code": "invalid_format", "value": "not-an-email" }
  ]
}

### 401 Unauthorized
HTTP/1.1 401 Unauthorized
WWW-Authenticate: Bearer realm="api", error="invalid_token", error_description="Token expired"

### 403 Forbidden
HTTP/1.1 403 Forbidden
Content-Type: application/json
Cache-Control: no-store

{
  "error": "insufficient_permissions",
  "message": "Admin role required"
}

### 404 Not Found
HTTP/1.1 404 Not Found
Cache-Control: no-store

{
  "error": "not_found",
  "message": "User with id '999' not found"
}

### 409 Conflict
HTTP/1.1 409 Conflict
Content-Type: application/json
Cache-Control: no-store

{
  "error": "conflict",
  "message": "Username already taken"
}

### 412 Precondition Failed
HTTP/1.1 412 Precondition Failed
Content-Type: application/json
Cache-Control: no-store

{
  "error": "precondition_failed",
  "message": "Resource modified by another user"
}

### 429 Too Many Requests
HTTP/1.1 429 Too Many Requests
Retry-After: 120
Content-Type: application/json
Cache-Control: no-store

{
  "error": "too_many_requests",
  "message": "Rate limit exceeded. Retry after 120 seconds"
}

### 500 Internal Server Error
HTTP/1.1 500 Internal Server Error
Content-Type: application/json
Cache-Control: no-store

{
  "error": "internal_error",
  "message": "Unexpected server error",
  "requestId": "7c8a3f9b-1e2d-4a6c-b3f5-8e7d1c2a4b6f"
}
```

---

## 12. Segurança em Camadas

```
┌─────────────────────────────────────────────────────────────┐
│                     TRANSPORT LAYER                         │
│  TLS 1.2/1.3  │  HSTS  │  HTTPS enforced                   │
├─────────────────────────────────────────────────────────────┤
│                     HTTP LAYER                              │
│  CSP  │  X-Content-Type-Options  │  X-Frame-Options         │
│  Referrer-Policy  │  Permissions-Policy                    │
├─────────────────────────────────────────────────────────────┤
│                     CROSS-ORIGIN LAYER                      │
│  CORS  │  CORP  │  COOP  │  COEP  │  Fetch Metadata        │
├─────────────────────────────────────────────────────────────┤
│                     SESSION LAYER                           │
│  Secure │ HttpOnly │ SameSite │ __Host- prefix              │
│  Bearer tokens │ Refresh rotation │ Short-lived access      │
├─────────────────────────────────────────────────────────────┤
│                     APPLICATION LAYER                       │
│  Input validation │ Rate limiting │ CSRF protection         │
│  Output encoding │ Parameterized queries                    │
└─────────────────────────────────────────────────────────────┘
```

### 12.1 Fetch Metadata — Resource Isolation Policy

```js
// Servidor valida Fetch Metadata para bloquear abusos
app.use((req, res, next) => {
  const site = req.headers['sec-fetch-site'];
  const mode = req.headers['sec-fetch-mode'];
  const dest = req.headers['sec-fetch-dest'];

  // API só deve ser acessada via fetch/XHR, não via navegação direta
  if (req.path.startsWith('/api/')) {
    if (dest === 'document') {
      return res.status(403).json({ error: 'API cannot be navigated directly' });
    }
  }

  // Endpoints admin só de mesma origem
  if (req.path.startsWith('/admin/')) {
    if (site !== 'same-origin') {
      return res.status(403).json({ error: 'Admin requires same-origin' });
    }
  }

  next();
});
```

### 12.2 Strict CSP com Nonce

```http
Content-Security-Policy:
  script-src 'nonce-{RANDOM}';
  object-src 'none';
  base-uri 'none';
  form-action 'self';
  frame-ancestors 'none';
  upgrade-insecure-requests;
```

```html
<script nonce="a1b2c3d4e5f6" src="/assets/app.a1b2c3d4.js"></script>
```

---

## 13. URI Schemes e Construção de URLs

### 13.1 API REST — Estrutura de Path URI

```javascript
// Construção correta de URLs com URL API
const apiUrl = new URL('https://api.example.com/v1');

apiUrl.pathname = `/users/${userId}/orders`;
apiUrl.searchParams.set('page', '2');
apiUrl.searchParams.set('limit', '20');
apiUrl.searchParams.set('sort', 'created_at:desc');

fetch(apiUrl.toString());
// → https://api.example.com/v1/users/42/orders?page=2&limit=20&sort=created_at%3Adesc
```

### 13.2 Uso Correto de Fragment vs Query

```javascript
// Query: dados para o servidor
new URL('https://example.com/search?q=javascript&lang=pt-BR');

// Fragment: dados do cliente / navegação na página
new URL('https://example.com/docs#section-3');

// Text fragment: destaque de texto
new URL('https://example.com/page#:~:text=HTTP%20semantics');

// Media fragment: trecho de vídeo
new URL('https://example.com/video.mp4#t=30,120');

// Múltiplos text fragments
new URL('https://example.com/page#:~:text=HTTP&text=URI');
```

### 13.3 Uso de `blob:` com Gerenciamento de Memória

```javascript
async function loadAvatar(url) {
  const response = await fetch(url);
  const blob = await response.blob();
  const blobUrl = URL.createObjectURL(blob);

  const img = document.getElementById('avatar');
  img.src = blobUrl;
  img.onload = () => URL.revokeObjectURL(blobUrl);
}

// Para MediaStream, usar srcObject em vez de createObjectURL
const video = document.getElementById('camera');
video.srcObject = await navigator.mediaDevices.getUserMedia({ video: true });
```

### 13.4 Uso Correto de `data:` URLs (com moderação)

```html
<!-- ✅ Aceitável: ícone inline pequeno -->
<img src="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='24' height='24'%3E%3Ccircle cx='12' cy='12' r='10' fill='blue'/%3E%3C/svg%3E"
     alt="Ícone azul">

<!-- ❌ Evitar: dados grandes em data URLs -->
<!-- ❌ Bloqueado: navegação top-level para data: -->
```

### 13.5 Canonicidade de Domínio (www vs non-www)

```nginx
# Escolha: non-www como canônico
server {
    listen 443 ssl http2;
    server_name www.example.com;
    return 301 https://example.com$request_uri;
}

# Ou www como canônico
server {
    listen 443 ssl http2;
    server_name example.com;
    return 301 https://www.example.com$request_uri;
}
```

---

## 14. Progressive Enhancement e Feature Detection

```javascript
// ❌ EVITAR: browser detection
const isChrome = navigator.userAgent.includes('Chrome');

// ✅ PREFERIR: feature detection
const supportsGrid = CSS.supports('display', 'grid');
const supportsWebP = () => {
  const canvas = document.createElement('canvas');
  return canvas.toDataURL('image/webp').startsWith('data:image/webp');
};
const supportsTouch = 'ontouchstart' in window;
const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches;

// ✅ Client Hints API (moderno)
const getPlatform = async () => {
  if (navigator.userAgentData) {
    const { platform } = await navigator.userAgentData.getHighEntropyValues(['platform']);
    return platform;
  }
  return 'unknown';
};

// ✅ Detecção de engine (apenas quando necessário)
const engine = (() => {
  const ua = navigator.userAgent;
  if (ua.includes('Firefox')) return 'gecko';
  if (ua.includes('Edg/')) return 'blink';
  if (ua.includes('Chrome')) return 'blink';
  if (ua.includes('Safari')) return 'webkit';
  return 'unknown';
})();

// Progressive Enhancement
if ('geolocation' in navigator) {
  navigator.geolocation.getCurrentPosition(showMap);
} else {
  showStaticMap();
}

if ('share' in navigator) {
  document.getElementById('share-btn').style.display = 'block';
}
```

---

## 15. Monitoramento (NEL)

```http
# Configuração do servidor
Reporting-Endpoints: nel="https://example.com/nel-reports"
NEL: {"report_to":"nel","max_age":31556952,"include_subdomains":true,"failure_fraction":1.0}
```

```json
// Relatório de erro DNS enviado pelo browser
{
  "age": 20,
  "type": "network-error",
  "url": "https://example.com/previous-page",
  "body": {
    "elapsed_time": 18,
    "method": "POST",
    "phase": "dns",
    "protocol": "http/1.1",
    "referrer": "https://example.com/previous-page",
    "sampling_fraction": 1,
    "server_ip": "",
    "status_code": 0,
    "type": "dns.name_not_resolved",
    "url": "https://cdn.example.com/resource.js"
  }
}
```

---

## 16. Checklist de Verificação

### Headers Obrigatórios (toda resposta)

- [ ] `Content-Type` correto para o recurso
- [ ] `Content-Length` (ou Transfer-Encoding: chunked)
- [ ] `X-Content-Type-Options: nosniff`
- [ ] `Strict-Transport-Security` (em produção)

### Headers de Segurança Recomendados

- [ ] `Content-Security-Policy` (strict CSP com nonces)
- [ ] `Referrer-Policy: strict-origin-when-cross-origin`
- [ ] `Permissions-Policy` (restritivo)
- [ ] `X-Frame-Options: DENY` (ou CSP frame-ancestors)

### Cache

- [ ] `Cache-Control` definido conforme tipo de recurso
- [ ] Assets versionados com `immutable` + hash no filename
- [ ] `ETag` para recursos cacheados
- [ ] `Vary` configurado corretamente

### CORS (quando aplicável)

- [ ] `Access-Control-Allow-Origin` específico (nunca `*` com credentials)
- [ ] `Access-Control-Allow-Credentials: true` (se necessário)
- [ ] `Vary: Origin` presente
- [ ] Preflight (OPTIONS) configurado

### URI e Links

- [ ] Apenas `https://` para sub-recursos
- [ ] Sem credenciais em URLs (`user:password@host`)
- [ ] Sem `javascript:` URLs
- [ ] `data:` URLs usadas com moderação
- [ ] `blob:` URLs liberadas após uso
- [ ] Text fragments com `rel="noopener"` cross-origin
- [ ] Domínio canônico definido (www vs non-www)

### Cookies

- [ ] `Secure` + `HttpOnly` para cookies de sessão
- [ ] `SameSite=Strict` (sessão) ou `Lax` (preferências)
- [ ] `__Host-` prefix para session fixation protection
- [ ] Refresh token rotacionado

### Autenticação

- [ ] HTTPS obrigatório
- [ ] Bearer tokens curtos (15 min)
- [ ] Refresh tokens com rotação
- [ ] Basic auth evitado

### Métodos HTTP

- [ ] GET/HEAD são seguros e idempotentes
- [ ] PUT/DELETE são idempotentes
- [ ] POST/PATCH não-idempotentes tratados corretamente
- [ ] OPTIONS para CORS preflight

### Status Codes

- [ ] 201 para criação com `Location` header
- [ ] 204 para deleção sem body
- [ ] 304 para cache válido
- [ ] 308 para redirect permanente (preserva método)
- [ ] 307 para redirect temporário (preserva método)
- [ ] 400/422 para erros de validação
- [ ] 401/403 para erros de autenticação/autorização
- [ ] 404 para recurso não encontrado
- [ ] 409 para conflitos
- [ ] 412 para precondition failed (optimistic locking)
- [ ] 429 para rate limiting com `Retry-After`
- [ ] 500 com `X-Request-Id` para rastreabilidade

### Compressão

- [ ] Brotli habilitado e prioritário
- [ ] Gzip como fallback
- [ ] `Vary: Accept-Encoding`

### Conexão

- [ ] HTTP/2 habilitado
- [ ] HTTP/3 (QUIC) habilitado se possível
- [ ] TLS 1.2 mínimo, TLS 1.3 preferido

### Privacidade

- [ ] Client Hints em vez de User-Agent string
- [ ] Feature detection em vez de browser detection
- [ ] Permissions Policy restritiva
- [ ] Referrer-Policy configurada

### Monitoramento

- [ ] NEL configurado (Network Error Logging)
- [ ] Server-Timing para performance
- [ ] `X-Request-Id` em erros 500
