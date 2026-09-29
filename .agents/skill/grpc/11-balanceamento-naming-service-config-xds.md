# 11 — Load balancing, naming, service config e xDS

> Fontes: `grpc_docs/doc/load-balancing.md`, `naming.md`, `service_config.md`,
> `connectivity-semantics-and-api.md`, `grpc_xds_features.md`,
> `grpc_xds_bootstrap_format.md`, `wait-for-ready.md`, `fail_fast.md`.

## 1. Arquitetura (cliente)

O balanceamento gRPC é **por chamada**, não por conexão:

```
Nome → Resolver → [endereços + service config] → LB policy → subchannels → RPC
```

1. O cliente resolve o **nome** (DNS por padrão).
2. O resolver retorna endereços, um **service config** (política de LB, retry,
   timeout) e atributos.
3. A **LB policy** cria subchannels e observa o estado de cada um.
4. Para cada RPC, escolhe o subchannel (servidor).

## 2. Resolução de nomes

Sintaxe URI: `scheme://authority/path`.

| Scheme | Exemplo | Uso |
|---|---|---|
| `dns` (default) | `dns:///servico:50051` | DNS |
| `unix` | `unix:///tmp/s.sock` | socket Unix |
| `xds` | `xds:///servico` | via xDS |
| custom | plugin | service discovery próprio |

- Sem scheme, assume `dns`.
- Porta default: 443 (TLS) / 80 (inseguro, varia).
- Implementações suportam resolvers plugáveis (Consul, etcd, K8s, etc.).

## 3. Políticas de LB

| Política | Comportamento |
|---|---|
| `pick_first` | escolhe o 1º endereço saudável (default) |
| `round_robin` | alterna entre subchannels prontos |
| `least_request` | envia ao subchannel com menos RPCs ativas |
| `ring_hash` | hash consistente (afinidade/sharding) |
| `grpclb` | balanceador externo (legado) |
| xDS policies | `weighted_target`, `priority`, `cluster_impl`, etc. |

Configuração via service config:

```json
{
  "loadBalancingConfig": [
    { "round_robin": {} }
  ]
}
```

- `pick_first` é o default; para múltiplos backends, configure explicitamente.
- `ring_hash` para cache/sharding por chave.
- Políticas xDS permitem roteamento por header, weighted, failover.

## 4. Service config

Estrutura JSON que configura o canal:

```json
{
  "loadBalancingConfig": [{ "round_robin": {} }],
  "methodConfig": [{
    "name": [{ "service": "acme.orders.v1.OrderService" }],
    "timeout": "2s",
    "waitForReady": true,
    "retryPolicy": {
      "maxAttempts": 5,
      "initialBackoff": "0.1s",
      "maxBackoff": "2s",
      "backoffMultiplier": 1.5,
      "retryableStatusCodes": ["UNAVAILABLE", "RESOURCE_EXHAUSTED"]
    }
  }]
}
```

- Pode vir do resolver (DNS TXT, xDS) ou ser definido pela aplicação.
- `methodConfig` por serviço/método.
- `retryPolicy` com backoff e códigos.
- `hedgingPolicy` para envio especulativo (cuidado).
- `timeout` default por método.

### 4.1 Retry policy

- `maxAttempts`, `initialBackoff`, `maxBackoff`, `backoffMultiplier`,
  `retryableStatusCodes`.
- Só retry em códigos idempotentes.
- Combine com `waitForReady`.
- Respeite `RetryInfo` do servidor.

## 5. Conectividade e comportamento

- `wait_for_ready`: aguarda canal pronto até o deadline.
- `fail_fast`: falha imediata (default em muitas linguagens).
- Estados: `IDLE`, `CONNECTING`, `READY`, `TRANSIENT_FAILURE`, `SHUTDOWN`.
- Backoff configurável (ver `10`).

## 6. xDS

xDS é um conjunto de protocolos de controle (ADS, CDS, EDS, LDS, RDS, SDS...)
para configurar dinamicamente roteamento, LB, TLS e políticas — usado em
service meshes (Envoy, Istio) e no modo **proxyless** (cliente gRPC fala xDS
diretamente com o control plane).

### 6.1 Capacidades típicas

- Descoberta de clusters/endpoints (CDS/EDS).
- Roteamento por header/path (RDS).
- Weighted targets, prioridades, failover.
- mTLS via SDS (certificados dinâmicos).
- Retry/timeout/hedging por rota.
- Circuit breaking, outlier detection.

### 6.2 Bootstrap

O cliente precisa de um arquivo de bootstrap apontando para o control plane:

```json
{
  "xds_servers": [{
    "server_uri": "traffic-director.example.com:443",
    "channel_creds": [{ "type": "google_default" }],
    "server_features": ["xds_v3", "ignore_resource_deletion"]
  }],
  "node": { "id": "client-1", "cluster": "prod" }
}
```

- Variável de ambiente para o bootstrap (depende da implementação).
- `xds:///servico` como alvo.
- Requer control plane compatível (Istio, Traffic Director, etc.).

### 6.3 Proxyless vs sidecar

| Modo | Vantagem | Custo |
|---|---|---|
| Proxyless (xDS no cliente) | menos hops, menor latência | cliente precisa suportar xDS |
| Sidecar (Envoy) | linguagem-agnóstico | hop extra, complexidade |

## 7. Load balancing com DNS

- DNS retorna múltiplos IPs; use `round_robin` ou `least_request`.
- TTL de DNS e reconexão influenciam a distribuição.
- Para sharding, `ring_hash` com chave derivada do request.
- Proxies (L4/L7) podem fazer LB externo; combine com LB do cliente.

## 8. Boas práticas

1. Configure LB explicitamente quando houver múltiplos backends.
2. Defina `methodConfig` com timeout e retry.
3. Use `wait_for_ready` para jobs tolerantes a espera.
4. Prefira `round_robin`/`least_request` a `pick_first` em produção.
5. Em mesh, prefira xDS para políticas centralizadas.
6. Monitore distribuição e falhas por subchannel.

## 9. Checklist

- [ ] Resolver correto (DNS/custom/xDS).
- [ ] Política de LB adequada (não `pick_first` por acidente).
- [ ] `methodConfig` com timeout e retry.
- [ ] Retry apenas em códigos idempotentes, com backoff.
- [ ] `wait_for_ready`/`fail_fast` coerentes.
- [ ] xDS/bootstrap configurados quando em mesh.
- [ ] Observabilidade por backend.
