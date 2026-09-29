# 02 — O contrato do `.proto` (referência completa)

> Sintaxe de referência: **Editions** (`edition = "2023"`). Onde houver
> diferença relevante, anota-se `proto2` e `proto3`. O contrato é a fonte de
> verdade binária e textual; trate-o como código versionado.
>
> Estrutura de um arquivo:
> `edition|syntax` → `package` → `import` → `option` → definições.

## 1. Cabeçalho

### 1.1 Declaração de sintaxe/edição

```proto
// Modelo atual (recomendado)
edition = "2023";

// Modelos legados
syntax = "proto2";
syntax = "proto3";
```

Regras:
- Um e apenas um. Omitir `syntax` significa proto2 (histórico, **não use**).
- `edition` deve ser uma edição conhecida pelo `protoc`
  (`2023`, `2024`, `2026` nesta versão).
- `protoc` rejeita arquivos "do futuro" (edição/feature desconhecida).
- Em editions, `optional`/`required` **não** são palavras-chave de rótulo;
  presença é controlada por feature (ver `03`).
- Comentários: `// linha` e `/* bloco */`.

### 1.2 Pacote

```proto
package acme.catalog.v1;
```

- Evita colisão de nomes; prefixa o nome totalmente qualificado dos tipos.
- Convenção: DNS reverso + versão (`v1`, `v2`).
- Consumidores referenciam tipos como `acme.catalog.v1.Product` ou
  `.acme.catalog.v1.Product` (a forma com ponto é absoluta).
- Em editions/proto3 mais novas, adota-se resolução estrita de nomes
  (ver `16-boas-praticas.md`).

### 1.3 Imports

```proto
import "google/protobuf/timestamp.proto";
import public "common/types.proto";   // reexporta (legado; evitar)
import weak "optional/dep.proto";     // vinculação fraca (legado; evitar)
```

- `import` traz tipos de outro arquivo.
- `public`: quem importa este arquivo também enxerga os tipos do importado
  (evite — acopla a API).
- `weak`: campo pode não estar presente em runtime (legado; evite).
- Todo import precisa ser **usado** (linters modernos exigem).
- Caminhos resolvidos por `--proto_path`/`-I`.

### 1.4 Options de arquivo

```proto
option java_package = "com.acme.catalog.v1";
option java_multiple_files = true;
option java_outer_classname = "CatalogProtos";
option go_package = "acme.com/catalog/v1;catalogv1";
option csharp_namespace = "Acme.Catalog.V1";
option optimize_for = SPEED;            // SPEED | CODE_SIZE | LITE_RUNTIME
option cc_enable_arenas = true;
option deprecated = true;
```

`option` é a forma genérica de anotar o contrato. Nomes de opção são
resolvidos como campos de mensagens (namespace de opções). Opções padrão do
descriptor incluem: `java_*`, `go_package`, `csharp_namespace`, `php_*`,
`objc_class_prefix`, `optimize_for`, `cc_enable_arenas`, `py_generic_services`,
`deprecated`, `features`, etc.

### 1.5 Custom options

```proto
import "google/protobuf/descriptor.proto";

extend google.protobuf.FieldOptions {
  string my_sensitivity = 50001;
}
extend google.protobuf.MessageOptions {
  bool pii = 50002;
}

message User {
  option (pii) = true;
  string email = 1 [(my_sensitivity) = "high"];
}
```

- Extensões de opções são declaradas com `extend` sobre `*Options`.
- Números de extensão para opções públicas devem ser registrados (evitar
  colisão). Ver o registro global em `docs/options.md`.
- Plugins/ferramentas (validação, redação, ORM) consomem custom options.

## 2. Mensagens

```proto
message Order {
  string id = 1;
  int64 amount_minor = 2;
  repeated LineItem items = 3;
  map<string, string> metadata = 4;

  message LineItem {          // tipo aninhado
    string sku = 1;
    uint32 quantity = 2;
  }
}
```

