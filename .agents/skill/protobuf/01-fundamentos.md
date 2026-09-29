# 01 — Fundamentos

## 1. O que Protocol Buffers resolve

Protobuf resolve **contrato + serialização** entre sistemas heterogêneos:

- Contrato: um `.proto` descreve mensagens, enums e serviços de forma
  independente de linguagem.
- Serialização: um formato binário compacto e rápido, com regras de evolução
  compatíveis.
- Geração de código: `protoc` produz classes nativas com setters/getters e
  (de)serialização, além de stubs de RPC.

O resultado é um **contrato versionado** que pode evoluir sem quebrar peers,
diferente de JSON sem schema, onde a validação e a compatibilidade são
responsabilidade da aplicação.

## 2. As três peças

### 2.1 IDL (`.proto`)

Arquivo texto que declara:

```proto
edition = "2023";
package acme.catalog.v1;

message Product {
  string id = 1;
  string name = 2;
  int64 price_minor = 3;
}
```

### 2.2 Compilador (`protoc`)

```bash
protoc --proto_path=. --cpp_out=./gen catalog.proto
```

`protoc` faz: parsing, resolução de tipos/imports, validação, geração de
`FileDescriptorProto` e invocação de plugins (`--<lang>_out`). O descritor
binário do schema é a fonte de verdade para runtimes dinâmicos e reflection.

### 2.3 Runtime

Cada linguagem tem um runtime que:

- Implementa a codificação/decodificação do wire format.
- Fornece as classes/estruturas geradas.
- Oferece reflection (`Descriptor`, `FieldDescriptor`, `DynamicMessage`).
- Impõe limites de parsing (profundidade, bytes totais, mensagens grandes).

**Regra crítica:** gencode e runtime devem ser da **mesma versão exata**. Uma
diferença de versão pode causar falhas de linkage ou comportamento incorreto.
Atualizações de patch devem ser aplicadas em ambos.

## 3. Formato binário em uma frase

O wire format é um fluxo de **pares (tag, valor)** auto-delimitados. A tag
codifica `(número_do_campo << 3) | tipo_de_wire`. Valores escalares usam varint,
tamanho fixo ou comprimento prefixado; mensagens e strings usam
comprimento-prefixado.

Isso implica:

- O nome do campo **não** vai para o binário — só o número.
- Campos com valor default podem ou não ser serializados, conforme a presença.
- Campos desconhecidos são armazenados e re-serializados (unknown fields).

Detalhes completos em `05-wire-format.md`.

## 4. Compatibilidade e evolução

O contrato pode evoluir com segurança se respeitar:

| Mudança | Compatível? |
|---|---|
| Adicionar campo novo | Sim |
| Remover campo (com `reserved`) | Sim |
| Renomear campo (binário) | Sim; **quebra JSON/TextFormat** |
| Mudar tipo entre `int32`/`int64`/`uint32`/`uint64`/`bool` | Sim (varint) |
| Mudar `int32` para `sint32`/`fixed32` | **Não** |
| Adicionar valor a enum | Sim (se aberto) |
| Reutilizar número de campo | **Não** |
| Trocar `optional` por `repeated` | **Não** |

A disciplina completa (com `oneof`, maps, enums fechados/abertos e presença)
está em `15-evolucao-de-contrato.md`.

## 5. Presença de campos (intuição)

- **Sem presença** (proto3 implícito): não há como distinguir "não definido" de
  "definido como default"; o default não é serializado.
- **Com presença explícita** (`optional` em proto3, `EXPLICIT` em editions):
  há `has_x`/clear; o default definido é serializado.
- **oneof**: sempre tem presença explícita; só um membro por vez.
- **repeated/map**: nunca têm presença; vazio == ausente.

Isso afeta merging, patches parciais e round-trips. Ver `06-presenca-de-campos.md`.

## 6. Quando escolher protobuf

