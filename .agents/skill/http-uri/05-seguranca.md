# Segurança HTTP + URI

## 1. Mecanismos de Segurança HTTP

### 1.1 CORS — Cross-Origin Resource Sharing

Mecanismo baseado em headers HTTP que permite servidor indicar quais origins podem carregar recursos.

**Simple Requests** (sem preflight):
- Métodos: GET, HEAD, POST
- Content-Type: `application/x-www-form-urlencoded`, `multipart/form-data`, `text/plain`
- Sem headers customizados

**Preflighted Requests**:
- OPTIONS request antes do request real
- Acionado por: métodos não-simple, headers customizados, Content-Type não-simple

**Credentialed Requests**:
```js
fetch('https://api.example.com/data', {
  credentials: 'include'
})
```
- Exige `Access-Control-Allow-Credentials: true`
- **NUNCA** usar wildcard `*` em `Access-Control-Allow-Origin` com credentials

**Headers CORS**:

| Request | Response |
|---------|----------|
| `Origin` | `Access-Control-Allow-Origin` |
| `Access-Control-Request-Method` | `Access-Control-Allow-Methods` |
| `Access-Control-Request-Headers` | `Access-Control-Allow-Headers` |
| | `Access-Control-Max-Age` |
| | `Access-Control-Allow-Credentials` |
| | `Access-Control-Expose-Headers` |

### 1.2 CSP — Content Security Policy

Mitigação contra XSS e data injection via header `Content-Security-Policy`.

**Diretivas principais**:

| Diretiva | Controla |
|----------|----------|
| `default-src` | Fallback para todas fetch directives |
| `script-src` | Scripts permitidos |
| `style-src` | Stylesheets permitidos |
| `img-src` | Imagens permitidas |
| `connect-src` | Fetch/XMLHttpRequest/WebSocket |
| `font-src` | Fontes |
| `frame-src` / `frame-ancestors` | Frames / quem pode embedar |
| `base-uri` | `<base>` tags |
| `form-action` | Destinos de formulários |
| `object-src` | `<object>`, `<embed>`, `<applet>` |

**Strict CSP (recomendado)**:
```http
Content-Security-Policy:
  script-src 'nonce-{RANDOM}';
  object-src 'none';
  base-uri 'none';
```

```html
<script nonce="{RANDOM}" src="app.js"></script>
```

> "Allowlist CSPs are hard to get right and often policies inadvertently whitelist unsafe domains, and hence don't provide effective protection against XSS."

**⚠️ Nunca usar**: `'unsafe-inline'` em `script-src` — anula proteção contra XSS.

### 1.3 CORP — Cross-Origin Resource Policy

Proteção contra side-channel attacks (Spectre) especificando quais origins podem carregar recursos via elementos HTML.

```
Cross-Origin-Resource-Policy: same-site
Cross-Origin-Resource-Policy: same-origin
Cross-Origin-Resource-Policy: cross-origin
```

Apenas para requests `no-cors` (elementos `<script>`, `<img>`, `<link>`, etc.).

### 1.4 Fetch Metadata Headers

Headers `Sec-*` que informam servidor sobre contexto da requisição:

| Header | Valores | Informação |
|--------|---------|------------|
| `Sec-Fetch-Site` | `cross-site`, `same-site`, `same-origin`, `none` | Origem da requisição |
| `Sec-Fetch-Mode` | `navigate`, `same-origin`, `no-cors`, `cors`, `websocket` | Modo do fetch |
| `Sec-Fetch-User` | `?1` | Se foi iniciada por usuário |
| `Sec-Fetch-Dest` | `document`, `iframe`, `script`, `style`, `image`, `empty`, etc. | Destino do fetch |

**Resource Isolation Policy**: servidor pode bloquear requisições que não sejam same-origin, same-site confiável ou navegações top-level.

### 1.5 Permissions Policy (antiga Feature Policy)

Controla acesso a APIs poderosas via header ou atributo `allow` em iframes.

```http
Permissions-Policy: geolocation=(self), camera=()
```

```html
<iframe src="..." allow="geolocation 'none'"></iframe>
```

APIs controláveis: geolocation, camera, microphone, fullscreen, payment, accelerometer, gyroscope, magnetometer, usb, bluetooth, etc.

### 1.6 Iframe Credentialless

Mecanismo para carregar recursos third-party em iframes com contexto efêmero, sem acesso a cookies ou storage do origin de terceiros.

```html
<iframe src="https://third-party.com" credentialless></iframe>
```

Usado com `Cross-Origin-Embedder-Policy: require-corp` para embedar conteúdo third-party.

### 1.7 HSTS — Strict-Transport-Security

Força navegador a usar HTTPS exclusivamente.

```http
Strict-Transport-Security: max-age=63072000; includeSubDomains; preload
```

- `max-age`: tempo em segundos
- `includeSubDomains`: aplica a todos subdomínios
- `preload`: inclusão em listas de pré-carregamento de browsers