- Uma mensagem é um conjunto de campos nomeados/numerados.
- Tipos aninhados são referenciados como `Order.LineItem`.
- Mensagens podem ser recursivas (`Order` pode conter `Order`).
- Mensagem é sempre `LENGTH_PREFIXED` por padrão (feature `message_encoding`;
  `DELIMITED` reproduz o comportamento de groups).

### 2.1 Campos

```
tipo_do_campo nome_do_campo = número [ opções ];
```

| Elemento | Regra |
|---|---|
| Tipo | escalar, mensagem, enum, `map`, `group` (legado) |
| Nome | `snake_case`; `[a-z][a-z0-9]*(_[a-z0-9]+)*` |
| Número | 1..536.870.911, exceto 19000–19999 (reservado ao protobuf) |
| Opções | `[deprecated = true, json_name = "x", ...]` |

```proto
int32 id = 1;
optional string nickname = 2;          // presença explícita (proto3/editions)
repeated string tags = 3;
map<string, int32> counts = 4;
bytes payload = 5;
google.protobuf.Timestamp created_at = 6;
Status status = 7;
```

### 2.2 Rótulos (labels)

| Rótulo | Onde | Significado |
|---|---|---|
| (nenhum) | proto3/editions | singular; presença conforme feature |
| `optional` | proto2, proto3 | presença explícita (proto2 sempre) |
| `required` | proto2 | **proibido em novos contratos**; wire-required |
| `repeated` | todos | lista ordenada; packing por feature |
| `map<K,V>` | todos | dicionário; não-repetido, não-`optional` |

**Regra moderna:** nunca use `required`. Use enum/nível de aplicação para
precondições e presença explícita para "definido ou não".

### 2.3 Números de campo

Faixas:

| Faixa | Uso |
|---|---|
| 1–15 | Codificam a tag em **1 byte** — use nos campos mais frequentes |
| 16–2047 | Tag em 2 bytes |
| 2048–536870911 | Tag em 3–5 bytes |
| 19000–19999 | **Reservado** pelo protobuf; erro se usado |
| 50000+ | Recomendado para extensões de opções |
| 16–2047 | Faixa comum de extensões de mensagens |

Regras:
- Números são **únicos** dentro da mensagem (considerando-se também a faixa de
  extensão).
- Número 0 é ilegal.
- Ao remover campo, **reserve o número**.
- Não reutilize números — quebra compatibilidade binária e de JSON.

### 2.4 Tipos e defaults

- Em proto2, campos opcionais podem ter `[default = ...]` explícito.
- Em proto3/editions, o default é o zero do tipo (0, `""`, `false`, primeiro
  enumerador) e **não** é configurável para escala (defaults customizados são
  expressos via features/limites).
- Default de campo = valor implícito quando ausente. Com presença explícita,
  "ausente" e "definido como default" são distinguíveis.

## 3. Enums

```proto
enum Status {
  STATUS_UNSPECIFIED = 0;
  STATUS_PENDING = 1;
  STATUS_PAID = 2;
}

message Payment {
  Status status = 1;
  repeated Status history = 2;
}
```

Regras:
- Em proto3/editions, o **primeiro valor deve ser `0`** (default/sentinela).
- Convenção de nome: prefixo do enum em `SHOUTY_SNAKE_CASE`; valor zero
  `*_UNSPECIFIED`/`*_UNKNOWN`.
- `option allow_alias = true;` permite valores duplicados:
  ```proto
  enum E {
    option allow_alias = true;
    E_UNSPECIFIED = 0;
    E_STARTED = 1;
    E_RUNNING = 1;   // alias
  }
  ```
- `reserved` funciona em enums (números e nomes).
- Enums podem ser aninhados e referenciados como `Order.Status`.
- **Aberto × fechado** (feature `enum_type`): aberto aceita valores fora da
  faixa e os mantém; fechado joga valores desconhecidos em unknown fields
  (proto2). Já foi o principal motivo de divergência entre runtimes.