| Cenário | Recomendação |
|---|---|
| Microserviços / IPC / RPC | Protobuf + gRPC |
| Fila/streaming de eventos internos | Protobuf (ou Avro se colunar) |
| API pública para navegador | Protobuf + gRPC-Web/Connect, ou JSON |
| Arquivos de configuração | JSON/YAML/TOML |
| Logs legíveis | JSON estruturado (não protobuf binário) |
| Dados analíticos colunares | Parquet/Arrow |
| Cache/sessão interna | Protobuf (compacto) ou MessagePack |

Protobuf brilha onde **contrato forte + payload compacto + evolução segura**
importam mais que legibilidade humana.

## 7. Ecossistema de ferramentas

| Ferramenta | Uso |
|---|---|
| `protoc` | Compilador de referência |
| `protoc-gen-*` | Plugins de geração (go, grpc, js, etc.) |
| `buf` | Lint, detecção de breaking change, geração, registry |
| `grpcurl` | Cliente gRPC via reflection/descriptor |
| `protoc-gen-validate` / `protovalidate` | Validação declarativa de campos |
| `grpc-gateway` | Tradução gRPC ↔ REST/JSON |
| `Connect` | RPC compatível com HTTP/1.1, HTTP/2 e gRPC |
| `prototiller` | Migração de proto2/proto3 para editions |

## 8. Instalação

### 8.1 `protoc` pré-compilado

Baixe `protoc-$VERSION-$PLATFORM.zip` na página de releases. Contém o binário e
os `.proto` padrão (`google/protobuf/*.proto`).

### 8.2 Build do fonte (C++)

```bash
cmake . -DCMAKE_BUILD_TYPE=Release
cmake --build . --parallel
# gera o binário `protoc`
```

### 8.3 Por linguagem

| Linguagem | Runtime |
|---|---|
| C++ | `libprotobuf` (build do fonte / vcpkg / Conan) |
| Java/Kotlin | `protobuf-java` / `protobuf-javalite` (Maven) |
| Python | `pip install protobuf` |
| Go | `google.golang.org/protobuf` + `protoc-gen-go` |
| C# | `Google.Protobuf` (NuGet) |
| Rust | crate `protobuf` + `protoc` (ou `prost`/`buffa`) |
| JS/TS | `google-protobuf` / `@bufbuild/protobuf` |
| PHP | extensão `protobuf` ou `google/protobuf` |
| Ruby | gem `google-protobuf` |

**Sempre** alinhe a versão do runtime com a do `protoc` que gerou o código.

### 8.4 Bazel / CMake / Buf

- Bazel: `bazel_dep(name = "protobuf", version = "...")` (Bzlmod) ou
  `http_archive` + `protobuf_deps()`.
- CMake: `protobuf_generate()` / `protobuf_generate_cpp()`.
- Buf: `buf.yaml` + `buf.gen.yaml`, sem necessidade de instalar `protoc`.

## 9. Hello, world

```proto
// greet.proto
edition = "2023";
package demo;

message Greeting {
  string name = 1;
  repeated string tags = 2;
}
```

```bash
protoc --proto_path=. --python_out=gen greet.proto
```

```python
from gen import greet_pb2

g = greet_pb2.Greeting(name="Ana", tags=["dev", "pt-BR"])
data = g.SerializeToString()
g2 = greet_pb2.Greeting()
g2.ParseFromString(data)
assert g2.name == "Ana"
assert g2.tags == ["dev", "pt-BR"]
```

## 10. Limites e custos

- **Tamanho de payload**: gRPC limita 4 MiB por mensagem por padrão; o wire
  format não impõe limite intrínseco, mas runtimes impõem defaults.
- **Profundidade de aninhamento**: limitada (default 100 no `protoc`/runtimes).
- **Número de campos**: máximo de tag `2^29 - 1`.
- **Não é auto-descritivo**: sem o `.proto`/descritor, o binário é opaco.
- **Não é criptográfico**: não oferece confidencialidade nem integridade;
  use TLS/criptografia por cima.

## 11. Próximos passos

- Escrever contrato: `02-contrato-do-proto.md` e `03-editions-e-features.md`.
- Escolher tipos: `04-tipos-e-numeros.md`.
- Entender o binário: `05-wire-format.md`.
