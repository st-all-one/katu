# 11 — Protocolo e Servidor

Os pacotes `protocol`, `client` e `server` implementam o modo remoto experimental (protocolo CBOR v8) para sessões Pi.

## 1. Objetivo

Permitir que um cliente (IDE, WebUI, outro processo) controle sessões Pi remotamente, com transporte neutro. O `chord` define a semântica dos payloads; o `pi-protocol` define envelopes e framing.

## 2. Envelopes

- Versão `8`, handshake identifica `serverId`.
- Alvos:
  - Server: `{ serverId }`
  - Session: `{ serverId, sessionId, attachmentId }`
- Requests correlacionados + responses; payloads **strict JSON** opacos.
- Cancelamento, subscriptions (updates opacos) e mudança de attachment out-of-band.
- `attach()`/`detach()` (management) não retornam roteamento; o server publica a rota viva em mensagem `attachment`.
- Erros: códigos opacos não-vazios.

## 3. Framing

- Cada frame: **4 bytes big-endian** de tamanho + **um item CBOR definite-length**.
- `encodeClientMessage`/`encodeServerMessage` validam e codificam frames completos.
- `ClientMessageDecoder`/`ServerMessageDecoder` aceitam fragmentação e coalescência arbitrárias.
- Limites default: 16 MiB por frame/CBOR, 1.000.000 de elementos/entradas, 64 níveis de aninhamento.
- Rejeita propriedades desconhecidas, não-JSON em payload opaco (não-finitos, bytes, undefined, protótipos, ciclos), CBOR malformado e framing inválido → `ProtocolValidationError`.

## 4. Semântica `chord` (payload)

- Chamadas: `{ serviceId, instance?, member, args }`.
- Controle `$chord.service`: `createServiceCatalogueCall`, `createServiceSubscribeCall`, `createServiceUnsubscribeCall`.
- Catálogos, snapshots/updates de subscriptions, códigos de erro, e codecs de delta por subscription.
- Estado replicado: overlays atômicos, drafts, `prepare`/`adopt`, fila pendente limitada (100; overflow vira `reset` com snapshot completo).
- Delta tracking: preserva string append/front-truncate, array splice/permute, set/delete; valida operações não confiáveis.

> Para o MVP, **não portar `chord`**. O protocolo remoto é fase 4+.

## 5. Client/Server

- `client`: transport-neutral, subpath `./unix`; connection, promise, errors, transport, types.
- `server`: listener, connection, session-router, transports (`unix`), testing.
- Server e worker lifecycle é **privado** (fora do protocolo público); o coordinador local é só roteador.

## 6. Plano Rust

### Crates sugeridas
- CBOR: `ciborium` (serde-friendly) ou `minicbor`. `ciborium` é mais ergonômico com `serde`; `minicbor` dá controle de definite-length e performance.
- Codificação de validação de JSON estrito: validar `serde_json::Value` contra tipos "strict JSON" (sem non-finite, sem undefined, profundidade/elementos limitados).
- Transporte unix: `tokio::net::UnixStream`; TCP/TLS depois com `tokio-rustls`.

### Estrutura
```
crates/pi-protocol/src/
├── lib.rs
├── types.rs        # envelopes, targets, version
├── codec.rs        # encode/decode + validação
├── framing.rs      # 4-byte BE length + CBOR item
└── cbor.rs         # limites, strict-json
crates/pi-client/src/{client,connection,transport,unix,promise,errors}.rs
crates/pi-server/src/{server,listener,connection,session_router,transports/unix}.rs
```

### Contratos a reproduzir
- Enums de mensagem com `#[serde(tag = "type")]` e **deny_unknown_fields**.
- Decoders stateful que aceitam chunks parciais (`push(chunk) -> Vec<Message>`, `end()`).
- Preservar ordem de bytes.
- Sem autenticação no protocolo experimental (documentar).

## 7. Casos de uso

1. IDE/extensão controlando uma sessão num processo isolado.
2. Múltiplos clientes (TUI + WebUI) na mesma sessão (attachments).
3. Escalar agentes em workers (com `pi-durable`).

## 8. Ordem de port

1. `pi-protocol` (envelopes + framing + CBOR + testes de fragmentação).
2. Transporte unix local.
3. `pi-client` + `pi-server` mínimos com 1 serviço.
4. Session router + attachments.
5. Estado replicado/`chord` (somente se necessário).
