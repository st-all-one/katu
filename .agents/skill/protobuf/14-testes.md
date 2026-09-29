# 14 — Testes e conformance

## 1. Camadas de teste

| Camada | O que valida | Ferramenta |
|---|---|---|
| Round-trip | serialize → parse → igual | testes unitários |
| Golden/vectors | bytes esperados estáveis | arquivos de referência |
| Conformance | conformidade com a spec do wire | `conformance_test_runner` |
| Fuzzing | robustez a entrada malformada | libFuzzer/OSS-Fuzz, property-based |
| Interop | compatibilidade entre linguagens/versões | testes cruzados |
| Contrato | lint e breaking change | `buf lint`/`buf breaking` |
| RPC | serviços gRPC | mocks, integração, `grpcurl` |
| Validação | regras de negócio no schema | `protovalidate`/PGV |
| Performance | regressão de tamanho/latência | benchmarks |

## 2. Testes unitários e round-trip

O teste mais importante: **serializar e parsear de volta**, verificando
igualdade semântica e presença.

```python
import pytest
from acme.v1 import order_pb2

def test_roundtrip():
    original = order_pb2.Order(
        id="o-1",
        amount_minor=1999,
        currency="BRL",
        status=order_pb2.STATUS_PAID,
        items=[order_pb2.LineItem(sku="SKU", quantity=2)],
        coupon_code="",
    )
    data = original.SerializeToString()
    parsed = order_pb2.Order()
    parsed.ParseFromString(data)
    assert parsed == original
```

```java
@Test void roundTrip() throws Exception {
  Order original = Order.newBuilder()
      .setId("o-1").setAmountMinor(1999).build();
  Order parsed = Order.parseFrom(original.toByteArray());
  assertEquals(original, parsed);
  assertTrue(parsed.hasCouponCode() == original.hasCouponCode());
}
```

### 2.1 Testar presença

```cpp
Order o;
o.set_coupon_code("");
EXPECT_TRUE(o.has_coupon_code());
EXPECT_EQ(o.coupon_code(), "");
```

### 2.2 Testar defaults e ausência

- Verifique que campos não definidos **não** aparecem ou comportam-se conforme
  o esperado.
- Teste merge (`MergeFrom`) para patches parciais.

### 2.3 Testar determinismo

```python
a = msg.SerializeToString(deterministic=True)
b = msg.SerializeToString(deterministic=True)
assert a == b
```

## 3. Golden files / test vectors

- Armazene payloads binários de referência e seus valores esperados.
- Detecta quebras de wire format entre versões.
- Combine com hash determinístico do payload.
- Para JSON: armazene o JSON canônico esperado e teste parse/print.

## 4. Conformance tests

O repositório oficial fornece o **runner** de conformidade:

```bash
# build do runner (C++)
cmake . -Dprotobuf_BUILD_CONFORMANCE=ON && cmake --build .
# ou
bazel test //src:conformance_test
```

O runner conversa com um executável da sua implementação por pipe, usando
`conformance/conformance.proto`. Ele testa:

- parsing/serialização de todos os tipos;
- campos desconhecidos;
- JSON;
- UTF-8;
- oneof, maps, grupos;
- casos de erro.

Uma implementação de runtime **deve** passar na conformance. Use-a como
referência ao escrever/validar um runtime.

## 5. Fuzzing

- O projeto usa OSS-Fuzz extensivamente. Faça o mesmo com seus parsers.
- Fuzz targets: parse binário, parse JSON, `Any` unpack, validação.
- Use entradas estruturadas (mutar mensagens reais) além de byte streams.

```cpp
extern "C" int LLVMFuzzerTestOneInput(const uint8_t* data, size_t size) {
  Order o;
  if (o.ParseFromArray(data, size)) {
    std::string out;
    o.SerializeToString(&out);
  }
  return 0;
}
```

Property-based testing (Hypothesis/fast-check/proptest): gere mensagens
aleatórias e verifique invariantes (round-trip, idempotência de parse).

