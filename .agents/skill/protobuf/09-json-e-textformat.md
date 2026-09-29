# 09 — ProtoJSON e TextFormat

Protobuf tem três formatos: **binário** (primário, endurecido), **ProtoJSON**
(interop) e **TextFormat** (debug). JSON/TextFormat são representações
*name-based* — o nome do campo importa, ao contrário do binário.

## 1. Mapeamento ProtoJSON

| Proto | JSON | Observação |
|---|---|---|
| `message` | objeto | `{}` se vazio |
| `repeated` | array | |
| `map` | objeto | chaves viram strings |
| `enum` | string do nome | aceita número; emite o nome |
| `bool` | `true`/`false` | |
| `string` | string | |
| `bytes` | string base64 (std) | aceita base64url |
| `int32`,`uint32`,`sint*`,`fixed*` | número | |
| `int64`,`uint64`,`fixed64`,`sfixed64` | **string** | evita perda de precisão em JS |
| `float`,`double` | número | `NaN`/`Infinity`/`-Infinity` como strings |
| `Timestamp` | string RFC 3339 | |
| `Duration` | string com `s` | |
| `FieldMask` | string camelCase com vírgulas | |
| `Struct` | objeto JSON | |
| `Value` | valor JSON | |
| `ListValue` | array | |
| `NullValue` | `null` | |
| `Empty` | `{}` | |
| `Any` | `@type` + campos | `@type` = URL do tipo |
| `optional`/presença | campo só aparece se definido | exceto se `always_print` |
| `oneof` | só o membro definido | |

## 2. Regras de nomenclatura

- Por padrão, JSON usa `lowerCamelCase` a partir do `snake_case` do campo.
- `json_name` sobrescreve o nome JSON:
  ```proto
  string user_id = 1 [json_name = "userId"];
  ```
- Ao **parsear**, tanto o nome proto (`user_id`) quanto o JSON (`userId`) são
  aceitos (por compatibilidade).
- Nomes de enum são strings; valores desconhecidos podem ser números.

## 3. Opções de impressão/parse

As APIs de JSON (C++/Java/Python/Go) oferecem flags. Nomes típicos:

| Opção | Efeito |
|---|---|
| `always_print_primitive_fields` / `always_print_fields_with_no_presence` | Emite campos no default |
| `always_print_enums_as_ints` | Enum como número |
| `preserve_proto_field_names` | Usa `snake_case` (não camelCase) |
| `ignore_unknown_fields` (parse) | Ignora campos desconhecidos em vez de erro |
| `print_enums_as_ints` | idem |
| `allow_unknown_fields` (Go) | idem parse |
| `deterministic` | Ordem estável |

> **Segurança**: ao parsear JSON não-confiável, **não** habilite
> `ignore_unknown_fields` sem validar; campos desconhecidos podem indicar
> entrada maliciosa. O default é rejeitar.

## 4. Exemplos

```proto
edition = "2023";
package acme.v1;

import "google/protobuf/timestamp.proto";

message User {
  string user_id = 1;
  int64 created_at_epoch = 2;
  bytes avatar = 3;
  Status status = 4;
  repeated string roles = 5;
  google.protobuf.Timestamp last_seen = 6;
}
enum Status { STATUS_UNSPECIFIED = 0; STATUS_ACTIVE = 1; }
```

```json
{
  "userId": "u-1",
  "createdAtEpoch": "1752062400",
  "avatar": "3q2+7w==",
  "status": "STATUS_ACTIVE",
  "roles": ["admin"],
  "lastSeen": "2026-07-09T12:00:00Z"
}
```

## 5. `Any` em JSON

Para mensagem comum:

```json
{
  "@type": "type.googleapis.com/acme.v1.Order",
  "id": "o-1"
}
```

Para WKT com encoding especial (ex.: `Timestamp`):

```json
{
  "@type": "type.googleapis.com/google.protobuf.Timestamp",
  "value": "2026-07-09T12:00:00Z"
}
```

## 6. TextFormat

Formato de depuração produzido por `DebugString()`/`ToString()` e aceito por
`protoc --decode`/`--encode` e `TextFormat`.

```text
name: "Ana"
id: 42
phones {
  number: "555-1234"
  type: HOME
}
```

Características:
- Campo ausente simplesmente não aparece.
- Repetidos e mensagens usam `{}`; mapa imprime como entradas.
- Aceita nomes de extensão entre `[...]` e `Any` como
  `[type.googleapis.com/acme.v1.Order] { ... }`.
- `--decode_raw` inspeciona binário sem schema (imprime tags/números).

## 7. Quando usar cada formato

| Formato | Uso | Risco |
|---|---|---|
| Binário | Produção, entradas hostis, performance | Nenhum adicional |
| ProtoJSON | Interop web/APIs, configuração, debug legível | Parsing mais complexo; permissivo por opção |
| TextFormat | Debug, ferramentas, testes | **Nunca** para entradas não-confiáveis |

O `SECURITY.md` oficial recomenda: **prefira o binário**; os demais são
secundários e menos endurecidos.

## 8. Segurança em JSON/TextFormat

1. **Limite tamanho e profundidade** — JSON permite aninhamento abusivo.
2. **Rejeite campos desconhecidos** (default) salvo necessidade explícita.
3. **Não** use TextFormat para parsear entrada de rede/usuário.
4. Cuidado com **prototype pollution** em parsers JS (chaves `__proto__`).
5. Faça **redação** de campos sensíveis antes de serializar para log.
6. Valide `Any.type_url` com allowlist.
7. Use `utf8_validation = VERIFY` para strings.
8. Não dependa de `NaN`/`Infinity`/int64 em JSON (perda de precisão em JS).

## 9. Debug sem schema

```bash
# Inspecionar bytes sem o .proto
protoc --decode_raw < payload.bin

# Com o schema (texto legível)
protoc -I. --decode=acme.v1.Order order.proto < payload.bin

# Texto -> binário
protoc -I. --encode=acme.v1.Order order.proto < payload.txt > payload.bin
```

`--decode_raw` mostra números de campo e tipos, útil para diagnosticar
incompatibilidades de contrato.

## 10. Pegadinhas

- **int64 em JS**: sempre trate como string (o JSON já emite string).
- **bytes**: base64 padrão; não confunda com base64url.
- **enums desconhecidos**: em parse, valores fora da faixa podem virar números;
  em enums fechados podem ir a unknown fields.
- **camelCase**: o nome JSON difere do nome proto; ferramentas precisam do
  descritor para converter.
- **ordem de campos**: JSON não é ordenado; não dependa de ordem.
- **presença**: campos no default não aparecem por padrão; isso oculta
  diferenças se você comparar JSON de mensagens semanticamente distintas.