- Negative enum values são permitidos, mas custam 10 bytes (varint).

## 4. Oneof

```proto
message Payment {
  oneof method {
    string card_token = 1;
    string pix_key = 2;
    Boleto boleto = 3;
  }
  string idempotency_key = 4;
}
```

- Exatamente **um** membro definido por vez. Definir outro limpa o anterior.
- Presença explícita garantida (o "case" do oneof).
- Não pode ser `repeated` nem `map`.
- `oneof` não pode ser usado diretamente como `optional`.
- Cuidado: adicionar membro a um oneof é compatível; remover/mover exige
  reservar. Consumidores que usam `switch` devem tratar `case` desconhecido.

## 5. Maps

```proto
map<string, Project> projects = 1;
map<int64, string> names = 2;
```

- Equivalente de fio a `repeated MapEntry { K key = 1; V value = 2; }`.
- Chaves: qualquer tipo integral, `bool` ou `string` (não `float`, `bytes`,
  enum, mensagem).
- Valores: qualquer tipo, inclusive mensagens e enums.
- **Sem ordem garantida** — não dependa de iteração ordenada em comparações.
- Duplicatas na decodificação: "última vence".
- Campo de map não tem presença; vazio == ausente.
- Não use `map` quando a chave é altamente esparsa e você precisa de semântica
  de repetição ordenada.

## 6. Serviços (RPC)

```proto
service OrderService {
  rpc GetOrder (GetOrderRequest) returns (Order);
  rpc ListOrders (ListOrdersRequest) returns (ListOrdersResponse);
  rpc WatchOrders (WatchRequest) returns (stream Order);          // server stream
  rpc UploadOrders (stream Order) returns (UploadSummary);        // client stream
  rpc Chat (stream Message) returns (stream Message);             // bidi
}
```

Regras:
- Cada `rpc` tem request e response (nomes de mensagem), opcionalmente
  `stream` em qualquer lado.
- `option idempotency_level = NO_SIDE_EFFECTS;` (proto3/editions) informa que
  o método é idempotente/seguro para retry.
- `option deprecated = true;` marca o método.
- Serviços herdam o `package`; nomes de método em `PascalCase`.
- Sem `stream`, é unary (uma request, uma response).
- Serviços podem ser `google.api.http` anotados (grpc-gateway) via custom
  options.

Ver a skill irmã [`../grpc_guide/`](../grpc_guide/) para semântica de transporte (RPC).

## 7. Extensões e faixas

```proto
message Foo {
  extensions 100 to 199;
  extend Foo {
    optional string bar = 126;   // declaração aninhada
  }
}
```

- Permitem adicionar campos a mensagens existentes sem editar o arquivo
  original (usado intensamente para custom options).
- `extensions` define as faixas reservadas para extensão.
- `extend` declara o campo estendido com número **dentro** da faixa.
- Extensões de `google.protobuf.*Options` são o mecanismo de custom options.
- **Evite** extensions em novos contratos de dados; prefira `Any` ou campos
  novos. Extensions não existem em proto3 (exceto para options).

## 8. Grupos (legado)

```proto
// proto2 apenas
optional group Result = 1 {
  optional string url = 2;
}
```

- Equivale a um campo de mensagem com encoding delimitado (`DELIMITED`).
- **Não use**. Use mensagens aninhadas + `features.message_encoding = DELIMITED`
  quando precisar exatamente do mesmo encoding legado.

## 9. `reserved`

```proto
message Account {
  reserved 3, 5 to 8, 12;
  reserved "old_name", "legacy_id";
}
```

- Impede reutilização de números/nomes removidos.
- Aparece em mensagens e enums.
- Deve vir antes/independente dos campos; boa prática: logo no início.
- Se um linter exigir, declare a "próxima" faixa:
  `reserved 100 to max;` (quando o próximo número é 100).

## 10. Opções por entidade

