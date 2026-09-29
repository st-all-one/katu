---
name: http-uri
description: >
  Modern HTTP/URI (RFC 9110–9114, RFC 3986): correct method semantics, status
  codes, conditional requests and caching, CORS, cookies, authentication,
  compression, security headers, URIs. Load when designing or reviewing HTTP
  APIs, caching, CORS, or cookie/auth behavior.
category: protocols
version: "2026.07"
tags: [http, uri, rest, api, security, caching, hsts, csp, cors, client-hints]
license: MIT
---

# Modern HTTP + URI

## Use When
- Designing/reviewing HTTP APIs, status codes, methods
- Caching, conditional requests, CORS, cookies
- Auth flows, security headers, redirects
- Correct URI construction and encoding

## Core Rules
- HTTPS always; TLS 1.2 min, 1.3 preferred. HSTS `max-age=63072000; includeSubDomains; preload`.
- HTTP/2 or /3 over /1.1.
- Method semantics: GET safe/idempotent, PUT/DELETE idempotent, POST/PATCH not idempotent. 201 Created, 204 No Content, 307/308 redirects.
- Cache explicitly: versioned assets `public, max-age=31536000, immutable`; HTML `no-cache` + `ETag`; mutable API `no-store`.
- Strict CSP with nonces + `object-src 'none'; base-uri 'none'`; never `'unsafe-inline'`.
- CORS: explicit origins; never `*` with credentials; always `Vary: Origin`.
- Cookies: `Secure; HttpOnly; SameSite` (+ `__Host-` prefix); prefer bearer + refresh rotation.
- Feature detection over UA sniffing; Client Hints (`Sec-CH-UA-*`) over UA strings.
- Brotli first, gzip fallback; `Vary: Accept-Encoding`.
- URIs: no credentials in URL, no `javascript:`, moderate `data:`, `revokeObjectURL` for `blob:`.

## Core Patterns
```http
# Cache + conditional
Cache-Control: public, max-age=31536000, immutable   # /assets/app.abc123.js
ETag: "abc123"
If-None-Match: "abc123"           → 304 Not Modified

# API
GET /users/42            → 200 { … }
POST /users              → 201 Created, Location: /users/42
PUT /users/42            → 200 (idempotent)
DELETE /users/42         → 204 No Content
PATCH /users/42          → 200 (partial, not idempotent)

# Security headers
Strict-Transport-Security: max-age=63072000; includeSubDomains; preload
Content-Security-Policy: default-src 'self'; script-src 'nonce-{RANDOM}'; object-src 'none'; base-uri 'none'
X-Content-Type-Options: nosniff
Referrer-Policy: strict-origin-when-cross-origin

# CORS
Access-Control-Allow-Origin: https://app.example
Access-Control-Allow-Credentials: true
Vary: Origin

# Cookie
Set-Cookie: __Host-session=…; Secure; HttpOnly; SameSite=Lax; Path=/

# URI
/users/42?filter=active&page=1
/ search / encode: encodeURIComponent(value)
```

## File Map
| File | Content |
|---|---|
| `00-index.md` | Index and reading guide |
| `01-fundamentos-http.md` | Messages, methods, status classes |
| `02-fundamentos-uri.md` | URI structure, encoding, schemes |
| `03-padrao-moderno-http.md` | Caching, conditional requests, HTTP/2-3 |
| `04-padrao-moderno-uri.md` | Canonical URI patterns |
| `05-seguranca.md` | Headers, CSP, HSTS, redirects |
| `06-modelo-basico.md` | Baseline request/response model |
| `07-especificacoes.md` | RFC map |
| `08-interdependencias.md` | Cross-standard interactions |
| `09-proxies-tunneling.md` | Proxies, CONNECT, tunneling |
| `10-client-hints.md` | Client Hint headers |
| `11-mime-types.md` | Media types, `Accept`, charset |
| `12-network-error-logging.md` | Network errors and logging |
| `13-authentication.md` | Auth schemes, bearer, refresh |
| `14-cookies.md` | Cookie attributes and scopes |
| `15-browser-detection.md` | Feature detection strategies |
| `16-scheme-ssh.md` | `ssh:` scheme reference |
| `EXEMPLO-IMPLEMENTACAO.md` | Reference implementation |

## Read Order
`00`→`01`→`03`→`13`→`14`→`05`.

## Prereqs
Basic client/server HTTP knowledge.
