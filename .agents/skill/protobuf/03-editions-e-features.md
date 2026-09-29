# 03 — Editions e Features (o contrato moderno)

> Protocol Buffers v37. Editions substituem `syntax` e unificam proto2/proto3.
> Edições conhecidas: `2023` (Edition Zero), `2024`, `2026`.
> Documento-fonte: `descriptor.proto` (`message FeatureSet`) e
> `docs/design/editions/*`.

## 1. Por que editions

Antes, o ecossistema se dividia entre `proto2` e `proto3`; migrar exigia uma
mudança disruptiva. **Editions** permitem evoluir o Protobuf incrementalmente:
em vez de escolher uma "sintaxe", o arquivo declara uma **edição** e pode ligar
ou desligar comportamentos via **features**.

Princípios (do design oficial):

1. `syntax = ...` é substituído por `edition = ...`; uma nova edição sai ~1×/ano.
2. **Features** são opções especiais de arquivo/mensagem/campo/enum que
   controlam codegen e runtime.
3. Uma edição muda apenas **defaults de features**; não cria comportamento novo.
4. Mensagens de qualquer combinação de features **interoperam** (o wire format
   não muda por causa de features).
5. A distinção proto2/proto3 desaparece; os dois viram subconjuntos expressáveis
   por features.
6. Features têm **herança**: o valor do pai vale para os filhos até ser
   sobrescrito.

## 2. Anatomia de uma feature

Toda feature é um campo/extensão do campo `features` (do tipo `FeatureSet`),
presente em toda entidade sintática. Exemplo:

```proto
option features.field_presence = EXPLICIT;         // no arquivo
message M {
  int32 a = 1;                                     // herda EXPLICIT
  int32 b = 2 [features.field_presence = IMPLICIT]; // sobrescreve
}
```

Metadados que toda feature carrega (visíveis em `descriptor.proto`):

| Metadado | Significado |
|---|---|
| `targets` | Onde pode ser aplicada (`TARGET_TYPE_FILE`, `_FIELD`, `_MESSAGE`, `_ENUM`, ...) |
| `retention` | `RETENTION_RUNTIME` (resolvida em runtime) ou `RETENTION_SOURCE` (só em compilação, removida do descritor) |
| `feature_support` | `edition_introduced`, `edition_deprecated`, `edition_removed`, `deprecation_warning` |
| `edition_defaults` | Default por edição (`EDITION_LEGACY`, `EDITION_PROTO3`, `EDITION_2023`, ...) |

`EDITION_LEGACY` representa "antes de editions" (proto2 e, em certos casos,
proto3) e serve de fallback.

## 3. FeatureSet — features core

Tabela consolidada (valores e defaults por edição):

| Feature | Valores | LEGACY (proto2) | PROTO3 | 2023 | 2024 | 2026 |
|---|---|---|---|---|---|---|
| `field_presence` | `EXPLICIT`, `IMPLICIT`, `LEGACY_REQUIRED` | `EXPLICIT` | `IMPLICIT` | `EXPLICIT` | `EXPLICIT` | `EXPLICIT` |
| `enum_type` | `OPEN`, `CLOSED` | `CLOSED` | `OPEN` | `OPEN` | `OPEN` | `OPEN` |
| `repeated_field_encoding` | `PACKED`, `EXPANDED` | `EXPANDED` | `PACKED` | `PACKED` | `PACKED` | `PACKED` |
| `utf8_validation` | `VERIFY`, `NONE` | `NONE` | `VERIFY` | `VERIFY` | `VERIFY` | `VERIFY` |
| `message_encoding` | `LENGTH_PREFIXED`, `DELIMITED` | `LENGTH_PREFIXED` | `LENGTH_PREFIXED` | `LENGTH_PREFIXED` | `LENGTH_PREFIXED` | `LENGTH_PREFIXED` |
| `json_format` | `ALLOW`, `LEGACY_BEST_EFFORT` | `LEGACY_BEST_EFFORT` | `ALLOW` | `ALLOW` | `ALLOW` | `ALLOW` |
| `enforce_naming_style` | `STYLE_LEGACY`, `STYLE2024`, `STYLE2026` | `STYLE_LEGACY` | `STYLE_LEGACY` | `STYLE_LEGACY` | `STYLE2024` | `STYLE2026` |
| `default_symbol_visibility` | `EXPORT_ALL`, `EXPORT_TOP_LEVEL`, `LOCAL_ALL`, `STRICT` | `EXPORT_ALL` | `EXPORT_ALL` | `EXPORT_ALL` | `EXPORT_TOP_LEVEL` | `STRICT` |
| `enforce_proto_limits` | `LEGACY_NO_EXPLICIT_LIMITS`, `PROTO_LIMITS2026` | legado | legado | legado | legado | `PROTO_LIMITS2026` |