### 1.8 X-Content-Type-Options

Previne MIME sniffing:

```http
X-Content-Type-Options: nosniff
```

O navegador confiará estritamente no header `Content-Type` e não tentará adivinhar o tipo.

### 1.9 User-Agent Reduction

Redução de informações sensíveis na string UA. Substituição por Client Hints (`Sec-CH-UA-*`).

**Informações removidas**:
- Versão exata do SO/plataforma
- Modelo do dispositivo
- Versão minoritária do browser

## 2. Segurança em URIs

### 2.1 Semantic URL Attacks

Phishing via componente `user@host` na authority:

```
https://cnn.example.com&story=breaking_news@10.0.0.1
```

Parece ser `cnn.example.com` mas o host real é `10.0.0.1`. Tudo antes do `@` é o componente "user".

**Mitigação**: browsers modernos mostram aviso ou ocultam user info em URLs.

### 2.2 User Info em URLs

> "Providing user information directly in HTTP URLs is not recommended, as it can expose sensitive information."

```url
postgresql://postgres:admin123@db:5432  ← NUNCA FAZER
```

**Alternativa**: HTTP authentication (Basic, Bearer, Digest) ou session cookies.

### 2.3 `javascript:` URLs

> "Using `javascript:` URLs on the web is discouraged as it may lead to execution of arbitrary code, similar to the ramifications of using `eval()`."

**Riscos**:
- Execução arbitrária de código
- Redução de acessibilidade
- Navegação acidental se código retornar string
- Sem entrada no histórico do navegador
- Bloqueio por CSP

**Alternativas**:
- `<button>` + event listener em vez de `<a href="javascript:...">`
- `srcdoc` em vez de `<iframe src="javascript:...">`
- Event listener `submit` + `preventDefault()` em vez de `<form action="javascript:...">`

### 2.4 `data:` URLs — Top-Level Navigation

Navegação top-level para `data:` URLs é bloqueada em todos browsers modernos devido a phishing.

### 2.5 `resource:` URLs — Exposição de Configurações

Firefox ≤ 56 permitia sites lerem configurações internas via `resource://`.

Mitigado no Firefox 57+ com `resource://content-accessible/`.

### 2.6 Text Fragments — Cross-Origin Security

Requer `rel="noopener"` em links cross-origin com text fragments para prevenir acesso ao contexto da página alvo.

## 3. Segurança em Autenticação e Sessão

### HTTP Authentication

| Scheme | Segurança | Uso |
|--------|-----------|-----|
| Basic (RFC 7617) | **Inseguro sem HTTPS** | Legado, evitar |
| Bearer (RFC 6750) | ✅ (com HTTPS) | OAuth 2.0 tokens |
| Digest (RFC 7616) | ✅ (MD5/SHA-256) | Autenticação HTTP |
| HOBA (RFC 7486) | ✅ | Origin-Bound Auth |
| Mutual (RFC 8120) | ✅ | Autenticação mútua |

> "The 'Basic' authentication scheme sends the credentials encoded but not encrypted. This would be completely insecure unless the exchange was over a secure connection (HTTPS/TLS)."

### Cookies

Atributos de segurança de cookie:

| Atributo | Protege Contra | Uso |
|----------|---------------|-----|
| `Secure` | Eavesdropping | Cookies só enviados em HTTPS |
| `HttpOnly` | XSS (roubo de cookie) | Inacessível via JavaScript |
| `SameSite=Strict` | CSRF | Cookie não enviado em requests cross-site |
| `SameSite=Lax` | CSRF (default) | Cookie enviado em navegações top-level |
| `SameSite=None` | — | Third-party cookie (exige `Secure`) |
| `Domain` | Escopo | Restringe a domínios específicos |
| `Path` | Escopo | Restringe a paths específicos |
| `__Host-` prefix | Session fixation | Requer Secure + Path=/ + sem Domain |
| `__Secure-` prefix | Session fixation | Requer Secure |

**Prefixo `__Host-`**: defense-in-depth contra session fixation.
```
Set-Cookie: __Host-session=abc123; Secure; Path=/
```

## 4. Matriz de Segurança: Header por Ameaça

| Ameaça | Header de Mitigação |
|--------|---------------------|
| XSS | `Content-Security-Policy` |
| MIME sniffing | `X-Content-Type-Options: nosniff` |
| Clickjacking | `X-Frame-Options: DENY` / `frame-ancestors` |
| CSRF | `SameSite=Lax|Strict` em cookies |
| MITM / HTTP downgrade | `Strict-Transport-Security` |
| Cross-origin data theft | `Cross-Origin-Resource-Policy` |
| Spectre side-channel | `Cross-Origin-Resource-Policy` + COOP + COEP |
| Phishing via data: URLs | Bloqueio automático (browsers) |
| Exfiltração de dados | `Permissions-Policy` |
| Referrer leakage | `Referrer-Policy: no-referrer` (ou strict) |
| User tracking | `Permissions-Policy` + User-Agent reduction |
