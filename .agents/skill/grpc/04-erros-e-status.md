# 04 — Erros e status codes

> Fonte: `grpc_docs/doc/statuscodes.md`, `status_ordering.md`,
> `http-grpc-status-mapping.md`.

## 1. Tabela de status

| Código | Nº | Significado |
|---|---|---|
| `OK` | 0 | Sucesso |
| `CANCELLED` | 1 | Cancelado pelo chamador |
| `UNKNOWN` | 2 | Erro desconhecido |
| `INVALID_ARGUMENT` | 3 | Argumento inválido, independente de estado |
| `DEADLINE_EXCEEDED` | 4 | Deadline estourou |
| `NOT_FOUND` | 5 | Entidade não encontrada |
| `ALREADY_EXISTS` | 6 | Entidade já existe |
| `PERMISSION_DENIED` | 7 | Sem permissão |
| `RESOURCE_EXHAUSTED` | 8 | Recurso esgotado (quota/limite) |
| `FAILED_PRECONDITION` | 9 | Estado do sistema impede a operação |
| `ABORTED` | 10 | Concorrência/transação abortada |
| `OUT_OF_RANGE` | 11 | Fora da faixa válida |
| `UNIMPLEMENTED` | 12 | Não implementado/suportado |
| `INTERNAL` | 13 | Erro interno (invariante quebrada) |
| `UNAVAILABLE` | 14 | Indisponível (transiente) |
| `DATA_LOSS` | 15 | Perda/corrupção irrecuperável |
| `UNAUTHENTICATED` | 16 | Sem credenciais válidas |

## 2. Escolhas corretas

| Situação | Código |
|---|---|
| Campo obrigatório ausente | `INVALID_ARGUMENT` |
| Recurso não existe | `NOT_FOUND` |
| Criar recurso que já existe | `ALREADY_EXISTS` |
| Sem permissão (identificado) | `PERMISSION_DENIED` |
| Sem credencial | `UNAUTHENTICATED` |
| Quota/limite de recurso | `RESOURCE_EXHAUSTED` |
| Estado inválido (não retry até corrigir) | `FAILED_PRECONDITION` |
| Conflito otimista/transação | `ABORTED` |
| Valor além do intervalo | `OUT_OF_RANGE` |
| Método não implementado | `UNIMPLEMENTED` |
| Bug/invariante | `INTERNAL` |
| Serviço fora do ar | `UNAVAILABLE` |
| Corrupção de dados | `DATA_LOSS` |

> Diferenças que confundem:
> - `FAILED_PRECONDITION` (não retry; corrigir estado) × `ABORTED` (retry
>   em nível superior) × `UNAVAILABLE` (retry com backoff).
> - `PERMISSION_DENIED` (identificado) × `UNAUTHENTICATED` (sem credencial).
> - `INVALID_ARGUMENT` (sempre inválido) × `FAILED_PRECONDITION` (depende do
>   estado).

## 3. Status gerados pela biblioteca

| Caso | Código | Onde |
|---|---|---|
| Cliente cancela | `CANCELLED` | ambos |
| Deadline expira | `DEADLINE_EXCEEDED` | ambos |
| Método não encontrado | `UNIMPLEMENTED` | servidor |
| Servidor desligando | `UNAVAILABLE` | servidor |
| Exceção não tratada no handler | `UNKNOWN` | servidor |
| Falha de conexão | `UNAVAILABLE` | cliente |
| Compressão não suportada | `UNIMPLEMENTED` | servidor |
| Falha ao descomprimir | `INTERNAL` | ambos |
| Limite de recursos (flow control) | `RESOURCE_EXHAUSTED` | servidor |
| Resposta grande demais para memória | `RESOURCE_EXHAUSTED` | cliente |
| Violação de flow control | `INTERNAL` | ambos |
| Erro ao parsear status | `UNKNOWN` | cliente |
| Credencial/auth inválida | `UNAUTHENTICATED` | ambos |
| Cardinalidade (método unary com N reqs) | `UNIMPLEMENTED` | servidor |
| Erro ao parsear proto | `INTERNAL` | ambos |
| Mensagem acima do limite | `RESOURCE_EXHAUSTED` | ambos |
| Keepalive watchdog | `UNAVAILABLE` | ambos |

Nunca gerados pela biblioteca (só pela aplicação): `INVALID_ARGUMENT`,
`NOT_FOUND`, `ALREADY_EXISTS`, `FAILED_PRECONDITION`, `ABORTED`, `OUT_OF_RANGE`,
`DATA_LOSS`.

