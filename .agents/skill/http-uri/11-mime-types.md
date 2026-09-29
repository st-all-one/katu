# MIME Types (Media Types)

## 1. Definição

MIME (Multipurpose Internet Mail Extensions) type indica a natureza e formato de um documento, arquivo ou conjunto de bytes. Definido na RFC 6838.

> "Browsers use the MIME type, not the file extension, to determine how to process a URL."

O header `Content-Type` é o mecanismo de transporte do MIME type na web.

## 2. Estrutura

```
type/subtype;parameter=value
```

Exemplos:
- `text/html; charset=utf-8`
- `application/json`
- `image/png`
- `multipart/form-data; boundary=----boundary123`

- **type**: categoria geral (text, image, video, etc.)
- **subtype**: formato específico (html, png, mp4, etc.)
- **parameter**: opcional (charset, boundary, codecs, etc.)

Case-insensitive, mas tradicionalmente lowercase.

## 3. Tipos Discretos

| Tipo | Descrição | Exemplos |
|------|-----------|----------|
| `application` | Dados binários que não se encaixam em outros tipos | `application/json`, `application/pdf`, `application/zip`, `application/octet-stream` |
| `audio` | Dados de áudio ou música | `audio/mpeg`, `audio/ogg`, `audio/wav` |
| `example` | Placeholder para exemplos (nunca usar em produção) | `audio/example` |
| `font` | Dados de fonte/tipografia | `font/woff`, `font/woff2`, `font/ttf`, `font/otf` |
| `image` | Dados de imagem (bitmap e vetor) | `image/jpeg`, `image/png`, `image/webp`, `image/svg+xml` |
| `model` | Dados de modelo 3D | `model/3mf`, `model/vrml` |
| `text` | Dados textuais legíveis | `text/html`, `text/css`, `text/javascript`, `text/plain`, `text/csv` |
| `video` | Dados de vídeo | `video/mp4`, `video/webm`, `video/ogg` |

### Tipos Default Importantes

**`application/octet-stream`**: binário desconhecido. Browsers não executam; forçam "Save As". É o fallback universal.

**`text/plain`**: texto desconhecido. Browsers exibem inline. **Não** deve ser usado para CSS, JS ou HTML — o navegador não interpretará corretamente.

## 4. Tipos Multipart

Documentos compostos de múltiplas partes, cada uma com seu próprio MIME type.

### multipart/form-data
Usado em formulários HTML com método POST e `enctype="multipart/form-data"`.

```http
Content-Type: multipart/form-data; boundary=boundaryString

--boundaryString
Content-Disposition: form-data; name="myFile"; filename="img.jpg"
Content-Type: image/jpeg

(data)
--boundaryString
Content-Disposition: form-data; name="myField"

(data)
--boundaryString--
```

### multipart/byteranges
Usado com `206 Partial Content` para enviar partes específicas de um recurso:

```http
Content-Type: multipart/byteranges; boundary=3d6b6a416f9b5

--3d6b6a416f9b5
Content-Type: text/html
Content-Range: bytes 100-200/1270

(partial data)
--3d6b6a416f9b5
Content-Type: text/html
Content-Range: bytes 300-400/1270

(partial data)
--3d6b6a416f9b5--
```

## 5. MIME Types Importantes para Web

| MIME Type | Recurso | Notas |
|-----------|---------|-------|
| `text/html` | HTML | Todo HTML deve usar este tipo |
| `text/css` | CSS | Se enviado como `text/plain`, browser ignora |
| `text/javascript` | JavaScript | Único garantido para funcionar |
| `application/json` | JSON | Padrão para APIs REST |
| `application/pdf` | PDF | - |
| `application/zip` | ZIP | - |
| `image/jpeg` | JPEG | JPEG photos |
| `image/png` | PNG | Gráficos com transparência |
| `image/webp` | WebP | Imagem moderna com compressão superior |
| `image/avif` | AVIF | Formato de imagem mais recente |
| `image/svg+xml` | SVG | Vetores |
| `image/gif` | GIF | Animações simples |
| `audio/mpeg` | MP3 | Áudio |
| `video/mp4` | MP4 | Vídeo |
| `font/woff2` | WOFF2 | Fonte web moderna |

**JavaScript Legacy MIME types** (evitar; usar apenas `text/javascript`):
- `application/javascript`, `application/ecmascript`, `application/x-javascript`
- `text/ecmascript`, `text/javascript1.0`–`1.5`, `text/jscript`, `text/livescript`

## 6. Parâmetro codecs em MIME de Mídia

Para áudio/vídeo, o parâmetro `codecs` especifica codecs usados:
```
video/mp4; codecs="avc1.64001E, mp4a.40.2"
audio/ogg; codecs="vorbis"
```

## 7. MIME Sniffing

Na ausência de MIME type ou quando o browser suspeita que está incorreto, ele pode tentar adivinhar o tipo analisando os bytes do recurso. Isso é chamado de **MIME sniffing** (definido no WHATWG MIME Sniffing Standard).

**Risco de segurança**: alguns MIME types representam conteúdo executável. Sniffing incorreto pode levar a ataques (ex.: imagem servida como `text/plain` interpretada como HTML).

**Mitigação**:
```http
X-Content-Type-Options: nosniff
```

Força o navegador a confiar exclusivamente no `Content-Type` enviado pelo servidor, sem tentar sniffing.

## 8. Tabela de Tipos Comuns (por Extensão)

| Extensão | MIME Type |
|----------|-----------|
| `.html`, `.htm` | `text/html` |
| `.css` | `text/css` |
| `.js`, `.mjs` | `text/javascript` |
| `.json` | `application/json` |
| `.xml` | `application/xml` |
| `.pdf` | `application/pdf` |
| `.zip` | `application/zip` |
| `.tar` | `application/x-tar` |
| `.gz` | `application/gzip` |
| `.png` | `image/png` |
| `.jpg`, `.jpeg` | `image/jpeg` |
| `.gif` | `image/gif` |
| `.webp` | `image/webp` |
| `.svg` | `image/svg+xml` |
| `.ico` | `image/vnd.microsoft.icon` |
| `.mp4` | `video/mp4` |
| `.webm` | `video/webm` |
| `.ogg`, `.ogv` | `video/ogg` |
| `.mp3` | `audio/mpeg` |
| `.wav` | `audio/wav` |
| `.woff` | `font/woff` |
| `.woff2` | `font/woff2` |
| `.ttf` | `font/ttf` |
| `.otf` | `font/otf` |
| `.csv` | `text/csv` |
| `.webmanifest` | `application/manifest+json` |
| `.docx` | `application/vnd.openxmlformats-officedocument.wordprocessingml.document` |
| `.xlsx` | `application/vnd.openxmlformats-officedocument.spreadsheetml.sheet` |
| `.pptx` | `application/vnd.openxmlformats-officedocument.presentationml.presentation` |
| `.epub` | `application/epub+zip` |
| `.7z` | `application/x-7z-compressed` |

## 9. Erros Comuns de Configuração

- **CSS servido como `text/plain`**: browser não interpreta estilos
- **JavaScript servido como `application/octet-stream`**: browser não executa
- **Áudio/vídeo com MIME type incorreto**: elementos `<audio>`/`<video>` não reproduzem
- **`.gz` no Windows/Mac**: enviado como `application/x-gzip` (não padrão) em vez de `application/gzip`

Servidores web enviam recursos não reconhecidos como `application/octet-stream`, forçando download em vez de renderização inline.

## 10. Referência IANA

Registro oficial: https://www.iana.org/assignments/media-types/media-types.xhtml
