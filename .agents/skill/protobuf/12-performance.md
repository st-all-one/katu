# 12 — Performance

Protobuf é compacto e rápido, mas não "de graça". O ganho vem de escolhas de
schema, uso correto das APIs e tuning do runtime.

## 1. Princípios

1. **Meça antes de otimizar** com payloads reais (tamanho e perfil de acesso).
2. **Wire format**: tags menores, varints curtos, packing e compressão.
3. **Schema**: tipos e números corretos evitam bytes desnecessários.
4. **Runtime**: arenas, strings sem cópia, reuso de objetos, lazy fields.
5. **RPC**: streaming, keepalive, limites e pooling.
6. Cuidado: otimizações de wire (ex.: `CORD`, `VIEW`) são **específicas de
   linguagem** e não mudam a interoperabilidade.

## 2. Escolhas de schema que economizam bytes

| Situação | Escolha | Ganho |
|---|---|---|
| Negativos frequentes | `sint32`/`sint64` (zigzag) | até 10→poucos bytes |
| Campos mais usados | números 1–15 | tag de 1 byte vs 2+ |
| IDs/hashes grandes | `fixed32`/`fixed64`/`uint*` | evita varint de 5–10 bytes |
| Listas de escalares | `PACKED` | remove tag por elemento |
| Strings curtas e repetidas | considerar `bytes`/enum | menos overhead |
| Valores grandes | `bytes` comprimidos no app | gzip/zstd |
| Muitos campos opcionais não usados | separar em submensagens | evita preset |
| `float` para dinheiro | **errado** | use inteiro |
| Evitar repetir campos grandes em cada mensagem | referenciar por ID/`Any` | — |

## 3. C++ — alocação e strings

### 3.1 Arenas

Arenas alocam todos os objetos de uma mensagem em um bloco contíguo e os
liberam de uma vez, evitando `malloc`/`free` por campo.

```bash
# compilar com suporte
-DPROTOBUF_USE_ARENA=1   # ou habilitar nas opções de build
```

```proto
option cc_enable_arenas = true;
```

```cpp
google::protobuf::Arena arena;
auto* order = google::protobuf::Arena::CreateMessage<Order>(&arena);
// sem delete; arena libera tudo
```

Ganhos típicos de 20–50% em cenários de many-to-many de mensagens pequenas.

### 3.2 Strings: `VIEW`, `CORD`, `STRING`

Feature C++ (`features.(pb.cpp).string_type`):

| Valor | Tipo gerado | Uso |
|---|---|---|
| `STRING` | `std::string` | default clássico (cópia) |
| `VIEW` | `std::string_view` | evita cópia em leitura; default em edition 2024 |
| `CORD` | `absl::Cord` | strings grandes/fragmentadas |

```proto
string name = 1 [features.(pb.cpp).string_type = VIEW];
```

`VIEW` reduz cópias, mas exige que o buffer de origem sobreviva. `CORD` é
eficiente para strings grandes e concatenadas.

### 3.3 Repetidos

- `features.(pb.cpp).repeated_type`: escolha entre `std::vector`, `RepeatedField`,
  arrays de tamanho fixo, etc., conforme o caso.
- `LazyField`/`lazy` para submensagens grandes raramente acessadas.

### 3.4 Outros

- Compile com `-O2`/`-O3` e LTO quando fizer sentido.
- Evite cópias: use `mutable_x()` e escreva direto, em vez de construir uma
  temporária e copiar.
- `Swap` em vez de `CopyFrom` quando não precisar do original.
- Reutilize buffers e mensagens em loops (clear + reparse).

## 4. Parsing e limites

Todo parse de entrada não-confiável deve ter limites — além da segurança,
limites previsíveis evitam picos de memória e latência.

```cpp
google::protobuf::io::CodedInputStream in(data, size);
in.SetRecursionLimit(64);
in.SetTotalBytesLimit(4 << 20);   // 4 MiB
order.ParseFromCodedStream(&in);
```

```go
proto.UnmarshalOptions{RecursionLimit: 64}.Unmarshal(data, &msg)
```

## 5. Serialização

- **Evite** serializar todos os campos se apenas alguns mudam (diffs,
  `FieldMask`).