| Escopo | Exemplo |
|---|---|
| Arquivo | `option java_package = "...";` |
| Mensagem | `option message_set_wire_format = true;` / `option deprecated = true;` |
| Campo | `[deprecated = true, json_name = "userName", debug_redact = true]` |
| Enum | `option allow_alias = true;` |
| Valor de enum | `[deprecated = true]` |
| Serviço/Método | `option idempotency_level = NO_SIDE_EFFECTS;` |
| Oneof | `option deprecated = true;` |

Opções de campo frequentemente usadas:

```proto
string user_name = 1 [json_name = "userName", deprecated = true, debug_redact = true];
int32 legacy = 2 [deprecated = true];
repeated int32 xs = 3 [packed = true];            // proto2; ver feature
string s = 4 [ctype = CORD];                       // proto2 (C++)
```

Em editions, prefira `features.*` (ex.: `features.field_presence`,
`features.repeated_field_encoding`, `features.(pb.cpp).string_type`).

## 11. Gramática resumida (EBNF informal)

```
file         = [ "edition" "=" str ";" ] | [ "syntax" "=" str ";" ]
               { "package" ident ";" }
               { "import" [ "public" | "weak" ] str ";" }
               { "option" option_name "=" value ";" }
               { top_level }

top_level    = "message" ident "{" message_body "}"
             | "enum"    ident "{" enum_body "}"
             | "service" ident "{" rpc* "}"
             | "extend"  type   "{" field* "}"

message_body = { field | map_field | oneof | group | message | enum
               | "option" ... | "reserved" ... | "extensions" ... | "extend" ... }

field        = [ label ] type field_name "=" number [ "[" options "]" ] ";"
label        = "repeated" | "optional" | "required"
map_field    = "map" "<" key_type "," type ">" field_name "=" number ... ";"
oneof        = "oneof" ident "{" field+ "}"
enum_body    = { "option" ... | enum_value | "reserved" ... }
enum_value   = ident "=" int [ "[" options "]" ] ";"
rpc          = "rpc" ident "(" [ "stream" ] type ")" "returns"
                    "(" [ "stream" ] type ")" ( ";" | "{" "option"* "}" )
```

## 12. Nomes e estilo

| Entidade | Caso | Regex (lint estrito) |
|---|---|---|
| Arquivo | `lower_snake_case.proto` | — |
| Pacote | `lower.snake.case` | `[a-z][a-z0-9]*(_[a-z0-9]+)*` por componente |
| Mensagem/Serviço/Método | `PascalCase` | `([A-Z][a-zA-Z0-9]*)+` |
| Campo | `lower_snake_case` | `[a-z][a-z0-9]*(_[a-z0-9]+)*` |
| Enum | `PascalCase` | idem mensagem |
| Valor de enum | `SHOUTY_SNAKE_CASE` | `[A-Z][A-Z0-9]*(_[A-Z0-9]+)*` |

O estilo estrito é aplicado pelas features `enforce_naming_style`
(`STYLE2024`/`STYLE2026`) e por `buf lint` (categorias DEFAULT/BASIC/STANDARD).

## 13. Regras de ouro do contrato

1. Um arquivo por domínio/versão; nome do arquivo = `lower_snake_case.proto`.
2. Sempre declare `edition` (ou, se legado, `syntax`) e `package`.
3. `1`–`15` para campos quentes; evite buracos não reservados.
4. Nunca reutilize número; `reserved` é obrigatório ao remover.
5. Enum: `0 = *_UNSPECIFIED`; prefixo do enum nos valores.
6. Prefira `enum` a `bool` para estados extensíveis.
7. Declare a versão no pacote (`v1`, `v2`), nunca no nome do campo.
8. Evite `required`, `group`, `weak`/`public` imports e extensions de dados.
9. Documente cada campo com `//` acima dele (vira doc do código gerado).
10. Mantenha o contrato compatível com `buf breaking --against main`.
