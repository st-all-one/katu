# 07 — Geração de código e build

## 1. `protoc`

Compilador de referência. Fluxo: parseia `.proto` → resolve imports/tipos →
valida → gera `FileDescriptorProto` → invoca plugins.

```bash
protoc [opções] arquivo.proto ...
```

Opções essenciais:

| Opção | Efeito |
|---|---|
| `-I, --proto_path=DIR` | Raiz de busca de imports (repetível) |
| `--<lang>_out=DIR` | Gera código para a linguagem (`cpp`, `java`, `python`, ...) |
| `--grpc_<lang>_out=DIR` | Gera stubs gRPC |
| `--descriptor_set_out=FILE` | Escreve `FileDescriptorSet` (binário) |
| `--include_imports` | Inclui dependências no descriptor set |
| `--include_source_info` | Inclui `SourceCodeInfo` (posições/comentários) |
| `--dependency_out=FILE` | Grava dependências (para build systems) |
| `--error_format=FORMAT` | Formato do erro (`gcc`, `msvs`) |
| `--experimental_allow_proto3_optional` | Legado (pré-3.15) |
| `--encode=MSG` / `--decode=MSG` | Converte entre binário e texto |
| `--print_free_field_numbers` | Lista números livres |
| `--fatal_warnings` | Trata warnings como erro |

### 1.1 Ordem de `--proto_path`

`protoc` procura imports **apenas** em `--proto_path`, e o caminho lógico do
import é relativo a essas raízes. Defina sempre `-I.` (ou a raiz do repo) e
referencie os arquivos de forma consistente; do contrário, erros de import
"file not found" ou nomes duplicados.

### 1.2 Exemplos

```bash
# C++
protoc -I. --cpp_out=gen proto/acme/v1/order.proto

# Python (+ type stubs + gRPC)
protoc -I. --python_out=gen --pyi_out=gen --grpc_python_out=gen \
  proto/acme/v1/order.proto

# Java
protoc -I. --java_out=gen --grpc_java_out=gen proto/acme/v1/order.proto

# Go
protoc -I. --go_out=gen --go-grpc_out=gen proto/acme/v1/order.proto

# Descriptor set para reflection/ferramentas
protoc -I. --descriptor_set_out=bundle.desc --include_imports \
  proto/acme/v1/order.proto
```

## 2. Plugins por linguagem

`protoc` é extensível: qualquer executável `protoc-gen-<name>` no `PATH`
habilita `--<name>_out`. Plugins recebem `CodeGeneratorRequest` (com
descritores) e devolvem `CodeGeneratorResponse` (arquivos gerados).

| Linguagem | Plugin | Runtime |
|---|---|---|
| C++ | embutido (`--cpp_out`) | libprotobuf |
| Java/Kotlin | embutido (`--java_out`) | protobuf-java/javalite |
| Python | embutido (`--python_out`) | protobuf |
| C# | embutido (`--csharp_out`) | Google.Protobuf |
| Ruby | embutido (`--ruby_out`) | google-protobuf |
| PHP | embutido (`--php_out`) | google/protobuf |
| Go | `protoc-gen-go` | google.golang.org/protobuf |
| JS/TS | `protoc-gen-js` / `@bufbuild/protoc-gen-es` | google-protobuf / @bufbuild |
| Rust | `protoc-gen-rust` (ou `prost`, `buffa`) | crate `protobuf` |
| gRPC (genérico) | `protoc-gen-grpc-<lang>` | runtime gRPC |

Boa prática: **pin** a versão de cada plugin (mesmo commit/versão do runtime).

## 3. Buf (recomendado)

Buf substitui a invocação manual de `protoc`, com lint, breaking-change e
registry.

### 3.1 `buf.yaml` (módulo)

```yaml
version: v2
modules:
  - path: proto
lint:
  use:
    - STANDARD
breaking:
  use:
    - FILE
```

### 3.2 `buf.gen.yaml` (geração)

```yaml
version: v2
managed:
  enabled: true
plugins:
  - remote: buf.build/protocolbuffers/python
    out: gen/py
  - remote: buf.build/grpc/python
    out: gen/py
  - local: protoc-gen-go
    out: gen/go
    opt: paths=source_relative
```

### 3.3 Comandos

```bash
buf lint                                  # estilo e convenções
buf format -w                             # formata .proto
buf breaking --against '.git#branch=main' # detecta quebras vs main
buf generate                              # gera código
buf build -o image.bin                    # compila módulo (imagem BSR)
buf push                                  # publica no BSR
```

Vantagens: sem instalar `protoc`, lint/breaking determinísticos, registry
versionado, `protovalidate` e plugins remotos.

## 4. Bazel

```starlark
# MODULE.bazel
bazel_dep(name = "protobuf", version = "37.0")
```

