# B1 · Decodificação estruturada por JSON Schema (W8-1) — protocolo e resultado

## Pergunta

A classe de falha **"argumentos de tool em JSON inválido"** (`wire::parse_arguments` →
`ProviderError::Decode`) fecha-se **por construção** se o pedido levar um schema derivado das
tools? E quanto custa — bytes e latência de codificação — quando ligada? Desligada, o pedido fica
**byte a byte** o atual?

## Mecanismo

Com `structured_output` ligado e tools no pedido, o dialeto `chat/completions` acrescenta:

```json
"response_format": {
  "type": "json_schema",
  "json_schema": {
    "name": "katu_tool_call",
    "strict": true,
    "schema": { "oneOf": [ { /* uma variante por tool */ } ] }
  }
}
```

Cada variante liga o **nome** (`const`) aos `parameters` da tool (`katu_tools::schema`): o
`oneOf` só admite `{name, arguments}` com os campos obrigatórios de uma tool real. É o que impede,
por construção, os argumentos que hoje falham a decodificação.

- **Opt-in por provider**: `structured_output` (config) para o `llama` local, ou
  `structured_output` no JSON declarativo / `ModelEntry::with_structured_output` /
  `LlamaConfig::with_structured_output`. Por omissão, **desligado**.
- **Fail-open**: se o endpoint responder `400` (campo não suportado), o pedido é repetido **sem**
  o campo e o comportamento volta a ser o atual. Nada muda no plano de dados: muda como o modelo
  *declara* a chamada.

## Como correr

```sh
# artefacto determinístico (o pedido real, com o harness estatístico de W7)
KATU_GRAMMAR_OUT=$PWD/bench/e18/grammar/raw.json \
  cargo test -q -p katu-providers --lib -- --ignored ab_grammar_by_artifact

# CI: o schema existe, o desligado não o leva, o malformado é excluído, o fail-open repete sem o campo
cargo test -q -p katu-providers --lib
```

## Resultado (artefacto `raw.json`)

Três tools representativas (`read`, `bash`, `edit`), 200 repetições por variante (IC 95 % do
harness de W7):

| | desligado | ligado |
|---|---|---|
| `response_format` | ausente | presente (`katu_tool_call`, `oneOf` de 3) |
| bytes do corpo | `970` | `1 932` (**+962**) |
| p50 de codificação | `34,9 µs` | `152,6 µs` |
| p95 de codificação | `80,8 µs` | `178,2 µs` |
| CI 95 % (média) | `[34,9 µs, 44,8 µs]` | `[149,3 µs, 153,6 µs]` |

O fixture malformado `{"path": }` continua a falhar a decodificação (`decode_error_off = true`),
mas o schema exige `path` na variante `read`: **não é gerável** por uma decodificação restrita.

## Limites (honestidade)

- Mede o **pedido** e o que o schema garante por construção; **não** mede o modelo nem os turnos
  reais. O modelo local não emite *tool calls* nativas, pelo que a taxa real de `Decode` é 0/0
  (sem sinal): a adoção **por omissão** exige ≥ 20 % dos turnos falhados evitados em turnos reais
  e fica **escrita** como pendente (método §0.3).
- O custo é real: +962 bytes e ~4× a codificação para 3 tools (o schema cresce com o catálogo). A
  decisão de ligar é por provider e por projeto — é por isso que o default é desligado.
- O `strict: true` e o `oneOf` seguem o dialeto `OpenAI` `json_schema`; endpoints que não o aceitem
  caem no **fail-open** (repetição sem o campo), medido em `a_rejected_structured_request_falls_back_to_the_current_bytes`.
