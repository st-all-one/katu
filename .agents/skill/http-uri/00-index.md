# HTTP + URI: Guia Completo do Padrão Moderno

## Índice

| # | Documento | Conteúdo |
|---|-----------|----------|
| 01 | [01-fundamentos-http.md](./01-fundamentos-http.md) | Fundamentos do HTTP: mensagens, sessões, métodos, status codes |
| 02 | [02-fundamentos-uri.md](./02-fundamentos-uri.md) | Fundamentos de URI: componentes RFC 3986, schemes |
| 03 | [03-padrao-moderno-http.md](./03-padrao-moderno-http.md) | HTTP moderno: HTTP/2, HTTP/3, caching, compressão, negociação |
| 04 | [04-padrao-moderno-uri.md](./04-padrao-moderno-uri.md) | URI moderno: schemes especiais, text fragments, media fragments |
| 05 | [05-seguranca.md](./05-seguranca.md) | Segurança HTTP+URI: CORS, CSP, HSTS, ataques semânticos |
| 06 | [06-modelo-basico.md](./06-modelo-basico.md) | Modelo básico de implementação: headers essenciais, padrões |
| 07 | [07-especificacoes.md](./07-especificacoes.md) | Especificações e RFCs completas |
| 08 | [08-interdependencias.md](./08-interdependencias.md) | Matriz de interdependências HTTP ↔ URI ↔ HTML ↔ API |
| 09 | [09-proxies-tunneling.md](./09-proxies-tunneling.md) | Proxies e tunneling: forward/reverse, CONNECT, PAC files |
| 10 | [10-client-hints.md](./10-client-hints.md) | Client Hints: Accept-CH, low/high entropy, Critical-CH |
| 11 | [11-mime-types.md](./11-mime-types.md) | MIME Types: estrutura, tipos discretos/multipart, tabela completa |
| 12 | [12-network-error-logging.md](./12-network-error-logging.md) | Network Error Logging: NEL header, Report-To, tipos de erro |
| 13 | [13-authentication.md](./13-authentication.md) | HTTP Authentication: fluxo completo, schemes, configuração |
| 14 | [14-cookies.md](./14-cookies.md) | Cookies: tipos, atributos, prefixes, privacidade, regulamentações |
| 15 | [15-browser-detection.md](./15-browser-detection.md) | Browser Detection: por que evitar, feature detection, UA reduction |
| 16 | [16-scheme-ssh.md](./16-scheme-ssh.md) | URI Scheme `ssh:`: sintaxe, usos, segurança |

## Propósito

Compilado de documentação técnica otimizada para consumo por IA e desenvolvedores, sintetizando o conteúdo dos diretórios `http/` e `uri/` do MDN Web Docs.

**Foco**: padrão moderno atual, semântica, boas práticas, segurança, acessibilidade e modelo básico de implementação.

**Cobertura**: 16 documentos abrangendo todos os guias e referências dos diretórios fonte (28 guias HTTP + 12 referências URI + gaps identificados).

## Referência Central

- HTTP Semantics → RFC 9110 (STD 97)
- HTTP/1.1 → RFC 9112 (STD 99)
- HTTP/2 → RFC 9113
- HTTP/3 → RFC 9114 (QUIC)
- URI Generic Syntax → RFC 3986 (STD 66)
