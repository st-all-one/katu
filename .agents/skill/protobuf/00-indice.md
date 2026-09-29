# 00 — Protocol Buffers: visão geral e mapa do guia

> Versão de referência: **Protocol Buffers v37** (`protoc` 37.0-dev, main
> 2026-07-09), runtimes 37-dev.
> Guia denso em pt-BR, otimizado para consumo por IA, baseado no repositório
> oficial `protocolbuffers/protobuf`.
> Modelo de contrato atual: **Editions** (`edition = "2023" | "2024" | "2026"`).
> Sintaxes legadas `proto2`/`proto3` continuam suportadas.

## 1. Protobuf e gRPC são independentes

Esta skill cobre **protobuf puro**. gRPC é um transporte RPC opcional que usa o
mesmo `.proto`; para ele, carregue a skill irmã [`../grpc_guide/`](../grpc_guide/).

## 2. O que é Protocol Buffers

**Protocol Buffers (protobuf)** é um mecanismo agnóstico de linguagem e de
plataforma para **serializar dados estruturados**. O usuário escreve um schema
(`.proto`) que descreve tipos; um compilador (`protoc`) gera código-fonte em
diversas linguagens; o runtime de cada linguagem faz a codificação/decodificação
binária.

Três camadas:

| Camada | Papel |
|---|---|
| **IDL** (`.proto`) | Contrato imutável e versionado entre produtor/consumidor |
| **`protoc` + plugins** | Valida o schema e gera código (mensagens, getters, setters, serialização, stubs gRPC) |
| **Runtime** | Biblioteca por linguagem que serializa/parseia e oferece reflection |

O formato de fio (*wire format*) é **binário, TLV (tag-length-value)**, compacto
e projetado para evoluir com compatibilidade para frente e para trás.

## 2. Modelo mental

1. **Um `.proto` define tipos por nome e campos por número.** O número é a
   identidade binária; o nome é a identidade em JSON/TextFormat.
2. **Cada campo presente vira um par (tag, valor)** no fluxo. Campos ausentes
   não aparecem.
3. **Mudanças compatíveis** adicionam/removem campos sem renumerar; o parser
   ignora campos desconhecidos e os preserva (unknown fields).
4. **`protoc` é determinístico por schema**, não por valor: a serialização só é
   determinística quando explicitamente pedida.
5. **Editions** substituem a distinção proto2/proto3 por um conjunto de
   *features* com herança; uma edição é um pacote de defaults.
6. **RPC (opcional)** usa `.proto` como IDL de serviço; o payload continua
   protobuf binário. A skill de gRPC trata do transporte, não do formato.

## 3. Quadro de versões e sintaxes

| Modelo | Declaração no arquivo | Presença default | Enum default | Packing default |
|---|---|---|---|---|
| proto2 | `syntax = "proto2";` | explícita | fechado | expandido |
| proto3 | `syntax = "proto3";` | implícita | aberto | empacotado |
| editions 2023 | `edition = "2023";` | explícita | aberto* | ver feature |
| editions 2024 | `edition = "2024";` | explícita | aberto | ver feature |
| editions 2026 | `edition = "2026";` | explícita | aberto | ver feature |

\* o default de `enum_type` em editions é `OPEN`. Arquivos proto2 migrados
recebem `CLOSED` explícito.

Editions mudam **apenas defaults de features** (ou introduzem features novas);
não alteram o wire format por si só.

## 4. Quando usar (e quando não)

**Use protobuf quando:**
- Há comunicação entre serviços (especialmente gRPC) com necessidade de
  contrato versionado e payloads compactos.
- O schema é conhecido em tempo de compilação e a tipagem forte ajuda.
- É necessário evoluir o contrato sem quebrar clientes antigos.
- Persistência/streaming de registros estruturados com alto volume.

**Evite quando:**
- Precisa inspecionar/editar os dados manualmente com frequência → JSON/YAML.
- Análise colunar/tabular em escala → Parquet/Arrow/Avro.
- Schema totalmente dinâmico e definido em runtime pelo usuário final.
- Payload minúsculo e comunicação já em JSON (custo de toolchain > ganho).

## 5. Comparações rápidas

