# URI: Fundamentos

## 1. Definição

> "Uniform Resource Identifiers (URI) are used to identify 'resources' on the web."

A **RFC 3986** é a especificação central. URIs são usadas como alvos de requisições HTTP e como links em HTML.

### URI vs URL vs URN

```
URI (identifica recurso)
├── URL (localiza recurso) → http://, https://, ftp://
└── URN (nomeia recurso)   → urn:isbn:, urn:ietf:
```

Toda URL é uma URI, mas nem toda URI é uma URL.

## 2. Anatomia da URI (RFC 3986)

```
  scheme    authority        path        query     fragment
  ┌───┐ ┌──────────┐ ┌──────────────┐ ┌────────┐ ┌──────┐
  http://www.example.com:80/path/to/page?key1=val1#section
  └─┬─┘└──────┬───────┘└─────┬──────┘└───┬────┘└───┬───┘
    │        authority       path       query    fragment
  scheme
```

### 2.1 Scheme (RFC 3986 §3.1)

Indica o protocolo para fetch do recurso.

```
protocol:
```

Caracteres permitidos: alfanuméricos + `+`, `-`, `.`.

**Schemes comuns**:

| Scheme | Propósito |
|--------|-----------|
| `http` / `https` | HTTP (padrão web) |
| `blob` | Objeto binário em memória |
| `data` | Dados inline embutidos na URL |
| `file` | Arquivos locais |
| `ftp` | File Transfer Protocol |
| `javascript` | Código JS na URL |
| `mailto` | Email |
| `tel` | Telefone |
| `urn` | Uniform Resource Name |
| `ws` / `wss` | WebSocket |
| `view-source` | Código-fonte do recurso |

**Regra moderna**: subrecursos em HTML **devem** usar apenas `http` e `https`. FTP para subrecursos está sendo removido.

### 2.2 Authority (RFC 3986 §3.2)

```
host
host:port
user@host
user@host:port
```

- **host**: domínio ou IP; resolvido via DNS
- **port**: opcional; default 80 (HTTP) / 443 (HTTPS)
- **user**: opcional; autenticação na URL — **NÃO RECOMENDADO**

### 2.3 Path (RFC 3986 §3.3)

Segmento hierárquico após authority. Schemes hierárquicos parseiam como segmentos `/`. Schemes opacos tratam como string única.

- Toda URI tem path (mínimo: `/` ou vazio)
- Browsers normalizam path vazio para `/`

### 2.4 Query (RFC 3986 §3.4)

```
?query
```

- Qualquer caractere exceto `#`
- Formato `?key=value&key2=value2` é **convenção**, não requisito da RFC
- **Enviada ao servidor** (diferente de fragment)

### 2.5 Fragment (RFC 3986 §3.5)

```
#fragment
```

- **NÃO enviado ao servidor** — processado apenas pelo cliente
- Usado para âncoras, text fragments e media fragments

## 3. Componentes URI — Tabela Comparativa

| Componente | Prefixo | Enviado ao Servidor? | Codificação |
|-----------|---------|---------------------|-------------|
| Scheme | `scheme:` | Sim | Apenas alfanumérico + `+`, `-`, `.` |
| Authority | `//` | Sim | Percent-encoding de caracteres não ASCII |
| Path | `/...` | Sim | Percent-encoding (`?` e `#` proibidos) |
| Query | `?` | Sim | Percent-encoding (`#` proibido) |
| Fragment | `#` | **Não** | Percent-encoding |

## 4. Boas Práticas de URI

1. **Sempre usar HTTPS** para produção
2. **Nunca incluir credenciais** na authority (`user:password@host`)
3. **Escolher e manter** domínio canônico (www vs non-www)
4. **Usar query para dados do servidor**, fragment para dados do cliente
5. **Percent-encoding** de caracteres reservados RFC 3986
6. **Evitar `javascript:` URLs** — use event listeners e DOM API
7. **Usar `data:` com moderação** — bloqueado em top-level navigation
8. **Gerenciar blob URLs** — sempre chamar `URL.revokeObjectURL()` quando não precisar mais
