# 08 — Tipos bem-conhecidos (Well-Known Types)

Tipos definidos em `google/protobuf/*.proto`, com semântica e mapeamento JSON
especiais. Use-os em vez de reinventar representações.

## 1. Catálogo

| Tipo | Arquivo | Uso |
|---|---|---|
| `Empty` | `empty.proto` | Request/response sem campos |
| `Timestamp` | `timestamp.proto` | Instante absoluto (UTC) |
| `Duration` | `duration.proto` | Intervalo de tempo com sinal |
| `Any` | `any.proto` | Mensagem de tipo dinâmico + `type_url` |
| `Struct` / `Value` / `ListValue` / `NullValue` | `struct.proto` | JSON arbitrário |
| `DoubleValue`, `FloatValue`, `Int64Value`, `UInt64Value`, `Int32Value`, `UInt32Value`, `BoolValue`, `StringValue`, `BytesValue` | `wrappers.proto` | Wrappers de presença (legado) |
| `FieldMask` | `field_mask.proto` | Conjunto de caminhos de campos |
| `Api`, `Method`, `Mixin` | `api.proto` | Descrição de API/RPC (reflection/serviços) |
| `Type`, `Field`, `Enum`, `EnumValue`, `Option` | `type.proto` | Sistema de tipos dinâmico |
| `SourceContext` | `source_context.proto` | Contexto de origem (ex.: arquivo) |

## 2. `Timestamp`

```proto
import "google/protobuf/timestamp.proto";

message Event {
  google.protobuf.Timestamp occurred_at = 1;
}
```

- Representa um instante desde `1970-01-01T00:00:00Z`.
- Campos: `seconds` (int64), `nanos` (int32, 0–999.999.999).
- Faixa: `0001-01-01T00:00:00Z` … `9999-12-31T23:59:59.999999999Z`.
- JSON: string RFC 3339 UTC com "Z" (`"2026-07-09T12:00:00Z"`), até 9 dígitos
  fracionários; aceita offsets ao parsear.
- Sempre normalize para UTC; use bibliotecas de calendário.
- **Não** use `int64` "epoch" sem especificar unidade/fuso.

## 3. `Duration`

```proto
google.protobuf.Duration timeout = 1;
```

- Campos: `seconds` (int64), `nanos` (int32).
- Faixa: ±10.000 anos.
- JSON: string com sufixo `s` (`"3.5s"`, `"0.000000001s"`).
- Nunca use `float` para duração (precisão).

## 4. `Any`

```proto
import "google/protobuf/any.proto";

message Envelope {
  google.protobuf.Any payload = 1;
  string correlation_id = 2;
}
```

- Campos: `type_url` (string), `value` (`bytes`).
- `type_url` = prefixo com `/` + nome totalmente qualificado, ex.:
  `type.googleapis.com/acme.v1.Order`.
- Operações: **pack** (empacotar), **unpack** (desempacotar, validando o tipo),
  **is** (testar tipo).
- JSON: `@type` + campos (ou `@type` + `value` para WKT com encoding especial).
- TextFormat: `[type.googleapis.com/acme.v1.Order] { ... }`.
- **Segurança**: sempre valide o `type_url` contra uma allowlist antes de
  fazer unpack (evita type-confusion e carregamento de tipos inesperados).
- Prefira `oneof` quando o conjunto de variantes é conhecido e fechado.

## 5. `Struct` / `Value` / `ListValue`

```proto
import "google/protobuf/struct.proto";

message Config {
  google.protobuf.Struct extra = 1;
}
```

- `Struct`: `map<string, Value>` — objeto JSON.
- `Value`: `oneof kind { null_value, number_value, string_value, bool_value,
  struct_value, list_value }`.
- `ListValue`: `repeated Value`.
- JSON: mapeiam diretamente para objeto/array/valor JSON.
- **Limitações**: `number_value` é `double` — não representa inteiros grandes
  nem `NaN`/`Infinity`; não use para dinheiro/IDs.
- Use quando o payload é JSON realmente arbitrário; caso contrário, prefira
  mensagem tipada.

## 6. Wrappers (`*Value`)

```proto
import "google/protobuf/wrappers.proto";
google.protobuf.Int32Value maybe_count = 1;
```

- Cada wrapper tem um `value` e **presença explícita** embutida (o wrapper em si
  é uma mensagem que pode estar ausente).
- **Legado**: substitua por `optional int32 maybe_count = 1;` (proto3/editions).
- Desvantagens: mais bytes, mapeamento JSON não-intuitivo e API verbosa.

## 7. `Empty`

```proto
import "google/protobuf/empty.proto";

service Health {
  rpc Ping (google.protobuf.Empty) returns (google.protobuf.Empty);
}
```

- Use em métodos sem payload. Em editions, uma mensagem local vazia também
  serve; `Empty` é convenção e permite compatibilidade.
- JSON: `{}`.

## 8. `FieldMask`

```proto
import "google/protobuf/field_mask.proto";

message UpdateUserRequest {
  User user = 1;
  google.protobuf.FieldMask update_mask = 2;
}
```

- `paths`: lista de caminhos separados por vírgula no JSON, ex.:
  `"name,address.city"`.
- Resolve o problema de "atualizar para o default": o cliente lista os campos
  que quer alterar, independentemente de presença.
- Aplicação: copie apenas os campos listados (`FieldMaskUtil`/helpers).
- Semântica: `*` significa todos os campos; caminhos usam `snake_case`.
- Valide caminhos contra o descritor (evita campos inexistentes).

## 9. `Api` / `Method` / `Type` / `Field` / `Enum`

- Descrevem serviços e tipos em runtime (usados por reflection e tooling).
- Raramente usados diretamente em payloads de aplicação; presentes em
  `google/protobuf/type.proto` e `api.proto`.

## 10. Como escolher

| Necessidade | Use |
|---|---|
| Instante | `Timestamp` |
| Intervalo | `Duration` |
| JSON dinâmico | `Struct`/`Value` |
| Tipo dinâmico com conjunto aberto | `Any` (validando `type_url`) |
| Variante conhecida | `oneof` |
| Presença de escalar | `optional T` |
| Atualização parcial | `FieldMask` |
| Nada | `Empty` |
| Inteiro grande/dinheiro | tipo próprio (`int64`) |
| Decimal exato | tipo próprio (string/inteiro com escala) |

## 11. Boas práticas

1. Importe os WKT explicitamente; nunca copie os `.proto` para o seu repo.
2. Prefira `oneof` a `Any` quando as variantes são conhecidas.
3. Trate `Any.type_url` como entrada não-confiável.
4. Nunca represente dinheiro/IDs grandes com `Value` (`double`).
5. Centralize conversão de datas em UTC e valide faixas.
6. Use `FieldMask` em vez de wrappers para updates parciais.
7. Em editions, migre wrappers para `optional`.
