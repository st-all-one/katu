# Especificações e RFCs

## 1. HTTP Specifications

### Core Standards

| RFC / Spec | Título | Status | STD # |
|-----------|--------|--------|-------|
| RFC 9110 | HTTP Semantics | Internet Standard | STD 97 |
| RFC 9111 | HTTP Caching | Internet Standard | STD 98 |
| RFC 9112 | HTTP/1.1 | Internet Standard | STD 99 |
| RFC 9113 | HTTP/2 | Proposed Standard | — |
| RFC 9114 | HTTP/3 | Proposed Standard | — |

### Historical HTTP

| RFC | Título | Ano | Status |
|-----|--------|-----|--------|
| RFC 1945 | HTTP/1.0 | 1996 | Informational |
| RFC 2068 | HTTP/1.1 (first) | 1997 | Proposed Standard |
| RFC 2616 | HTTP/1.1 (second) | 1999 | Proposed Standard |
| RFC 7230-7235 | HTTP/1.1 (third) | 2014 | Proposed Standard |
| RFC 7540 | HTTP/2 (original) | 2015 | Proposed Standard |

### HTTP/2 and HTTP/3 Extensions

| RFC | Título |
|-----|--------|
| RFC 7541 | HPACK: Header Compression for HTTP/2 |
| RFC 9204 | QPACK: Header Compression for HTTP/3 |
| RFC 8446 | TLS 1.3 |
| RFC 7301 | TLS ALPN Extension |
| RFC 7838 | HTTP Alternative Services |
| RFC 6454 | The Web Origin Concept |

### Authentication & Sessions

| RFC | Título |
|-----|--------|
| RFC 6265 | HTTP State Management (Cookies) |
| RFC 7617 | The 'Basic' HTTP Authentication Scheme |
| RFC 7616 | HTTP Digest Access Authentication |
| RFC 6750 | The OAuth 2.0 Authorization Framework: Bearer Token Usage |
| RFC 7486 | HTTP Origin-Bound Authentication (HOBA) |
| RFC 8120 | Mutual Authentication Protocol |
| RFC 7804 | Salted Challenge Response HTTP Authentication (SCRAM) |
| RFC 8292 | Voluntary Application Server Identification (VAPID) |

### Caching

| RFC | Título |
|-----|--------|
| RFC 5861 | HTTP Cache-Control Extensions for Stale Content |
| RFC 8246 | HTTP Immutable Responses |
| RFC 3229 | Delta Encoding in HTTP |

### WebSocket

| RFC | Título |
|-----|--------|
| RFC 6455 | The WebSocket Protocol |

### Security

| RFC | Título |
|-----|--------|
| RFC 6797 | HTTP Strict Transport Security (HSTS) |
| RFC 7034 | HTTP Header Field X-Frame-Options |
| RFC 7239 | Forwarded HTTP Extension |
| RFC 5246 | TLS 1.2 |
| RFC 8446 | TLS 1.3 |
| RFC 2817 | Upgrading to TLS Within HTTP/1.1 |

### Status Codes

| RFC | Título |
|-----|--------|
| RFC 6585 | Additional HTTP Status Codes |
| RFC 7725 | An HTTP Status Code to Report Legal Obstacles (451) |
| RFC 2324 | HTCPCP/1.0 (418 I'm a Teapot) — April Fools |
| RFC 7168 | HTCPCP-TEA — April Fools |

### Compression

| RFC | Título |
|-----|--------|
| RFC 9842 | Compression Dictionary Transport |

### Content Negotiation & MIME

| RFC | Título |
|-----|--------|
| RFC 6838 | Media Type Specifications and Registration |
| RFC 7578 | Returning Values from Forms: multipart/form-data |
| RFC 6266 | Use of the Content-Disposition Header |

### URI

| RFC | Título | STD # |
|-----|--------|-------|
| RFC 3986 | URI: Generic Syntax | STD 66 |
| RFC 3987 | Internationalized Resource Identifiers (IRI) | Proposed Standard |
| RFC 8141 | Uniform Resource Names (URN) | Proposed Standard |
| RFC 2397 | The "data" URL scheme | Proposed Standard |

## 2. Living Standards (WHATWG)

| Spec | Descrição |
|------|-----------|
| Fetch | Fetch API / CORS Protocol |
| HTML | HTML Living Standard (inclui `javascript:` special case) |
| MIME Sniffing | MIME Sniffing Standard |
| URL | URL Living Standard |

## 3. W3C Specifications

| Spec | Status |
|------|--------|
| Content Security Policy Level 3 | W3C Working Draft |
| Media Fragments URI 1.0 | W3C Recommendation |
| CSS Pseudo-Elements Module Level 4 | W3C Working Draft |
| Upgrade Insecure Requests | W3C Candidate Recommendation |
| Trusted Types | W3C Working Draft |
| WebAppSec Fetch Metadata Request Headers | W3C Working Draft |

## 4. WICG Drafts

| Spec | Descrição |
|------|-----------|
| Scroll-to-Text Fragment | Text Fragments specification |
| Permissions Policy | Permissions Policy specification |
| Client Hints Infrastructure | User-Agent Client Hints |
| Anonymous IFrame | iframe credentialless |
| Reporting API | Report-To header, NEL |

## 5. IANA Registries

| Registry | Descrição |
|----------|-----------|
| URI Schemes | Registro oficial de todos os schemes |
| URN Namespaces | Registro de namespaces (NIDs) |
| HTTP Status Codes | Registro de status codes |
| HTTP Method Registry | Registro de métodos HTTP |
| HTTP Headers | Registro de headers HTTP |

## 6. Referências Cruzadas

```
┌─────────────────────────────────────────────────────┐
│                    Web Platform                     │
├─────────────────────────────────────────────────────┤
│  ┌──────────────────┐    ┌──────────────────────┐   │
│  │  HTTP Semantics   │    │   URI RFC 3986       │   │
│  │  (RFC 9110)       │◄──►│   (STD 66)           │   │
│  └────────┬─────────┘    └──────────┬───────────┘   │
│           │                         │               │
│  ┌────────▼─────────┐    ┌──────────▼───────────┐   │
│  │  HTTP/1.1        │    │  Schemes             │   │
│  │  HTTP/2          │    │  (http://, data:,    │   │
│  │  HTTP/3          │    │   blob:, urn:, etc.) │   │
│  └──────────────────┘    └──────────────────────┘   │
│                                                      │
│  ┌──────────────────────────────────────────────┐   │
│  │  Security Layer                               │   │
│  │  CORS (Fetch)  │  CSP (W3C)  │  HSTS (RFC    │   │
│  │  CORP           │  COOP       │   6797)       │   │
│  │  Permissions    │  Fetch MD   │   TLS 1.3     │   │
│  └──────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────┘
```