```starlark
# BUILD.bazel
proto_library(
    name = "order_proto",
    srcs = ["order.proto"],
    deps = ["@com_google_protobuf//:timestamp_proto"],
)

cc_proto_library(name = "order_cc", deps = [":order_proto"])
py_proto_library(name = "order_py", deps = [":order_proto"])
```

- `proto_library` é a unidade de compilação de schema.
- Regras `*_proto_library` geram código por linguagem.
- Herda `--proto_path` das deps; sem cópias/duplicações.
- Use `strip_import_prefix`/`import_prefix` corretamente para os caminhos.

## 5. CMake

```cmake
find_package(Protobuf REQUIRED)
protobuf_generate_cpp(PROTO_SRCS PROTO_HDRS order.proto)
add_executable(app main.cc ${PROTO_SRCS} ${PROTO_HDRS})
target_link_libraries(app protobuf::libprotobuf)
```

Para gRPC:

```cmake
add_executable(server server.cc ${PROTO_SRCS})
target_link_libraries(server grpc++ protobuf::libprotobuf)
```

Ver `docs/cmake_protobuf_generate.md` e `docs/cpp_build_systems.md`.

## 6. Por linguagem — notas

### 6.1 C++

- Gera `.pb.h`/`.pb.cc`. Recomenda-se `option cc_enable_arenas = true;`.
- `optimize_for = SPEED` (default), `CODE_SIZE`, `LITE_RUNTIME`.
- Strings: `features.(pb.cpp).string_type` (`STRING`/`VIEW`/`CORD`).
- Sem RTTI/exceptions opcional via macro de build.

### 6.2 Java/Kotlin

- `java_multiple_files`, `java_package`, `java_outer_classname`.
- `protobuf-javalite` para Android (menor, sem reflection completa).
- Kotlin usam os mesmos stubs via extensões.

### 6.3 Python

- Gera `_pb2.py` (mensagens) e `_pb2_grpc.py` (serviços).
- `pyi_out` gera stubs de tipo.
- Runtime puro-Python vs extensão C (mais rápido e mais endurecido).
- Cuidado com colisão de nomes de módulos e `sys.path`.

### 6.4 Go

- `--go_out` + `option go_package = "modulo/path;nomepacote"`.
- `--go-grpc_out` para serviços.
- `paths=source_relative` mantém a estrutura de pastas.

### 6.5 C#

- `csharp_namespace` controla o namespace gerado.
- Pacote NuGet `Google.Protobuf`.

### 6.6 Rust

- `protobuf` (oficial-ish) ou crates alternativos (`prost`, `buffa`).
- `protoc` externo é necessário para gerar.

### 6.7 JS/TS

- `google-protobuf` (runtime clássico) ou `@bufbuild/protobuf` (ESM, moderno).
- gRPC-Web ou Connect para navegador.

## 7. Descriptor sets e reflection

- `--descriptor_set_out` produz um `FileDescriptorSet` — usado para reflection
  em runtime, `grpcurl`, gateways, validação e migração.
- Em runtimes dinâmicos (`DynamicMessage`), o descritor é carregado e as
  mensagens são manipuladas por nome.
- `--include_source_info` preserva comentários e posições (IDEs, docs).

## 8. Geração em CI/CD

Boas práticas:

1. Um passo de build reproduzível gera todo o código (`buf generate`/Bazel).
2. **Não** versione código gerado se o build o gera; se versionar, marque como
   gerado e sincronize.
3. Rode `buf lint` e `buf breaking --against main` em PRs.
4. Pin de plugins e runtime (lockfile).
5. Cacheie os binários de plugins (evita downloads por build).
6. Gere o descriptor set como artefato para tooling/observabilidade.

## 9. Troubleshooting de build

| Sintoma | Causa provável |
|---|---|
| `File not found` em import | `--proto_path` errado ou caminho lógico inconsistente |
| `Duplicate symbol` | Dois `.proto` com mesmo `package`/tipo |
| `java_outer_classname` conflito | Dois arquivos com mesmo outer classname |
| `go_package` ausente | Opção obrigatória para Go |
| Gencode/runtime incompatível | Versões diferentes de `protoc` e runtime |
| `.proto` do futuro | `edition`/feature não suportada pelo `protoc` |
| Import público/weak problemático | Uso de `public`/`weak` (evite) |

## 10. Referência de flags adicionais

```bash
protoc --help
protoc --version
protoc --decode_raw < data.bin          # depurar wire sem schema
protoc --decode=acme.v1.Order order.proto < data.bin
protoc --encode=acme.v1.Order order.proto < data.txt > data.bin
protoc --print_free_field_numbers order.proto
```

`--decode_raw` é extremamente útil para inspecionar payloads sem o schema.
