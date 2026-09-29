# Matriz de Interdependências HTTP ↔ URI ↔ Web Platform

## 1. Mapa de Dependências

```
                    ┌──────────────────────┐
                    │       URI/URL        │
                    │      (RFC 3986)      │
                    └──────────┬───────────┘
                               │
                    ┌──────────▼───────────┐
                    │   HTTP Semantics     │
                    │    (RFC 9110)        │
                    └──────────┬───────────┘
                               │
           ┌───────────────────┼───────────────────┐
           │                   │                   │
    ┌──────▼──────┐    ┌──────▼──────┐    ┌──────▼──────┐
    │   HTTP/1.1  │    │   HTTP/2    │    │   HTTP/3    │
    │  (RFC 9112) │    │  (RFC 9113) │    │  (RFC 9114) │
    └─────────────┘    └─────────────┘    └─────────────┘
           │                   │                   │
           └───────────────────┼───────────────────┘
                               │
                    ┌──────────▼───────────┐
                    │  Security Layer      │
                    │  CORS, CSP, HSTS,    │
                    │  Fetch Metadata,     │
                    │  Permissions Policy  │
                    └──────────────────────┘
```

## 2. HTTP ↔ URI

| Aspecto | HTTP | URI | Interdependência |
|---------|------|-----|------------------|
| Request target | A URI é o alvo de toda request HTTP | A URI fornece o scheme, authority e path | `GET https://example.com/path` |
| Host header | HTTP/1.1 exige `Host` header | Authority contém host + port | `Host: example.com` |
| Origin | CORS usa `Origin` header | Origin = scheme + host + port | `Origin: https://app.com` |
| Content negotiation | `Accept-*` headers determinam representação | Path + query identificam recurso | `Accept: text/html` vs `Accept: application/json` |
| Cache | `Vary` header referencia headers de URI | URL completa é chave do cache | `Vary: Accept-Language` |
| CSP | CSP bloqueia `javascript:` URLs | Scheme da URI determina policy | `script-src 'unsafe-inline'` bloqueia `javascript:` |
| HSTS | Força HTTPS | Scheme http:// → https:// | `Strict-Transport-Security` |
| Same-origin policy | Baseada em origin HTTP | Origin = scheme://host:port | `https://a.com` ≠ `https://b.com` |
| Fetch Metadata | Headers Sec-* informam destino | Destino = fetch target URL | `Sec-Fetch-Site: cross-site` |

## 3. HTTP ↔ HTML

| Recurso | HTTP | HTML |
|---------|------|------|
| Transporte | HTTP transporta documentos HTML | HTML é o payload principal |
| CSP | `Content-Security-Policy` header | Controla recursos HTML carregados |
| CORS | Headers CORS | Requests de páginas HTML cross-origin |
| Redirecionamento | HTTP redirects (3xx) | HTML `<meta http-equiv="refresh">` |
| Links | URI como target de link | `<a href="...">` usa URI |
| Framing | `X-Frame-Options` / `frame-ancestors` | `<iframe>`, `<frame>`, `<object>` |
| Forms | `POST`/`GET` com `Content-Type` específico | `<form action="..." method="...">` |
| MIME | `Content-Type` header | Browser usa MIME para interpretar HTML |
| Cache | `Cache-Control` header | Recursos HTML (documentos) |

## 4. HTTP ↔ JavaScript

| API | Mecanismo HTTP | Propósito |
|-----|---------------|-----------|
| Fetch | GET/POST/PUT/DELETE sobre HTTP | Chamadas de API |
| XMLHttpRequest | GET/POST/etc sobre HTTP | Chamadas de API (legado) |
| WebSocket | Upgrade HTTP/1.1 | Conexão bidirecional |
| EventSource (SSE) | GET com `text/event-stream` | Server-Sent Events |
| Service Worker | HTTP intercept via fetch event | Offline, cache, proxy |

## 5. HTTP ↔ TLS/HTTPS

| Aspecto | HTTP | TLS/HTTPS |
|---------|------|-----------|
| Segurança | HTTP sem TLS é texto plano | TLS criptografa tudo |
| Port default | 80 | 443 |
| HSTS | Header HTTP | Força TLS |
| ALPN | Negotiation de HTTP/1.1 vs HTTP/2 | Extensão TLS |
| QUIC (HTTP/3) | UDP-based | TLS 1.3 embutido |

## 6. URI ↔ HTML

| Componente URI | Atributo HTML | Exemplo |
|---------------|---------------|---------|
| Scheme | `href`, `src` | `<a href="https://...">` |
| Path | `href`, `src` | `<img src="/images/photo.jpg">` |
| Query | `href`, `action` | `<a href="?page=2">` |
| Fragment | `href` com `#` | `<a href="#section">` |
| Text fragment | `href` com `#:~:text=` | `<a href="#:~:text=foo">` |
| `data:` URL | `src`, `href` | `<img src="data:image/png,...">` |
| `blob:` URL | `src` de mídia | `<video src="blob:...">` |
| `javascript:` URL | `href` (evitar) | `<a href="javascript:...">` |

## 7. URI ↔ Web APIs

| API Web | Como usa URI | Detalhes |
|---------|-------------|----------|
| `URL` | Parse, construção, normalização | `new URL('https://example.com/path')` |
| `URLSearchParams` | Manipulação de query | `new URLSearchParams('?key=val')` |
| `URL.createObjectURL()` | Cria `blob:` URLs | Gerenciamento de memória |
| `History API` | Manipulação de URL no browser | `pushState`, `replaceState` |
| `Location` | URL atual da página | `window.location` |
| `navigator.registerProtocolHandler()` | Handler customizado para scheme | `registerProtocolHandler('mailto', ...)` |

## 8. Dependências de Segurança (Cross-layer)

| Ameaça | Camada HTTP | Camada URI | Camada HTML |
|--------|-------------|------------|-------------|
| XSS | CSP | `javascript:` bloqueado | Sanitização de input |
| CSRF | `SameSite` cookie | — | CSRF tokens |
| Clickjacking | `X-Frame-Options` | — | `frame-ancestors` CSP |
| Phishing | HSTS | `data:` bloqueado top-level | Visual indicators |
| Data theft | CORP, COOP | Blob partitioning | Cross-origin isolation |
| Tracking | Fetch Metadata | User info em URLs | Permissions Policy |
| Session fixation | `__Host-` cookie prefix | — | — |
| MITM | HSTS | HTTPS enforcement | Upgrade Insecure Requests |

## 9. Pirâmide de Abstinação Web Platform

```
                    ┌─────────────────────┐
                    │   Web Applications   │
                    │  (React, Angular,    │
                    │   APIs, PWA)         │
                    └──────────┬──────────┘
                               │
                    ┌──────────▼──────────┐
                    │    HTML/CSS/JS      │
                    │  (DOM, Events,      │
                    │   CSSOM, Web APIs)  │
                    └──────────┬──────────┘
                               │
                    ┌──────────▼──────────┐
                    │     HTTP + URI      │
                    │  (Métodos, Status,  │
                    │   Components,       │
                    │   Schemes)          │
                    └──────────┬──────────┘
                               │
                    ┌──────────▼──────────┐
                    │   Security Layer    │
                    │  (CORS, CSP, HSTS,  │
                    │   TLS, COOP, COEP,  │
                    │   Fetch Metadata)   │
                    └──────────┬──────────┘
                               │
                    ┌──────────▼──────────┐
                    │    Transport Layer  │
                    │  (TCP, QUIC, TLS)   │
                    └─────────────────────┘
```
