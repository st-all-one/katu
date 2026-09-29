# HTTP Authentication — Framework Completo

## 1. O Framework HTTP Authentication (RFC 7235)

HTTP define um framework genérico para controle de acesso e autenticação. O fluxo é:

1. Cliente faz request sem credenciais
2. Servidor responde com **401 Unauthorized** + header `WWW-Authenticate`
3. Cliente (geralmente via prompt ao usuário) reenvia com header `Authorization`
4. Servidor valida e retorna recurso ou 403 Forbidden

```
Request (sem auth) → 401 + WWW-Authenticate → Request com Authorization → 200 + recurso
```

### Proxy Authentication

Mesmo mecanismo, mas com headers e status específicos:

| Recurso | Proxy |
|---------|-------|
| 401 Unauthorized | 407 Proxy Authentication Required |
| `WWW-Authenticate` | `Proxy-Authenticate` |
| `Authorization` | `Proxy-Authorization` |

Ambos podem coexistir: servidor e proxy podem exigir autenticação independente.

### Access Forbidden

- **Credenciais inválidas** → 401 (ou 407)
- **Credenciais válidas mas insuficientes** → 403 Forbidden (browser não tenta novamente)
- **Ocultar existência do recurso** → 404 Not Found

## 2. Headers de Autenticação

### WWW-Authenticate / Proxy-Authenticate (Response)

```http
WWW-Authenticate: <scheme> realm=<realm>
Proxy-Authenticate: <scheme> realm=<realm>
```

- `<scheme>`: esquema de autenticação (Basic, Bearer, Digest, etc.)
- `realm`: descrição da área protegida (ex.: "Access to the staging site")

### Authorization / Proxy-Authorization (Request)

```http
Authorization: <scheme> <credentials>
Proxy-Authorization: <scheme> <credentials>
```

## 3. Esquemas de Autenticação

| Scheme | RFC | Segurança | Uso Principal |
|--------|-----|-----------|---------------|
| **Basic** | RFC 7617 | ⚠️ Inseguro sem HTTPS | Legado, evitar |
| **Bearer** | RFC 6750 | ✅ Com HTTPS | OAuth 2.0 tokens |
| **Digest** | RFC 7616 | ✅ (MD5/SHA-256) | Autenticação HTTP |
| **HOBA** | RFC 7486 | ✅ | Origin-Bound Auth (signature-based) |
| **Mutual** | RFC 8120 | ✅ | Autenticação mútua |
| **Negotiate/NTLM** | RFC 4559 | ⚠️ | Windows/AD |
| **VAPID** | RFC 8292 | ✅ | Web Push |
| **SCRAM** | RFC 7804 | ✅ | Salted Challenge Response |
| **AWS4-HMAC-SHA256** | AWS | ✅ | AWS API |

### Basic Auth (RFC 7617)

Codifica `username:password` em base64. **Não é criptografia** — base64 é reversível.

```http
WWW-Authenticate: Basic realm="Access to staging"
Authorization: Basic YWxhZGRpbjpvcGVuIHNlc2FtZQ==
```

**Riscos**:
- Credenciais em texto plano (apenas ofuscadas)
- Vulnerável a CSRF (credenciais enviadas em toda requisição independente de origem)
- **NUNCA usar sem HTTPS**

**Mitigação**: sempre usar HTTPS + POST para mudanças de estado + CSRF tokens.

### Bearer Token (RFC 6750)

```http
Authorization: Bearer eyJhbGciOiJIUzI1NiIs...
```

Padrão OAuth 2.0. Token geralmente JWT. Exige HTTPS.

### Digest Auth (RFC 7616)

```http
WWW-Authenticate: Digest realm="testrealm",
  nonce="dcd98b7102dd2f0e8b11d0f600bfb0c093",
  algorithm=SHA-256,
  qop=auth

Authorization: Digest username="Mufasa",
  realm="testrealm",
  nonce="dcd98b7102dd2f0e8b11d0f600bfb0c093",
  uri="/dir/index.html",
  response="e258e0e3a8d8d2a0d3f0...",
  algorithm=SHA-256
```

Usa hash MD5 ou SHA-256. Firefox 93+ suporta SHA-256. MD5 não recomendado.

## 4. Configuração Server-Side

### Apache (.htaccess)

```apache
AuthType Basic
AuthName "Access to staging site"
AuthUserFile /path/to/.htpasswd
Require valid-user
```

`.htpasswd` (usuário:hash):
```
aladdin:$apr1$ZjTqBB3f$IF9gdYAGlMrs2fuINjHsz.
```

### Nginx

```nginx
location /status {
    auth_basic "Access to staging site";
    auth_basic_user_file /etc/apache2/.htpasswd;
}
```

## 5. Cross-Origin Images and Auth

Desde Firefox 59, imagens cross-origin não disparam diálogos de autenticação HTTP, prevenindo roubo de credenciais via embedding de imagens arbitrárias em páginas third-party.

## 6. Character Encoding

Browsers usam `utf-8` para usernames e passwords (Firefox usava ISO-8859-1, mudou para utf-8).

## 7. URL-Embedded Credentials (Deprecated)

```url
https://username:password@www.example.com/
```

**Descontinuado**. Browsers modernos removem user:password da URL antes de enviar a requisição.