| Critério | Protobuf | JSON | XML | Avro | Thrift |
|---|---|---|---|---|---|
| Formato | binário | texto | texto | binário | binário |
| Schema | obrigatório | opcional | opcional (XSD) | obrigatório | obrigatório |
| Tamanho | muito baixo | alto | muito alto | baixo | baixo |
| Velocidade | alta | média | baixa | alta | alta |
| Evolução | excelente | boa | média | excelente | boa |
| Legível | não | sim | sim | não | não |
| RPC nativo (skill irmã) | gRPC | — | — | — | Thrift RPC |
| Ecossistema | amplo | universal | amplo | Hadoop/Kafka | menor |

## 6. Ecossistema

- **`protoc`**: compilador de referência (C++). Plugins por linguagem.
- **Buf**: CLI/registry moderno — `buf lint`, `buf breaking`, `buf generate`,
  Buf Schema Registry (BSR), ProtoJSON, `protovalidate`.
- **gRPC (skill irmã)**: RPC sobre HTTP/2/3 usando protobuf; gera stubs
  cliente/servidor. Ver [`../grpc_guide/`](../grpc_guide/).
- **grpc-gateway / Connect / gRPC-Web**: expõem serviços gRPC a HTTP/JSON/web
  (ver a skill de gRPC).
- **Well-Known Types**: tipos padronizados (`Timestamp`, `Duration`, `Any`,
  `Struct`, etc.) com mapeamento JSON especial.
- **Editions + Prototiller**: ferramentas de migração de sintaxe para editions.

## 7. Como este guia está organizado

| Bloco | Arquivos | Foco |
|---|---|---|
| Fundamentos | `01` | Conceitos, comparações, instalação |
| Contrato | `02`, `03`, `04` | Sintaxe `.proto`, editions/features, tipos |
| Dados | `05`, `06`, `09` | Wire format, presença, JSON/Texto |
| Build | `07`, `08` | Geração de código, WKT |
| Sem gRPC | `10` | Contrato rigoroso sem gRPC (HTTP/REST, filas, arquivos); PHP 7.2/Laravel 5.5 |
| Produção | `11`, `12`, `13` | Segurança, performance, logs |
| Qualidade | `14`, `15`, `16` | Testes, evolução, boas práticas |
| Apoio | `17`, `18` | Troubleshooting, referência |
| **RPC (skill irmã)** | `../grpc_guide/` | gRPC: protocolo, streaming, status, LB, xDS, auth + implementação por stack |

## 8. Fluxo recomendado para a IA

1. Identifique a tarefa: **definir contrato**, **gerar código**, **serializar**,
   **expor RPC**, **endurecer**, **otimizar**, **testar** ou **evoluir**.
2. Consulte o arquivo correspondente ao bloco acima.
3. Ao escrever um `.proto`, comece por `02` (gramática) e `03` (editions) e
   confirme tipos em `04`.
4. Ao revisar/evoluir, aplique obrigatoriamente `15` e `16`.
5. Para qualquer entrada não-confiável, leia `11` antes de implementar.

## 9. Regras de ouro (resumo)

- Número de campo é para sempre: **reserve, nunca reuse**.
- Enum: sempre `0 = *_UNSPECIFIED`.
- Prefira **presença explícita** a inferir por valor default.
- `string` só com UTF-8; caso contrário `bytes`.
- Mantenha gencode e runtime na **mesma versão**.
- Limite entradas não-confiáveis (tamanho/depth/bytes).
- Não logue payload sem **redação** de campos sensíveis.

## 10. Protobuf sem gRPC

Se o ambiente não suporta gRPC (ex.: PHP 7.2 sem `ext-grpc`), use
`10-protobuf-sem-grpc.md`: protobuf sobre HTTP/REST, filas, arquivos ou cache,
mantendo todo o rigor de contrato (`buf lint`/`buf breaking`, codegen, presença,
compatibilidade).

## 11. Skill irmã: gRPC

Esta skill cobre **protobuf puro**. Para RPC com gRPC — protocolo HTTP/2,
streaming, status codes, metadata/deadlines, interceptors, reflection/health,
compressão/keepalive, load balancing, service config, xDS, segurança,
observabilidade e **implementação por stack** (Rust, Go, Dart/Flutter,
TypeScript, web, PHP 7.2/Laravel 5.5, PHP 8.4/Laravel 12) — carregue
[`../grpc_guide/`](../grpc_guide/).

O mesmo `.proto` serve para ambos: a skill de protobuf define o contrato e a
serialização; a de gRPC define o transporte e o serviço.
