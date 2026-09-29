# HTTP Client Hints

## 1. Conceito

Client Hints são headers de request que o servidor pode **solicitar ativamente** do cliente para obter informações sobre dispositivo, rede, usuário e preferências do agente.

Diferente do `User-Agent` tradicional (enviado sempre, cheio de dados irrelevantes), Client Hints são:
- **Opt-in**: servidor pede, cliente decide se envia
- **Granulares**: apenas o que o servidor precisa
- **Privacy-preserving**: cliente controla o que compartilha

## 2. Fluxo

```
Request 1 (browser → servidor) — sempre envia low entropy hints:
  Sec-CH-UA: "Chrome";v="143", "Chromium";v="143"
  Sec-CH-UA-Platform: "Android"
  Sec-CH-UA-Mobile: ?1
  User-Agent: Mozilla/5.0 (...)

Response 1 (servidor → browser):
  Accept-CH: Sec-CH-UA-Model, Sec-CH-UA-Form-Factors

Request 2+ (browser → servidor) — adiciona hints solicitados:
  Sec-CH-UA-Model: "Pixel 9"
  Sec-CH-UA-Form-Factors: "Mobile"
```

O servidor também pode especificar Client Hints via HTML:
```html
<meta http-equiv="Accept-CH" content="Width, Downlink, Sec-CH-UA">
```

## 3. Low Entropy Hints (sempre enviados)

São hints que revelam pouca informação para fingerprinting. Enviados por padrão em toda requisição:

| Header | Exemplo | Descrição |
|--------|---------|-----------|
| `Sec-CH-UA` | `"Chrome";v="143"` | Browser + versão major |
| `Sec-CH-UA-Platform` | `"Android"` | SO/plataforma |
| `Sec-CH-UA-Mobile` | `?1` | Mobile (`?1`) ou não (`?0`) |
| `Save-Data` | `on` | Preferência por economia de dados |

## 4. High Entropy Hints (solicitados via Accept-CH)

Revelam mais informações. Cliente decide se fornece (baseado em preferências, permissão ou Permissions Policy).

### User-Agent Client Hints
| Header | Exemplo | Descrição |
|--------|---------|-----------|
| `Sec-CH-UA-Arch` | `"x86"` | Arquitetura da CPU |
| `Sec-CH-UA-Bitness` | `"64"` | "64" ou "32" bits |
| `Sec-CH-UA-Full-Version-List` | `"Chrome";v="143.0.1234.56"` | Versão completa |
| `Sec-CH-UA-Model` | `"Pixel 9"` | Modelo do dispositivo |
| `Sec-CH-UA-Form-Factors` | `"Mobile"` | Fator de forma |
| `Sec-CH-UA-Platform-Version` | `"15.0.0"` | Versão do SO |
| `Sec-CH-UA-WoW64` | `?0` | Se executa em WoW64 |

### User Preference Media Features
| Header | Exemplo | Descrição |
|--------|---------|-----------|
| `Sec-CH-Prefers-Reduced-Motion` | `"reduce"` | Prefere animações reduzidas |
| `Sec-CH-Prefers-Color-Scheme` | `"dark"` | Prefere tema escuro |
| `Sec-CH-Prefers-Reduced-Transparency` | `"reduce"` | Prefere transparência reduzida |
| `Sec-CH-Prefers-Reduced-Data` | `"reduce"` | Prefere dados reduzidos |

### Device Client Hints
| Header | Exemplo | Descrição |
|--------|---------|-----------|
| `Sec-CH-Device-Memory` | `"8"` | Memória RAM (GB aproximado) |
| `Sec-CH-DPR` | `"2.0"` | Device Pixel Ratio |
| `Sec-CH-Viewport-Height` | `"1080"` | Viewport height (px) |
| `Sec-CH-Viewport-Width` | `"1920"` | Viewport width (px) |
| `Width` | `"1920"` | Largura do resource width (px) |

### Network Client Hints
| Header | Exemplo | Descrição |
|--------|---------|-----------|
| `Downlink` | `"5.0"` | Largura de banda (Mbps) |
| `ECT` | `"4g"` | Effective Connection Type |
| `RTT` | `"100"` | Round-trip time (ms) |

## 5. Critical Client Hints

Hints que o servidor **precisa** para renderizar corretamente. Definidos via header `Critical-CH`.

Requisição adicional é feita se o hint crítico não estava presente na requisição original:

```
Response:
  Accept-CH: Sec-CH-Prefers-Reduced-Motion
  Critical-CH: Sec-CH-Prefers-Reduced-Motion
  Vary: Sec-CH-Prefers-Reduced-Motion

Browser verifica: "Sec-CH-Prefers-Reduced-Motion" não estava no request original
→ retry automático com o header incluso
```

> "Accept-CH requests all values you'd like for the page, while Critical-CH requests only the subset you must have on-load."

## 6. Cache e Vary

Cada hint que altera o recurso servido deve ser incluído em `Vary`:
```
Vary: Sec-CH-UA-Model, Sec-CH-UA-Form-Factors
```

**Cuidado**: hints de rede como `Downlink` e `RTT` mudam frequentemente. Incluí-los em `Vary` pode tornar o recurso effectively uncacheable (nova entrada para cada valor).

## 7. Lifetime

O conjunto de hints solicitados via `Accept-CH` persiste até o browser ser fechado. Para limpar:
```
Clear-Site-Data: "clientHints"
```

Ou reenviar `Accept-CH` com lista vazia.

## 8. Uso para Responsive Design

Client Hints podem substituir ou complementar media queries quando não há controle individual de stylesheets:

```
# Mobile phone
Sec-CH-UA: "Chrome";v="143"
Sec-CH-UA-Platform: "Android"
Sec-CH-UA-Mobile: ?1

# Tablet
Sec-CH-UA: "Chrome";v="143"
Sec-CH-UA-Platform: "Android"
Sec-CH-UA-Mobile: ?0
```

`Sec-CH-UA-Mobile` é o indicador chave para diferenciar mobile de tablet/desktop.

## 9. Relação com User-Agent Reduction

Client Hints são a alternativa moderna à string User-Agent. UA Client Hints são mais eficientes e privadas que o `User-Agent` tradicional, e devem substituí-lo.

Headers UA tradicionais reduzidos (User-Agent Reduction):
- Versão exata do SO → removida
- Modelo do dispositivo → removido
- Versão minoritária do browser → zeroed

Disponíveis via Client Hints de alta entropia (quando solicitados e permitidos).
