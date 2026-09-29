# 05 — Metadata, deadlines e cancelamento

## 1. Metadata

Metadata são pares chave/valor que viajam em headers/trailers HTTP/2, separados
do payload.

```go
// cliente: enviar
ctx = metadata.AppendToOutgoingContext(ctx, "authorization", "Bearer "+tok)

// servidor: ler
md, _ := metadata.FromIncomingContext(ctx)
vals := md.Get("x-tenant-id")
```

- Chaves: ASCII minúsculas (`[0-9a-z_.-]`).
- Valores ASCII imprimíveis; binário em chave com sufixo **`-bin`** (base64 no
  wire; a API costuma aceitar `[]byte`).
- `grpc-*` é reservado ao protocolo.
- Não coloque segredos em metadata sem TLS — headers podem ser logados.
- Limite de tamanho de headers varia (tipicamente ≤16 KiB por lista).

### 1.1 Usos comuns

| Chave | Uso |
|---|---|
| `authorization` | token bearer/JWT |
| `x-request-id`, `x-correlation-id` | correlação |
| `traceparent`, `tracestate` | W3C Trace Context |
| `x-tenant-id` | multi-tenancy |
| `x-idempotency-key` | deduplicação |
| `grpc-timeout` | deadline (gerenciado pelo runtime) |

### 1.2 Propagação entre serviços

- Interceptors devem **repassar** metadata relevante ao chamar dependências
  (auth, trace, tenant, request id).
- Não repasse headers que não se aplicam (ex.: `authorization` para outro
  domínio de confiança, a menos que intencional).

## 2. Deadlines

Um deadline é um instante absoluto após o qual a chamada é abortada com
`DEADLINE_EXCEEDED`.

```go
ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
defer cancel()
resp, err := client.GetOrder(ctx, req)
```

- Sempre defina um deadline; chamadas sem deadline podem pendurar indefinidamente.
- O runtime envia `grpc-timeout` (duração relativa) no header.
- O servidor deve **respeitar** o deadline e cancelar trabalho ao estourar.
- Propague o deadline às chamadas downstream; **não** aumente o timeout do
  downstream além do restante do upstream.

### 2.1 Regras

| Regra | Por quê |
|---|---|
| Toda chamada com deadline | Evita threads/recursos presos |
| Deadline menor na borda | Falha rápida e previsível |
| Propagar o restante do tempo | Evita trabalho órfão |
| Distinguir `DEADLINE_EXCEEDED` de `CANCELLED` | Diagnóstico |
| Não confundir deadline com timeout de retry | São coisas diferentes |

## 3. Cancelamento

```go
ctx, cancel := context.WithCancel(context.Background())
go func() { /* ... */ cancel() }()
resp, err := client.Watch(ctx, req)   // err = CANCELLED
```

- Cliente cancela via contexto/`CancellationToken`/`AbortSignal`.
- Servidor detecta cancelamento e deve **liberar recursos** e parar trabalho.
- Em streaming, cancelar encerra o stream; use `defer`/`finally` para limpeza.
- Propague cancelamento a dependências (DB, HTTP, filas).

## 4. `wait_for_ready`

Controla o comportamento quando o canal não está `READY`:

| Valor | Comportamento |
|---|---|
| `false` (default em muitas linguagens) | falha imediata (`UNAVAILABLE`) |
| `true` | a chamada aguarda o canal ficar pronto (até o deadline) |

```go
resp, err := client.GetOrder(ctx, req, grpc.WaitForReady(true))
```

Use `wait_for_ready` em chamadas que preferem esperar a falhar na hora
(ex.: jobs em lote). Cuidado: sem deadline, pode esperar muito.

## 5. `fail_fast`

- `fail_fast = true`: se o canal não está pronto, falha imediatamente.
- `fail_fast = false`: equivalente a aguardar (`wait_for_ready`), dependendo da
  linguagem.
- Combine com deadline e política de retry.

## 6. Metadata em trailers (resposta)

- O servidor pode enviar **trailing metadata** junto ao status:

  ```go
  grpc.SetTrailer(ctx, metadata.Pairs("x-result-count", "42"))
  ```

- Útil para contadores, paginação, quotas e informações pós-processamento.
- Limite de tamanho; não use para payload.

## 7. Padrões de propagação

```
Cliente → [auth, trace, tenant, request-id] → Serviço A
Serviço A → [auth(serviço), trace, tenant, request-id] → Serviço B
```

- Autenticação de serviço (mTLS) + autorização no servidor.
- Trace context sempre propagado.
- Tenant sempre validado (nunca confie no cliente).
- `request-id` para idempotência/correlação.

## 8. Boas práticas

1. Sempre deadline; nunca infinito.
2. Cancele recursos ao terminar (defer/finally).
3. Propague deadline e trace.
4. Não coloque payload em metadata.
5. Não confie em `tenant`/`user` de metadata sem validar.
6. Limite o número/tamanho de headers.
7. Logue metadata relevante (sem segredos).

## 9. Checklist

- [ ] Deadline em toda chamada e propagado.
- [ ] Cancelamento libera recursos.
- [ ] Metadata de auth/trace/tenant/request-id propagada corretamente.
- [ ] `wait_for_ready`/`fail_fast` definidos conforme o caso.
- [ ] Trailing metadata usada para contadores/quotas.
- [ ] Nenhum segredo em metadata sem TLS.
