# URI Moderno: Padrões e Práticas

## 1. Schemes Especiais

### 1.1 `blob:` URLs

**Propósito**: representar objetos binários (Blob, MediaSource) como URLs para uso com elementos que esperam URLs (`<img>`, `<video>`, etc.).

**Sintaxe**:
```
blob:<origin>/<uuid>
```

**Diferença para `data:`**:
- `data:` embute o dado na URL (limite de ~512 MB)
- `blob:` aponta para objeto em memória (tamanho flexível)

**Gerenciamento de Memória**:
```js
// Criação
const url = URL.createObjectURL(blob);
img.src = url;

// Liberação (quando recurso não for mais necessário)
URL.revokeObjectURL(url);
```

> "Each time you call `createObjectURL()`, a new object URL is created, even if you've already created one for the same object. Each of these must be released by calling `URL.revokeObjectURL()` when you no longer need them."

**Anti-pattern**: revogar imediatamente após `load` do elemento. Isso quebra interações como "salvar imagem como...".

**Storage Partitioning**: blob URLs só podem ser acessadas de ambientes com mesma chave de armazenamento do criador.

**MediaStream**: usar `srcObject` em vez de `createObjectURL()` (deprecated para streams).

### 1.2 `data:` URLs

**Propósito**: embutir dados inline no documento.

**Sintaxe**:
```
data:[<media-type>][;base64],<data>
```

- `<media-type>` default: `text/plain;charset=US-ASCII`
- `;base64`: codificação base64

**Exemplos**:
```
data:,Hello%2C%20World%21
data:text/plain;base64,SGVsbG8sIFdvcmxkIQ==
data:text/html,%3Ch1%3EHello%3C%2Fh1%3E
data:image/svg+xml,%3Csvg%20xmlns%3D...%3E%3C%2Fsvg%3E
```

**Segurança**:
- Top-level navigation para `data:` é bloqueado em todos browsers modernos
- Origin opaca (não herda contexto de navegação)
- Não suporta query strings

**Limitações**:
- Chromium/Firefox: ~512 MB
- Safari: ~2048 MB
- Erros em media-type são ignorados silenciosamente

### 1.3 `javascript:` URLs

> **⚠️ DESENCORAJADO**: "Using `javascript:` URLs on the web is discouraged as it may lead to execution of arbitrary code, similar to the ramifications of using `eval()`. It may also reduce accessibility because it deviates from normal link behavior."

**Sintaxe**: `javascript:<script>`

**Comportamento perigoso**: se a última expressão retornar uma string, o navegador **navega para novo documento** com aquela string como HTML.

**Onde usar vs evitar**:

| Contexto | Permite `javascript:`? | Alternativa |
|----------|----------------------|-------------|
| `<a href>` | ✅ (não recomendado) | `<button>` + event listener |
| `<form action>` | ✅ (não recomendado) | Event listener + `preventDefault()` |
| `<iframe src>` | ✅ (não recomendado) | `srcdoc` attribute |
| `<link href>` | ❌ (bloqueado) | `data:text/javascript` |
| `window.location` | ✅ (não recomendado) | DOM API |

**CSP bloqueia**: políticas `script-src` podem bloquear `javascript:` navigation.

### 1.4 `resource:` URLs (Firefox-only)

> **Não padrão**: "resource: is not defined in any specification."

Protocolo interno do Firefox para carregar recursos built-in e de extensões.

**Risco de segurança crítico** (Firefox ≤ 56):
- Sites podiam ler configurações internas via `resource:///defaults/preferences/firefox.js`
- Fingerprinting via diferenças de config entre plataformas/locales

**Mitigação (Firefox 57+)**: acesso negado por padrão; recursos essenciais movidos para `resource://content-accessible/`.

### 1.5 `urn:` URLs

**Propósito**: identificar recursos por nome, não por localização.

**Sintaxe** (RFC 8141): `urn:<NID>:<NSS>`

