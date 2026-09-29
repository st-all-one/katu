# Cookies HTTP — Detalhamento

## 1. Fluxo Básico

```
Servidor envia:  Set-Cookie: <name>=<value>; <attributes>
Cliente envia:   Cookie: <name>=<value>
```

### Exemplo
```
Response:
  Set-Cookie: yummy_cookie=chocolate
  Set-Cookie: tasty_cookie=strawberry

Request posterior:
  Cookie: yummy_cookie=chocolate; tasty_cookie=strawberry
```

### Atualização
Para atualizar, servidor envia novo `Set-Cookie` com mesmo nome, path e domain.

### Remoção
Enviar `Set-Cookie` com mesmo nome, path e domain, e `Max-Age=0` ou `Expires` no passado.

## 2. Tipos de Cookie

### Session Cookies
Sem `Max-Age` ou `Expires`. Deletados quando a sessão do browser termina.

### Permanent Cookies
Com `Max-Age` ou `Expires`. Persistem até a data especificada.

```http
Set-Cookie: id=a3fWa; Max-Age=2592000
Set-Cookie: id=a3fWa; Expires=Thu, 31 Oct 2021 07:28:00 GMT
```

> `Expires` é mais antigo. `Max-Age` menos propenso a erros de fuso horário. `Max-Age` tem precedência quando ambos presentes.

## 3. Atributos de Segurança

| Atributo | Proteção | Comportamento |
|----------|----------|---------------|
| `Secure` | Eavesdropping/MITM | Só enviado em HTTPS (nunca em HTTP) |
| `HttpOnly` | XSS | Inacessível via JavaScript (`Document.cookie`) |
| `SameSite=Strict` | CSRF | Só enviado em requests same-site |
| `SameSite=Lax` (default) | CSRF | Enviado em navegações top-level GET |
| `SameSite=None` | — | Enviado cross-site (exige `Secure`) |

```http
Set-Cookie: session=abc123; Secure; HttpOnly; SameSite=Strict
```

### SameSite Detalhamento

**`Strict`**: cookie nunca enviado em requests cross-site. Ideal para autenticação.

**`Lax`**: enviado em navegações top-level GET (ex.: clicar em link de outro site). Default quando nenhum SameSite é especificado.

**`None`**: enviado em todas requisições cross-site. Exige `Secure`. Usado por ad-tech, analytics, widgets third-party.

> Browsers estão bloqueando third-party cookies por padrão. Desenvolvedores devem reduzir dependência.

## 4. Escopo: Domain e Path

### Domain
- Se **não especificado**: cookie disponível apenas no servidor que o definiu (e não em subdomínios)
- Se **especificado** (`Domain=mozilla.org`): cookie disponível no domínio e todos subdomínios
- Servidor só pode definir Domain para seu próprio domínio ou parent (não para subdomínio ou domínios diferentes)

### Path
Define prefixo de URL que deve corresponder. Subdiretórios também correspondem.

```http
Set-Cookie: id=a3fWa; Path=/docs
```
Corresponde a: `/docs`, `/docs/`, `/docs/Web/`, `/docs/Web/HTTP`
Não corresponde a: `/`, `/docsets`, `/fr/docs`

> Path **não é** uma medida de segurança. Não impede leitura de cookie de outro path via JavaScript.

## 5. Cookie Prefixes (Defense-in-depth)

| Prefixo | Requisitos | Proteção Contra |
|---------|------------|-----------------|
| `__Secure-` | Secure + HTTPS | Session fixation parcial |
| `__Host-` | Secure + HTTPS + Path=/ + sem Domain | Session fixation (escopo = origin) |
| `__Http-` | Secure + HTTPS + HttpOnly | Session fixation + XSS |
| `__Host-Http-` | Secure + HTTPS + HttpOnly + Path=/ + sem Domain | Session fixation + XSS + escopo origin |

```http
Set-Cookie: __Host-session=abc123; Secure; Path=/
```

Browser rejeita cookies com prefixo se os requisitos não forem atendidos.

## 6. Remoção em Massa

```http
# Remove todos cookies do domínio registrável
Clear-Site-Data: "cookies"
```

## 7. Zombie Cookies

Técnicas que recriam cookies após exclusão. Violam privacidade e regulamentações (GDPR, ePrivacy, CCPA).

## 8. Privacidade e Regulamentações

### Third-party Cookies
Cookies definidos por conteúdo embutido via `<iframe>`. Usados para tracking entre sites. Browsers estão bloqueando por padrão.

### Regulamentações
- **GDPR** (EU): notificar usuário, permitir opt-out
- **ePrivacy Directive** (EU): consentimento para cookies não essenciais
- **CCPA** (California): direito de opt-out

### Boas Práticas
- Regenerar e reenviar session cookies a cada autenticação (previne session fixation)
- Usar `HttpOnly` + `Secure` + `SameSite=Strict` para cookies de sessão
- Preferir `__Host-` prefix para máxima segurança de escopo
- Não usar cookies para armazenamento geral (preferir Web Storage API / IndexedDB)
- Cookies: ~4KB max, centenas por domínio max

## 9. Casos de Uso

| Propósito | Cookie | Atributos |
|-----------|--------|-----------|
| Sessão de usuário | `session=abc123` | `Secure; HttpOnly; SameSite=Strict; __Host-` |
| Preferência de idioma | `lang=pt-BR` | `Secure; SameSite=Lax` |
| Affiliate tracking | `affiliate=e4rt45dw` | `SameSite=Lax` |
| Analytics third-party | `analytics=xyz` | `SameSite=None; Secure` |
