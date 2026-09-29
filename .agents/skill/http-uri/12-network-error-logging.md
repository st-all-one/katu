# Network Error Logging (NEL)

## 1. Conceito

NEL é um mecanismo **experimental** que permite sites optarem por receber relatórios de falhas (e, opcionalmente, sucessos) de network fetches dos browsers.

Configurado via header de resposta `NEL`, com endpoints definidos via `Report-To`.

> "This experimental header allows websites and applications to opt-in to receive reports about failed (or even successful) network fetches from supporting browsers."

## 2. Configuração

### NEL Header (Response)

JSON-encoded:

```http
NEL: { "report_to": "nel",
       "max_age": 31556952 }
```

| Chave | Tipo | Default | Descrição |
|-------|------|---------|-----------|
| `report_to` | string | — | Grupo de reporting (definido no Report-To) |
| `max_age` | number | — | Lifetime da política (segundos) |
| `include_subdomains` | boolean | `false` | Aplica a subdomínios (apenas erros DNS) |
| `success_fraction` | float (0–1) | `0` | Proporção de sucessos a reportar |
| `failure_fraction` | float (0–1) | `1` | Proporção de falhas a reportar |

### Report-To Header

Define os endpoints de reporting:

```http
Report-To: { "group": "nel",
             "max_age": 31556952,
             "endpoints": [
               { "url": "https://example.com/nel-reports" }
             ]
           }
```

### Requisitos

- Origem considerada segura pelo browser (HTTPS)
- Grupo de reporting deve ter lifetime >= NEL policy

## 3. Formato dos Relatórios

Relatórios são enviados via Reporting API no formato:

```json
{
  "age": 20,
  "type": "network-error",
  "url": "https://example.com/previous-page",
  "body": {
    "elapsed_time": 338,
    "method": "POST",
    "phase": "application",
    "protocol": "http/1.1",
    "referrer": "https://example.com/previous-page",
    "sampling_fraction": 1,
    "server_ip": "192.0.2.172",
    "status_code": 400,
    "type": "http.error",
    "url": "https://example.com/bad-request"
  }
}
```

Campos do body:
- `elapsed_time`: tempo decorrido (ms)
- `method`: método HTTP
- `phase`: fase onde ocorreu o erro (dns, connection, application, etc.)
- `protocol`: versão do protocolo
- `referrer`: referrer da página
- `sampling_fraction`: fração de amostragem
- `server_ip`: IP do servidor
- `status_code`: código de status (0 se não houve response)
- `type`: tipo do erro

## 4. Tipos de Erro

### DNS
| Tipo | Descrição |
|------|-----------|
| `dns.unreachable` | Servidor DNS inalcançável |
| `dns.name_not_resolved` | DNS respondeu mas não resolveu o nome |
| `dns.failed` | Falha no DNS não coberta acima (ex.: SERVFAIL) |
| `dns.address_changed` | IP do servidor mudou desde o último report; dados downgraded |

### TCP
| Tipo | Descrição |
|------|-----------|
| `tcp.timed_out` | Timeout de conexão TCP |
| `tcp.closed` | Conexão fechada pelo servidor |
| `tcp.reset` | Conexão resetada |
| `tcp.refused` | Conexão recusada |
| `tcp.aborted` | Conexão abortada |
| `tcp.address_invalid` | Endereço IP inválido |
| `tcp.address_unreachable` | IP inalcançável |
| `tcp.failed` | Falha TCP não coberta acima |

### HTTP
| Tipo | Descrição |
|------|-----------|
| `http.error` | Resposta recebida com status 4xx ou 5xx |
| `http.protocol.error` | Conexão abortada por erro de protocolo HTTP |
| `http.response.invalid` | Resposta vazia, content-length mismatch, encoding inválido |
| `http.response.redirect_loop` | Loop de redirecionamento detectado |
| `http.failed` | Falha HTTP não coberta acima |

## 5. Comportamento

- **Página anterior**: o `url` no top-level do relatório é a URL da página **de onde** a requisição foi feita (previous-page), não a URL que falhou (que está em `body.url`).
- **Erros DNS sem server_ip**: quando `phase` é `dns`, não há `server_ip`.
- **Amostragem**: `failure_fraction` default 1.0 (reporta todas falhas). `success_fraction` default 0 (não reporta sucessos).

## 6. Exemplo de Erro DNS

```json
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
    "url": "https://example-host.com/"
  }
}
```

## 7. Casos de Uso

- Monitoramento de falhas de rede que não aparecem em logs do servidor (ex.: DNS failure, conexão recusada)
- Detecção de problemas de CDN/routing
- Shadow monitoring de falhas de rede do lado do cliente
- Debug de desempenho de rede

## 8. Status

**Experimental** — consulte browser compatibility antes de usar em produção.

Depende da Reporting API. Alternativa: usar navegadores com suporte a `NEL` header e relatórios via endpoints configurados.
