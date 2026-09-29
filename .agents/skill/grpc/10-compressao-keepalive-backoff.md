# 10 — Compressão, keepalive e connection backoff

> Fontes: `grpc_docs/doc/compression.md`, `compression_cookbook.md`,
> `keepalive.md`, `connection-backoff.md`.

## 1. Compressão

gRPC comprime **por mensagem**, não por conexão. O `grpc-encoding` indica o
codec da mensagem; `grpc-accept-encoding` lista o que o peer aceita.

### 1.1 Codecs

| Codec | Notas |
|---|---|
| `identity` | sem compressão |
| `gzip` | padrão amplamente suportado |
| `deflate` | suportado |
| `snappy` | suportado em algumas implementações |
| custom | via codec plugável |

### 1.2 Regras

- Compressão é **assimétrica**: request e response podem usar codecs diferentes
  (ou nenhum).
- Pode ser definida por canal (default) e por RPC/mensagem.
- Habilitar no cliente **e** no servidor; o servidor decide comprimir a
  resposta.
- Não comprima dados já comprimidos (imagens, vídeo, zip).
- Compressão custa CPU: meça o trade-off banda × CPU.
- **Segurança:** compressão + entrada controlada por atacante pode habilitar
  CRIME/BEAST. Em dados sensíveis, considere desabilitar (especialmente com
  segredos no payload).
- O bit de compressão no frame indica quando a mensagem está comprimida.

### 1.3 Exemplos

```go
import _ "google.golang.org/grpc/encoding/gzip"

// cliente
resp, err := client.Get(ctx, req, grpc.UseCompressor(gzip.Name))

// servidor: aceita automaticamente; comprime resposta se solicitado
```

```java
// servidor
server = NettyServerBuilder.forPort(port)
    .compressorRegistry(CompressorRegistry.getDefaultInstance())
    .decompressorRegistry(DecompressorRegistry.getDefaultInstance())
    .build();
```

### 1.4 Cookbook (resumo)

- **Payload grande e repetitivo** → compressão ajuda.
- **Payload binário já comprimido** → não comprima.
- **Streaming de muitas mensagens pequenas** → compressão por mensagem pode ter
  overhead; avalie.
- **Baixa latência** → evite compressão desnecessária.
- **Ataque de compressão** → desligue ou limite quando a entrada é hostil.

## 2. Keepalive

Pings HTTP/2 detectam conexões mortas. Parâmetros de canal (gRPC Core):

| Argumento | Efeito |
|---|---|
| `GRPC_ARG_KEEPALIVE_TIME_MS` | período entre pings |
| `GRPC_ARG_KEEPALIVE_TIMEOUT_MS` | tempo de espera pelo ACK antes de fechar |
| `GRPC_ARG_KEEPALIVE_PERMIT_WITHOUT_CALLS` | enviar pings mesmo sem chamadas |
| `GRPC_ARG_HTTP2_MAX_PINGS_WITHOUT_DATA` | limite de pings sem dados |
| `GRPC_ARG_HTTP2_MIN_RECV_PING_INTERVAL_WITHOUT_DATA_MS` | **servidor**: intervalo mínimo aceito entre pings |
| `GRPC_ARG_HTTP2_MAX_PING_STRIKES` | **servidor**: strikes antes de GOAWAY |

Regras:
- **Alinhe** cliente e servidor. Cliente agressivo demais gera
  `GOAWAY` com `too_many_pings`.
- Defaults conservadores; ajuste apenas se necessário (redes com NAT/proxy que
  derrubam conexões ociosas).
- Servidores devem tolerar pings de clientes legítimos.

Exemplo (Go):

```go
kp := keepalive.ClientParameters{
    Time:                30 * time.Second,
    Timeout:             10 * time.Second,
    PermitWithoutStream: false,
}
conn, _ := grpc.NewClient(addr, grpc.WithKeepaliveParams(kp))
```

## 3. Connection backoff

Ao falhar a conexão, o cliente tenta reconectar com **backoff exponencial**.

Parâmetros (gRPC):

| Parâmetro | Padrão (aprox.) | Descrição |
|---|---|---|
| `INITIAL_BACKOFF` | 1 s | espera inicial |
| `MULTIPLIER` | 1.6 | fator de crescimento |
| `JITTER` | 0.2 | variação aleatória |
| `MAX_BACKOFF` | 120 s | teto |
| `MIN_CONNECT_TIMEOUT` | 20 s | timeout mínimo de conexão |

- Backoff evita tempestade de reconexões.
- Jitter distribui tentativas.
- Idle timeout (default 5 min) coloca canais `READY` em `IDLE` sem atividade.

## 4. Conectividade

Estados do canal:

```
IDLE → CONNECTING → READY
                  ↘ TRANSIENT_FAILURE → CONNECTING ...
qualquer → SHUTDOWN
```

- `IDLE`: sem RPCs; a primeira chamada reativa.
- `READY`: conexão pronta.
- `TRANSIENT_FAILURE`: falha transiente; vai reconectar.
- `SHUTDOWN`: encerrado; novas RPCs falham.
- Após `GOAWAY`, o canal re-resolve e reconecta.

Ver `11-balanceamento-naming-service-config-xds.md`.

## 5. Tuning conjunto

| Objetivo | Ação |
|---|---|
| Reduzir banda | compressão gzip |
| Reduzir CPU | identity (sem compressão) |
| Detectar conexão morta rápido | keepalive menor (alinhado) |
| Evitar tempestade de reconexão | backoff+jitter |
| Alto throughput | janelas HTTP/2 maiores |
| Baixa latência | streams multiplexados, conexões quentes |

## 6. Checklist

- [ ] Compressão habilitada no cliente e servidor quando justificar.
- [ ] Não comprimir dados já comprimidos.
- [ ] Avaliar CRIME/BEAST em dados sensíveis.
- [ ] Keepalive alinhado cliente/servidor.
- [ ] Backoff com jitter/teto.
- [ ] Monitorar `GOAWAY`/`too_many_pings`.
- [ ] Idle timeout adequado ao ambiente (NAT/proxy).