### 3.1 `field_presence`

- `EXPLICIT` — presença explícita (`has_`/clear; default definido é serializado).
- `IMPLICIT` — sem presença (default não serializado; merge ignora default).
- `LEGACY_REQUIRED` — campo wire-required (equivalente a `required` proto2).
  Exige allowlist; **evite**.

Em editions, os rótulos `optional`/`required` **não existem mais** para fields
singulares; controle por feature (no campo, na mensagem ou no arquivo).

```proto
edition = "2023";
option features.field_presence = IMPLICIT;                // default do arquivo

message M {
  int32 a = 1;                                            // herda IMPLICIT
  int32 b = 2 [features.field_presence = EXPLICIT];       // sobrescreve
  int32 c = 3 [features.field_presence = LEGACY_REQUIRED];// evitar
}
```

### 3.2 `enum_type`

- `OPEN` — valores desconhecidos ficam no campo (são preservados).
- `CLOSED` — valores desconhecidos vão para unknown fields.

Arquivos proto2 migrados recebem `CLOSED` explícito. Enums `IMPLICIT` em
C++/Java são sempre tratados como abertos por compatibilidade histórica.

### 3.3 `repeated_field_encoding`

- `PACKED` — repetidos escalares num único valor length-delimited.
- `EXPANDED` — um par tag/valor por elemento.

Só se aplica a tipos escalares (numéricos/bool/enum); `string`, `bytes` e
mensagens são sempre length-delimited por elemento.

### 3.4 `utf8_validation`

- `VERIFY` — valida UTF-8 ao parsear strings.
- `NONE` — não valida.

**Segurança:** mantenha `VERIFY`. Desligar permite strings inválidas.

### 3.5 `message_encoding`

- `LENGTH_PREFIXED` — mensagem normal.
- `DELIMITED` — encoding de grupo (`StartGroup`/`EndGroup`); usado para
  reproduzir `group` legado em editions.

### 3.6 `json_format`

- `ALLOW` — ProtoJSON estrito/padrão.
- `LEGACY_BEST_EFFORT` — tolera formatos históricos (proto2).

### 3.7 `enforce_naming_style`

`STYLE_LEGACY` → `STYLE2024` → `STYLE2026`. Aplica os regexes estritos de
nomes (`PascalCase`, `snake_case`, `SHOUTY_SNAKE_CASE`). O `protoc` rejeita
nomes fora do padrão quando ativo.

### 3.8 `default_symbol_visibility`

Controla a visibilidade default de símbolos (nas linguagens que suportam):

- `EXPORT_ALL` — tudo exportado (legado).
- `EXPORT_TOP_LEVEL` — topo exportado, aninhado local.
- `LOCAL_ALL` — tudo local.
- `STRICT` — tudo local, aninhados não exportáveis; **recomendado** para novos
  protos.

Visibilidade explícita usa `export`/`local`:

```proto
edition = "2024";
local message Parent {
  local message Nested { ... }
  export enum PublicEnum { E_UNSPECIFIED = 0; }
}
```

### 3.9 `enforce_proto_limits`

`PROTO_LIMITS2026` (edition 2026) impõe limites explícitos no nível do
`protoc` (nº de símbolos/campos, profundidade, etc.), substituindo o
comportamento legado em que os limites só falhavam no codegen. Consulte a
documentação da Edition 2026 para a lista exata.

## 4. Features por linguagem

Extensões de `FeatureSet` definidas pelos backends (números reservados no
descriptor). Aplicam-se com `features.(pb.<lang>).<feature>`.

| Linguagem | Extensão | Features relevantes |
|---|---|---|
| C++ | `.pb.cpp` (1000) | `string_type` (`VIEW`/`CORD`/`STRING`), `repeated_type`, `enum_name_uses_string_view`, `legacy_closed_enum` |
| Java | `.pb.java` (1001) | `utf8_validation`, `large_enum`, `legacy_closed_enum` |
| Java mutable | `.pb.java_mutable` (9989) | builders mutáveis |
| Go | `.pb.go` (1002) | API level, strip enum prefix, etc. |
| Python | `.pb.python` (1003) | — |
| C# | `.pb.csharp` (1004) | — |