## 4. Erros ricos (`google.rpc.Status`)

O status do wire é `(code, message)`. Para detalhes estruturados, use
`google.rpc.Status` com `details` do tipo `Any`, empacotando mensagens padrão:

```proto
import "google/rpc/status.proto";
import "google/rpc/error_details.proto";

rpc CreateOrder (CreateOrderRequest) returns (Order) {
  option (google.api.http) = { post: "/v1/orders" body: "*" };
}
```

Detalhes comuns:

| Detalhe | Uso |
|---|---|
| `BadRequest` | violações por campo (`FieldViolation`) |
| `ErrorInfo` | `reason`, `domain`, `metadata` (máquina-legível) |
| `RetryInfo` | quando e quanto esperar antes de retry |
| `QuotaFailure` | violações de quota |
| `PreconditionFailure` | pré-condições violadas |
| `DebugInfo` | detalhes de debug (não expor em produção) |
| `Help` | links de ajuda |
| `LocalizedMessage` | mensagem localizada |

Exemplo (Go):

```go
st := status.New(codes.InvalidArgument, "pedido inválido")
st, _ = st.WithDetails(&errdetails.BadRequest{
    FieldViolations: []*errdetails.BadRequest_FieldViolation{
        {Field: "items", Description: "precisa de ao menos 1 item"},
    },
})
return nil, st.Err()
```

Leitura no cliente:

```go
st := status.Convert(err)
for _, d := range st.Details() {
    if br, ok := d.(*errdetails.BadRequest); ok { /* ... */ }
}
```

## 5. Mensagens de erro

- `grpc-message` é percent-encoded; mantenha curto e genérico.
- **Não** vaze stack traces, SQL, caminhos internos ou PII.
- Use `ErrorInfo.reason` (estável) para máquina; `message` para humano.
- Localize na borda, não no servidor.

## 6. Ordenação de status (status ordering)

Em streaming, se múltiplos erros ocorrem, a ordenação importa. Regras gerais:
termine com o status mais informativo; não converta um erro específico em
`UNKNOWN`. Em proxies/agregadores, preserve o status original quando possível.

## 7. Retry e backoff

- Retry apenas em códigos idempotentes: `UNAVAILABLE`, `RESOURCE_EXHAUSTED`,
  `ABORTED` (nível superior), `DEADLINE_EXCEEDED` (com cautela).
- **Nunca** retry em `INVALID_ARGUMENT`, `NOT_FOUND`, `ALREADY_EXISTS`,
  `PERMISSION_DENIED`, `UNAUTHENTICATED`, `FAILED_PRECONDITION`, `UNIMPLEMENTED`.
- Sempre **backoff exponencial + jitter** e **teto**.
- Use `RetryInfo` para orientar o cliente.
- A política de retry pode ser configurada via **service config**
  (`retryPolicy`) — ver `11`.

## 8. Mapeamento HTTP ↔ gRPC

Quando a resposta não tem `grpc-status` (intermediário), aplique a tabela de
`02-protocolo-http2-e-web.md`. Recíproco: gateways traduzem gRPC para HTTP.

## 9. Padrão de erro de aplicação

```proto
// Erro tipado opcional (complementa google.rpc.Status)
message ApiError {
  string code = 1;          // estável, ex.: "ORDER_ALREADY_PAID"
  string message = 2;       // humano
  repeated FieldViolation violations = 3;
}
message FieldViolation { string field = 1; string description = 2; }
```

- Use `ErrorInfo`/`BadRequest` padrão quando interoperar com o ecossistema.
- Mantenha um catálogo de `code` estáveis documentado.

## 10. Observabilidade de erros

- Logue `code`, `message`, `method`, `peer`, `trace_id` — sem payload.
- Métricas por código/método (`RED`).
- Alerta em picos de `INTERNAL`, `UNKNOWN`, `UNAVAILABLE`.
- Trate `UNKNOWN` de handler como bug (exceção não tratada).

## 11. Checklist de erros

- [ ] Status codes corretos para cada caso.
- [ ] Erros ricos com `google.rpc.Status` + `details`.
- [ ] `message` sem dados sensíveis/internos.
- [ ] Retry só em códigos idempotentes, com backoff+jitter.
- [ ] `RetryInfo` quando fizer sentido.
- [ ] Handler mapeia exceções para status (evita `UNKNOWN`).
- [ ] Logs/métricas por código e método.
