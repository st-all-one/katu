# Proxies e Tunneling HTTP

## 1. Tipos de Proxy

### Forward Proxy (Proxy direto)
Atua em nome do **cliente**. Esconde identidade do cliente; servidor vê IP do proxy.

Casos de uso: anonimato (Tor), controle de banda, cache de grupo, filtragem de conteúdo.

### Reverse Proxy
Atua em nome do **servidor**. Esconde identidade do servidor; cliente não sabe qual servidor processou a requisição.

Casos de uso:
- **Load balancing**: distribuir carga entre servidores
- **Cache de conteúdo estático**: offload do servidor web
- **Compressão**: otimizar conteúdo antes de enviar
- **SSL termination**: descriptografar TLS antes do backend
- **Autenticação**: gate de acesso unificado

```
Cliente → Forward Proxy → Internet → Servidor
Cliente → Internet → Reverse Proxy → Servidor(s) interno(s)
```

## 2. Headers de Proxy

### Padrão (RFC 7239)
```
Forwarded: for=192.0.2.60; proto=https; by=203.0.113.43
```
- `for`: IP original do cliente
- `proto`: protocolo original (http/https)
- `by`: proxy que adicionou o header
- `host`: host original

### Não Padrão (de facto)
```
X-Forwarded-For: <IP do cliente>
X-Forwarded-Host: <host original>
X-Forwarded-Proto: <http|https>
```

### Via Header
Identifica o proxy em si (não o cliente):
```
Via: 1.1 proxy.example.com
```

Pode aparecer tanto em request quanto response headers. Múltiplos proxies empilham:
```
Via: 1.0 proxy-a.example.com, 1.1 proxy-b.example.com
```

## 3. HTTP Tunneling via CONNECT

O método `CONNECT` estabelece um túnel TCP bidirecional através de um proxy.

```
Request:  CONNECT example.com:443 HTTP/1.1
          Host: example.com:443

Response: 200 Connection Established
```

**Fluxo típico (HTTPS via proxy)**:
1. Cliente envia `CONNECT host:443`
2. Proxy estabelece conexão TCP com `host:443`
3. Proxy responde `200 Connection Established`
4. Cliente inicia handshake TLS diretamente com o host (toda comunicação subsequente é encapsulada no túnel)

**Restrições**:
- Funciona apenas em HTTP/1.1 (não em HTTP/2+)
- Muitos proxies limitam a porta 443 (HTTPS)
- Proxy não pode inspecionar o conteúdo (tráfego criptografado)

## 4. Proxy Auto-Configuration (PAC)

Arquivo JavaScript que determina se o browser deve usar proxy para cada URL.

### Formato
```js
function FindProxyForURL(url, host) {
  // Retorna string de configuração
  return "PROXY proxy.example.com:8080; DIRECT";
}
```

### Valores de Retorno
| Valor | Comportamento |
|-------|---------------|
| `DIRECT` | Sem proxy |
| `PROXY host:port` | Proxy HTTP |
| `SOCKS host:port` | Servidor SOCKS |
| `HTTP host:port` | Proxy HTTP (Firefox) |
| `HTTPS host:port` | Proxy HTTPS (Firefox) |
| `SOCKS4 host:port` | SOCKS4 (Firefox) |
| `SOCKS5 host:port` | SOCKS5 (Firefox) |

Múltiplos valores separados por `;`: fallback se o primeiro falhar.
```
PROXY w3proxy:8080; PROXY mozilla:8081; DIRECT
```

### Funções Pré-definidas do PAC
```
# Baseadas em hostname
isPlainHostName(host)     → true se não tem domínio (sem dots)
dnsDomainIs(host, dom)    → true se host está no domínio
localHostOrDomainIs(h, d) → true se match exato ou hostname simples
isResolvable(host)        → true se DNS resolve
isInNet(host, pat, mask)  → true se IP está na sub-rede

# Utilidades
dnsResolve(host)          → resolve hostname para IP
myIpAddress()             → IP da máquina
dnsDomainLevels(host)     → número de dots no hostname
shExpMatch(str, exp)      → shell glob match
weekdayRange(wd1, wd2)    → range de dias da semana
dateRange(...)            → range de datas
timeRange(...)            → range de horas
alert(msg)                → log no console
```

### Exemplos

**Proxy para tudo exceto hosts locais**:
```js
function FindProxyForURL(url, host) {
  if (isPlainHostName(host) || dnsDomainIs(host, ".mozilla.org")) {
    return "DIRECT";
  }
  return "PROXY w3proxy.mozilla.org:8080; DIRECT";
}
```

**Proxy específico por protocolo**:
```js
function FindProxyForURL(url, host) {
  if (url.startsWith("http:"))
    return "PROXY http-proxy:8080";
  if (url.startsWith("https:") || url.startsWith("snews:"))
    return "PROXY security-proxy:8080";
  return "DIRECT";
}
```

**Configuração**: MIME type `application/x-ns-proxy-autoconfig`, arquivo `.pac`.

## 5. Headers Hop-by-Hop vs End-to-End

**End-to-end**: devem ser transmitidos ao destinatário final (cliente ou servidor). Proxies retransmitem sem modificar.

**Hop-by-hop**: significativos apenas para uma conexão. **Não** retransmitidos por proxies.

Apenas headers hop-by-hop podem ser definidos via header `Connection`:
```http
Connection: Upgrade
Connection: close
Connection: Keep-Alive
```

Lista de headers hop-by-hop padrão: `Connection`, `Keep-Alive`, `Proxy-Authenticate`, `Proxy-Authorization`, `TE`, `Trailer`, `Transfer-Encoding`, `Upgrade`.

## 6. Proxy Authentication

Requisições a proxies autenticados usam headers específicos (separados da autenticação do servidor de origem):

| Header | Direção | Propósito |
|--------|---------|-----------|
| `Proxy-Authenticate` | Response | Desafio do proxy (407) |
| `Proxy-Authorization` | Request | Credenciais para o proxy |
| `407 Proxy Authentication Required` | Status | Proxy exige autenticação |

Fluxo:
```
Cliente → Request → Proxy → Servidor
                    ← 407 Proxy Authentication Required
                         Proxy-Authenticate: Basic realm="proxy"
Cliente → Request (com Proxy-Authorization) → Proxy → Servidor
                    ← Response
```