Exemplos:

```proto
edition = "2023";
import option "google/protobuf/cpp_features.proto";

option features.(pb.cpp).string_type = VIEW;   // std::string_view
// no campo:
string name = 1 [features.(pb.cpp).string_type = CORD];
repeated int32 xs = 2 [features.(pb.cpp).repeated_type = ...];
```

> Regra para **schema producers**: não force features específicas de linguagem
> em schemas compartilhados; consumidores podem aplicá-las localmente.

## 5. Herança e resolução

1. Defaults da edição (`edition_defaults`) formam a base.
2. Features se herdam do pai para o filho (arquivo → mensagem → campo).
3. A sobrescrita mais próxima vence.
4. Resolução é feita por comparação de edição + merge de `FeatureSet`
   (`FeatureSetDefaults`), reproduzível em qualquer linguagem.

Isso permite aplicar uma feature uma vez no arquivo em vez de em cada campo,
minimizando churn.

## 6. Equivalências proto2/proto3 → editions

| proto2 | proto3 | editions equivalentes |
|---|---|---|
| `required int32 x = 1;` | — | `int32 x = 1 [features.field_presence = LEGACY_REQUIRED];` |
| `optional int32 x = 1;` | `optional int32 x = 1;` | `int32 x = 1;` (default EXPLICIT) |
| `optional int32 x = 1 [default = 5];` | — | default customizado (limitado) |
| `repeated int32 x = 1;` (expandido) | `repeated int32 x = 1;` (packed) | `repeated int32 x = 1 [features.repeated_field_encoding = PACKED];` |
| enum fechado | enum aberto | `option features.enum_type = CLOSED|OPEN;` |
| `group` | — | `message` + `features.message_encoding = DELIMITED` |
| `optional` semântica proto2 | `optional` = presença explícita | `features.field_presence = EXPLICIT` |

## 7. Migração para editions

Passos recomendados (produtor de schema):

1. Garanta que **todos** os runtimes do seu suporte entendem editions.
2. Rode a ferramenta de upgrade (`prototiller`/`protoc` de migração) para
   converter proto2/proto3 → edition 2023 **sem mudança de comportamento**
   (o wire format não muda).
3. Revise as features emitidas (especialmente `field_presence`,
   `enum_type`, `repeated_field_encoding`).
4. Escolha a edição-alvo: **a mais nova suportada pelo runtime mais antigo**
   da sua matriz.
5. Publique `.proto`, não código gerado.
6. Valide com `buf breaking` e testes de round-trip/interop.

Regra de suporte (schema producers):
- Publique apenas `.proto`.
- Mantenha a edição consistente em todo o conjunto publicado.
- Minimize features específicas de linguagem.
- Ao subir de edição, suba o major da biblioteca publicada (se publicar gencode).

## 8. Riscos e pegadinhas

- **Edição futura**: `protoc` antigo rejeita `edition` que não conhece; o
  runtime deve ter fallback razoável ao encontrar descritor do futuro.
- **Retenção `SOURCE`**: features como `enforce_naming_style` **não** aparecem
  no descritor em runtime (não há como inspecioná-las em runtime).
- **`EDITION_LEGACY`**: não é uma edição real; é o bucket de defaults pré-editions.
- **Enums `CLOSED` + C++/Java**: o comportamento ainda depende do arquivo
  (quirk histórico preservado para migração sem quebra).
- **Mistura de features**: mensagens com qualquer combinação interoperam no
  wire, mas o comportamento de API pode diferir por linguagem.
- Não edite features manualmente em massa sem ferramenta: prefira o migrador
  oficial para garantir equivalência.

## 9. Checklist de contrato moderno

- [ ] `edition = "2023"` (ou mais nova suportada) declarada.
- [ ] `package` com versão (`v1`).
- [ ] Campos `1`–`15` reservados para os mais usados.
- [ ] Presença explícita onde "ausente ≠ default" importa.
- [ ] Enums com `0 = *_UNSPECIFIED`.
- [ ] `reserved` para tudo que foi removido.
- [ ] Features de linguagem **não** forçadas em schema compartilhado.
- [ ] `buf lint` e `buf breaking` passando no CI.
- [ ] Testes de round-trip binário e JSON.