**Exemplos**:
- `urn:isbn:9780141036144` → livro "1984" de George Orwell
- `urn:ietf:rfc:7230` → RFC 7230 (HTTP/1.1)

## 2. Fragmentos Avançados

### 2.1 Text Fragments

Link direto para texto específico em página web, sem necessidade de IDs definidos pelo autor.

**Sintaxe**:
```
https://example.com#:~:text=[prefix-,]textStart[,textEnd][,-suffix]
```

| Componente | Obrigatório | Descrição |
|-----------|-------------|-----------|
| `:~:` | ✅ | Fragment directive (instrução user-agent) |
| `text=` | ✅ | Inicia text fragment |
| `textStart` | ✅ | Início do texto |
| `textEnd` | ❌ | Fim do texto (cria intervalo) |
| `prefix-` | ❌ | Texto que deve preceder o trecho |
| `-suffix` | ❌ | Texto que deve suceder o trecho |

**Exemplos**:
```
#:~:text=use                    → primeira ocorrência de "use"
#:~:text=avoid-,use             → segunda ocorrência (prefixada por "avoid")
#:~:text=human,URL              → texto de "human" até "URL"
#:~:text=linked%20URL,-'s%20format  → "linked URL" seguido de "'s format"
```

**Múltiplos fragmentos**: separe com `&`:
```
#:~:text=Causes&text=linked
```

**Regras**:
- Case-insensitive
- Prefix/suffix/text no mesmo block-level element
- Cross-origin requer `rel="noopener"`
- Apenas navegações iniciadas pelo usuário
- Main frame apenas (não funciona em iframes)
- Estilização com CSS `::target-text`

**Opt-out**:
```
Document-Policy: force-load-at-top
```

### 2.2 Media Fragments

Fragmentos para mídia (vídeo, áudio, SVG).

**Temporal** (W3C Media Fragments):
```
#t=[npt:][timeStart][,timeEnd]
```

Formatos de tempo: segundos (`0-5`), `hh:mm:ss`, `mm:ss`.

| Exemplo | Comportamento |
|---------|---------------|
| `#t=0,5` | 0s a 5s |
| `#t=,5` | Início a 5s |
| `#t=2` | 2s até o fim |
| `#t=2,3.5` | 2s a 3.5s |
| `#t=0:00:02,00:03.5` | 2s a 3.5s (notação horária) |

**Espacial**:
```
#xywh=[unit:]xCoord,yCoord,width,height
```

Unidades: `pixel:` (default) ou `percent:`.

**Suporte**: temporal funciona em todos browsers modernos. Espacial apenas no Firefox 147+.

## 3. www vs non-www

**Regra**: escolha um domínio canônico e mantenha.

### Técnica 1: 301 Redirect (recomendado)

```
http://www.example.org/whaddup → 301 Moved Permanently
Location: http://example.org/whaddup
```

### Técnica 2: `<link rel="canonical">`

```html
<link href="http://example.org/whaddup" rel="canonical">
```

Serve mesmo conteúdo em ambos domínios; browser history trata como entradas independentes.

### Técnica 3: Ambos (recomendado)

Configure servidor para responder em ambos domínios, com redirect do não-canônico para o canônico.

## 4. Matriz de Decisão de Schemes

| Cenário | Scheme | Por quê |
|---------|--------|---------|
| Site/página web | `https://` | Segurança, SEO, features modernas |
| API REST | `https://` | Padrão universal |
| Imagem inline pequena | `data:image/...` | Evita request extra (use com moderação) |
| Arquivo binário grande em memória | `blob:` | Sem limite de tamanho prático |
| Identificador persistente | `urn:` | Imutável, independente de localização |
| Ação JS em link | Event listener | Acessível, segura, sem side effects |
| Recurso interno Firefox | API de extensão | `resource:` depreciado e inseguro |
| WebSocket | `wss://` | Conexão bidirecional segura |
