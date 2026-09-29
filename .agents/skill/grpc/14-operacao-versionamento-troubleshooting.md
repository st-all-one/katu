# 14 — Operação, versionamento e troubleshooting

> Fontes: `grpc_docs/doc/versioning.md`, `environment_variables.md`,
> `grpc_release_schedule.md`, `fork_support.md`, `workarounds.md`,
> `command_line_tool.md`, `c-style-guide.md`, `cpp-style-guide.md`,
> `g_stands_for.md`.

## 1. Versionamento

### 1.1 Do protocolo

- O **wire protocol** gRPC é estável; mudanças são compatíveis.
- O **IDL** evolui pelas regras do protobuf (nunca reutilizar número; `reserved`).
- Serviços versionam pelo **pacote** (`acme.orders.v1`), não pelo método.
- Adicionar método/campo é compatível; remover exige deprecação.

### 1.2 Das bibliotecas

- Mantenha **plugin de codegen** e **runtime** na mesma versão.
- Siga o release schedule oficial; RC antes do release.
- Atualize patches de segurança (gRPC e TLS).

### 1.3 Deprecação

```proto
service OrderService {
  rpc LegacyGet (GetRequest) returns (Order) { option deprecated = true; }
}
```

- Marque `deprecated`, mantenha por um período, depois remova.
- Nunca mude o `:path` de um método existente.

## 2. Variáveis de ambiente (C-core)

| Variável | Efeito |
|---|---|
| `grpc_proxy`, `https_proxy`, `http_proxy` | proxy HTTP CONNECT |
| `no_grpc_proxy`, `no_proxy` | hosts sem proxy |
| `GRPC_SSL_CIPHER_SUITES` | cifras OpenSSL |
| `GRPC_DEFAULT_SSL_ROOTS_FILE_PATH` | raízes PEM |
| `GRPC_POLL_STRATEGY` | engine de polling (epoll/poll) |
| `GRPC_TRACE` | tracers de debug |
| `GRPC_VERBOSITY` | nível de log (deprecado) |
| `GRPC_ABORT_ON_LEAKS` | aborta em leaks (debug) |
| `GOOGLE_APPLICATION_CREDENTIALS` | credenciais Google |
| `GRPC_ENABLE_FORK_SUPPORT` | suporte a `fork()` |

- Variáveis afetam **todo** o processo; use com cuidado.
- Em contêineres, defina explicitamente raízes/cifras.

## 3. Fork e processos

- gRPC em processos que usam `fork()` exige cuidado: threads e pollers não
  sobrevivem ao fork de forma limpa.
- Habilite suporte a fork (`GRPC_ENABLE_FORK_SUPPORT`) e inicialize antes do
  fork quando necessário.
- Prefira `fork`+`exec` ou inicialize gRPC no processo filho.

## 4. Workarounds e compatibilidade

O documento `workarounds.md` cataloga diferenças/interoperações entre
implementações (ex.: tcp_nodelay, cabeçalhos, compressão). Ao interoperar
linguagens, valide com os interop tests (`13-testes-e-interop.md`).

## 5. Estilo de código

- **C++/C-core**: `cpp-style-guide.md`/`c-style-guide.md` (Google style).
- Outras linguagens: siga o guia da linguagem + gRPC style config
  (`grpc-style-config.toml`).

## 6. Operação

### 6.1 Shutdown gracioso

1. Ao receber `SIGTERM`, marque health como `NOT_SERVING`.
2. Pare de aceitar novas RPCs.
3. Drene chamadas em andamento (com deadline).
4. Encerre (`GracefulStop`/equivalente).

```go
sig := make(chan os.Signal, 1)
signal.Notify(sig, syscall.SIGTERM, syscall.SIGINT)
<-sig
hs.SetServingStatus("", healthpb.HealthCheckResponse_NOT_SERVING)
stopped := make(chan struct{})
go func() { srv.GracefulStop(); close(stopped) }()
select {
case <-stopped:
case <-time.After(30 * time.Second):
    srv.Stop()
}
```

### 6.2 Deploy

- Rolling update com health checks e drenagem.
- LB/xDS para remover instâncias antes de desligar.
- Mantenha conexões quentes (evita handshake).

### 6.3 Recursos

- Limites de memória por mensagem/stream.
- Thread pools dimensionados.
- Timeouts de inatividade.

## 7. Troubleshooting

| Sintoma | Causa provável | Ação |
|---|---|---|
| `UNAVAILABLE` frequente | backend fora/rede | LB, retry com backoff, health |
| `GOAWAY "too_many_pings"` | keepalive agressivo | alinhar keepalive (`10`) |
| `RESOURCE_EXHAUSTED` | mensagem > limite | aumentar limite alinhado ou streaming |
| `DEADLINE_EXCEEDED` | deadline curto/lentidão | ajustar deadline/otimizar |
| `UNIMPLEMENTED` | método não registrado/proxy | verificar registro e `content-type` |
| `INTERNAL` no parse | payload inválido/proxy | validar schema/transporte |
| `UNKNOWN` | exceção não tratada no handler | mapear para status |
| `UNAUTHENTICATED` | credencial ausente/ inválida | verificar TLS/call creds |
| `PERMISSION_DENIED` | sem autorização | revisar política |
| `CANCELLED` inesperado | cancelamento/deadline cliente | investigar caller |
| Reflection vazia | não habilitada/descriptor ausente | `reflection.Register` + descriptor |
| Tráfego não chega | proxy HTTP/1.1 sem bridging | usar gRPC-Web/Connect/L7 HTTP/2 |
| Latência alta | `pick_first`, sem LB, TLS handshake | configurar LB, reusar canais |
| Conexões caindo | NAT/proxy idle | keepalive alinhado |

### 7.1 Diagnóstico

```bash
grpcurl -plaintext localhost:50051 list
grpcurl -plaintext localhost:50051 describe acme.orders.v1.OrderService
GRPC_TRACE=http,transport GRPC_VERBOSITY=DEBUG ./server
grpc_cli ls localhost:50051
```

- Verifique `content-type` e `:path` no proxy.
- Confirme HTTP/2 fim-a-fim (ALPN `h2`).
- Cheque certificados (datas, SAN, cadeia).
- Confirme limites de mensagem cliente/servidor.
- Verifique se compression codecs batem.

## 8. Release schedule

- Releases periódicos com RC; branches de release.
- Consuma RC em CI para antecipar quebras.
- Acompanhe deprecações da sua linguagem.

## 9. Checklist operacional

- [ ] Shutdown gracioso (health + drenagem) implementado.
- [ ] Health checks e LB configurados.
- [ ] Variáveis de ambiente relevantes documentadas.
- [ ] TLS/cifras/raízes definidos.
- [ ] Keepalive e limites alinhados.
- [ ] Observabilidade e alertas ativos.
- [ ] Fork tratado (se aplicável).
- [ ] Versões alinhadas e patches aplicados.