- **Determinismo** tem custo (ordenação) — habilite só quando necessário
  (hash/assinatura/golden).
- **Caches**: serialize uma vez e reutilize os bytes; não serialize dentro de
  loops quentes.
- **Streaming**: para coleções grandes, serialize em streaming
  (`CodedOutputStream`) ou use RPC de streaming em vez de uma mensagem gigante.

## 6. Compressão

- Protobuf já é compacto; comprimir ajuda em payloads grandes e repetitivos.
- gRPC tem compressão por mensagem (`gzip`, `deflate`, `snappy`).
- **Não** comprima dados já comprimidos/imagens.
- Compressão tem custo de CPU: meça o trade-off.
- Nunca comprima dados sensíveis com entrada atacante controlada sem avaliar
  CRIME/BEAST.

## 7. gRPC tuning

| Parâmetro | Efeito |
|---|---|
| `max_send/recv_message_length` | Evita `RESOURCE_EXHAUSTED`; alinhe cliente/servidor |
| Keepalive time/timeout | Detecta conexões mortas rapidamente |
| HTTP/2 window size | Throughput em streams longos |
| `GRPC_ARG_HTTP2_MAX_FRAME_SIZE` | Frames maiores |
| Connection pooling / canais reutilizados | Evita handshake por chamada |
| Streaming vs unary | Streaming para grandes volumes |
| Compression | Reduz banda, aumenta CPU |
| Deadlines | Evita threads presas |
| Load balancing | Distribui carga; least-request/ring-hash |

Sempre **alinhe** limites e keepalive entre cliente e servidor para evitar
`GOAWAY`.

## 8. Padrões de uso eficiente por linguagem

| Linguagem | Dicas |
|---|---|
| C++ | Arenas, `VIEW`/`CORD`, `mutable_*`, mover/swap, lazy fields |
| Java | Dotar builders completos antes de `build()`; reutilizar builders; `ByteString` evita cópias; `javalite` para Android |
| Python | Extensão C (não puro-Python), reutilizar mensagens, evitar atribuição em loop criando cópias |
| Go | `proto.MarshalOptions`/`UnmarshalOptions`, evitar reflection, `[]byte` reutilizado, `proto.Size` |
| C# | `ByteString`, reutilizar `CodedInputStream`, evitar boxing |
| Rust | `prost`/`buffa`, buffers reutilizados, `Bytes` sem cópia |
| JS/TS | `@bufbuild/protobuf` (ESM), evitar conversões JSON em caminho quente |

## 9. Antipadrões de performance

- Mensagem monolítica com dezenas de campos grandes sempre enviados.
- Recalcular serialização a cada acesso.
- `map` onde uma lista ordenada bastaria (ou vice-versa).
- `Any`/`Struct` para dados quentes (overhead de tipo dinâmico).
- `int32` para valores negativos (10 bytes).
- `repeated` não empacotado para listas longas de números.
- Copiar mensagens grandes em vez de mover/referenciar.
- Habilitar determinismo sempre.
- Parsear novamente o mesmo payload várias vezes.
- Criar canal gRPC por chamada.

## 10. Medição

- Benchmarks por linguagem no repositório (`benchmarks/`).
- Meça: tamanho serializado, tempo de parse, tempo de serialize, alocações,
  throughput, p99 de latência.
- Ferramentas: `hyperfine`/JMH/`pytest-benchmark`/`go test -bench`, profiling
  (perf, async-profiler, pprof), tracing (OTel).
- Use payloads representativos; microbenchmarks de mensagens vazias enganam.

## 11. Checklist de performance

- [ ] Campos `1`–`15` para os mais usados.
- [ ] `sint*` para negativos; `fixed*` para IDs grandes.
- [ ] `PACKED` em listas de escalares.
- [ ] Arenas (C++) / builders reutilizados (Java).
- [ ] Strings sem cópia quando a origem sobrevive (`VIEW`).
- [ ] Limites de parse configurados.
- [ ] Serialização cacheada onde aplicável.
- [ ] Compressão avaliada com medição.
- [ ] Canais gRPC reutilizados; keepalive/limites alinhados.
- [ ] Benchmarks em CI para regressão.