## 6. Testes de interoperabilidade

- Gere código em múltiplas linguagens a partir do **mesmo** `.proto` e cruze
  payloads (Python → Java → Go → C++).
- Teste versões diferentes de runtime lendo o mesmo binário (compatibilidade).
- Teste editions misturadas (proto3 e editions 2023 interoperam no wire).
- Teste gRPC entre linguagens.

## 7. Lint e breaking change (CI)

```bash
buf lint
buf format --diff --exit-code
buf breaking --against '.git#branch=main'
```

Regras comuns de breaking: remoção de campo/serviço/método, mudança de tipo,
reutilização de número, mudança de `repeated`↔singular, remoção de valor de
enum em uso. Integre ao CI e bloqueie merge em violação.

## 8. Testes de serviços gRPC

- **Unitários**: implemente o serviço e chame via stub in-process
  (`bufconn`/`InProcessServer`/`grpc_testing`).
- **Contrato**: valide requests/responses, status codes, metadata.
- **Erros**: force cada status relevante (`INVALID_ARGUMENT`, `NOT_FOUND`,
  `DEADLINE_EXCEEDED`, `RESOURCE_EXHAUSTED`).
- **Streaming**: teste ordem, cancelamento, backpressure e encerramento.
- **Deadlines/timeouts**: simule lentidão.
- **Interop**: `grpcurl` com reflection em ambiente de teste.

```go
func TestGetOrder(t *testing.T) {
  lis := bufconn.Listen(1024 * 1024)
  srv := grpc.NewServer()
  orderpb.RegisterOrderServiceServer(srv, &server{})
  go srv.Serve(lis)
  // ... dial com grpc.WithContextDialer(bufconnDialer) ...
}
```

## 9. Testes de validação de schema

Use `protovalidate` (ou PGV) para expressar regras no `.proto` e testá-las:

```proto
import "buf/validate/validate.proto";

message Order {
  string id = 1 [(buf.validate.field).string.uuid = true];
  int64 amount_minor = 2 [(buf.validate.field).int64.gte = 0];
  repeated LineItem items = 3 [(buf.validate.field).repeated.min_items = 1];
}
```

Teste casos válidos e inválidos em CI para garantir que as regras refletem o
domínio.

## 10. Testes de performance

- Benchmarks de serialize/parse com payloads representativos.
- Monitore **tamanho** serializado — regressão de tamanho é regressão de custo.
- Compare versões de contrato/runtime.
- Ferramentas: `hyperfine`, JMH, `pytest-benchmark`, `go test -bench`,
  `cargo bench`.

## 11. Testes de evolução

- Adicione/remova campos e rode o suite contra dados "antigos" persistidos.
- Verifique que mensagens antigas parseiam e que unknown fields sobrevivem.
- Teste migração proto2/proto3 → editions (round-trip sem mudança de wire).
- Congele golden files por versão e compare.

## 12. Matriz de testes recomendada

| Dimensão | Casos |
|---|---|
| Tipos | todos os escalares, enum, msg, map, oneof, WKT |
| Presença | explícita/implícita, default definido, ausente |
| Compatibilidade | add/remove field, enum novo, unknown fields |
| Formato | binário, JSON, TextFormat |
| Segurança | entrada truncada, profundidade, UTF-8, `Any` |
| RPC | unary, 3 tipos de streaming, erros, deadlines |
| Versões | runtime antigo × novo, editions × proto3 |

## 13. Checklist de testes

- [ ] Round-trip binário para cada mensagem.
- [ ] Presença testada explicitamente.
- [ ] Determinismo testado quando usado.
- [ ] Golden files versionados.
- [ ] `buf lint` + `buf breaking` no CI.
- [ ] Fuzzing nos parsers.
- [ ] Interop entre linguagens.
- [ ] Serviços gRPC com erros e streaming.
- [ ] Validação (protovalidate) com casos válidos/inválidos.
- [ ] Benchmarks de tamanho/latência.
