# ADR 0012 — Catálogo de dialetos e providers declarativos

- **Estado:** aceite
- **Data:** 2026-09-30
- **Decisões fundacionais:** DF8 (provider commodity), DF4 (fail-closed), DF5 (evidência tipada),
  DF9 (instrumentação)
- **Épicos:** E12 (camada de providers: T02, T06, T10)
- **Relaciona:** [ADR 0011](0011-porta-provider-e-builtin-opencode.md),
  [plan/13](../plan/13-providers.md)

## Contexto

A ronda da ADR 0011 deixou o caminho `chat/completions` a funcionar ponta-a-ponta, mas o gateway
`opencode go/zen` expõe **quatro dialetos** e cada modelo pode exigir o seu. Faltava também
parametrizar o wire por modelo (campo do teto de tokens, cache de prefixo, `reasoning_format`) e
endurecer o retry/erros. O cruzamento com o `pi` (catálogo gerado `model → api`) e com o `goose`
(definições JSON `engine: openai` + `session_id_header_override`) mostrou duas vias possíveis:
integrar o GDK do `goose` (risco R1, tipos externos na API) ou **possuir** um formato declarativo
mínimo, alinhado com a regra *built-in é tese; o resto é commodity* (plan/13 §3).

## Decisão

1. **Catálogo `model → dialeto`** no `katu-providers` (`catalog.rs`): `Dialect`,
   `MaxTokensField`, `ModelEntry` e `Catalog`. O catálogo do built-in é **derivado de uma
   definição declarativa** (`ProviderSpec`), não de tabelas Rust duplicadas.
2. **Providers declarativos** (`declarative.rs` + `providers/*.json`, no estilo do `goose`):
   `engine` + `base_url` + `api_key_env` + `session_id_header` + catálogo de modelos. O
   `Declarative<T>` reusa os mesmos adaptadores de wire; a chave é injetada pela borda
   (`with_api_key`), nunca lida aqui (o provider é puro).
3. **Dialetos extra**: `responses` (OpenAI Responses API) e `messages` (Anthropic) implementados
   com o mesmo `wire::stream` (retry/erro partilhados); `google` e WebSocket/HTTP2 ficam
   explicitamente `Unsupported` até haver evidência.
4. **Parametrização do wire por modelo**: `max_completion_tokens` vs `max_tokens`,
   `prompt_cache_key`/`prompt_cache_retention` (cache de prefixo) e `reasoning_format` (opt-in).
5. **Retry e erros endurecidos**: `Retry-After` em segundos, milissegundos **ou data `HTTP`**
   (descontando o `Date` da resposta — o provider não toca relógio), `retry_after_seconds` do corpo,
   normalização do erro (`error.message`/`message`/`detail`) e sanitização de `URL`s
   (credenciais/*query* fora do log).

## Alternatives considered

1. **Integrar o GDK `goose-provider-types`/`goose-providers` diretamente.** Rejeitada: traz
   `tokio`/`reqwest`/`chrono` e tipos externos para a API do katu (risco R1 e ruído no firewall);
   o caminho built-in não deve depender do GDK.
2. **Tabelas Rust por dialeto (sem JSON declarativo).** Rejeitada: duplicaria o catálogo e
   tornaria os demais providers trabalho manual, contra a regra commodity.
3. **Enviar sempre `max_completion_tokens`.** Rejeitada: a maioria dos gateways e o `llama.cpp`
   rejeitam o campo; o nome tem de ser por modelo.
4. **Interpretar o `Retry-After` em data com `SystemTime::now`.** Rejeitada: quebra a pureza do
   provider (DF5/instrumentação); usa-se o `Date` da própria resposta.
5. **Implementar já `google` e WebSocket.** Adiada: sem evidência de que algum modelo em uso o
   exija; a rota fica tipada (`Unsupported`) em vez de silenciosa.

## Consequências

- **Positivas:** um só despacho (`engine::stream`) serve built-in e declarativo; trocar de provider
  commodity é um JSON; o modelo certo escolhe o dialeto; o retry e a classificação de erro são
  testáveis sem rede (`MockTransport`).
- **Negativas / dívida:** os dialetos `responses`/`messages` ainda **não** foram validados ao vivo
  (faltam credenciais de modelos que os usem); `google`/WebSocket pendentes; o catálogo embutido
  reflete a lista do `goose` e pode ficar desatualizado (E12-T10 deve ler do endpoint).
- **Travas:** testes do crate cobrem encode+decode dos três dialetos, retry (transitório vs
  conta), `Retry-After` (segundos/ms/data) e `usage` robusto; `make check` mantém o firewall e a
  instrumentação.
