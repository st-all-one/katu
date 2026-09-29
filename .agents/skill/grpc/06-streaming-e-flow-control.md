# 06 — Streaming e flow control

## 1. Modos de streaming

| Modo | Direção | Exemplo |
|---|---|---|
| Unary | 1 → 1 | CRUD |
| Server streaming | 1 → N | feed, download, watch |
| Client streaming | N → 1 | upload, lote, agregação |
| Bidi | N ↔ N | chat, telemetria duplex |

Em cada stream, as mensagens são entregues **na ordem** em que foram enviadas
(por direção). As direções de um bidi são independentes.

## 2. Semântica

- Enviar 0 mensagens é válido (stream vazio); o status final ainda é enviado.
- Encerrar o envio = **half-close**; o outro lado pode continuar respondendo.
- Um erro interrompe o stream e fixa o status para ambas as direções.
- Cancelamento (cliente/servidor/deadline) encerra com `CANCELLED`/
  `DEADLINE_EXCEEDED`.
- Mensagens podem ser comprimidas individualmente (ver `10`).
- Limite de tamanho aplica-se **por mensagem**, não ao total do stream.

## 3. Padrões por linguagem

### 3.1 Go

```go
// server streaming
func (s *server) Watch(req *pb.WatchRequest, stream pb.Svc_WatchServer) error {
    for _, ev := range feed {
        if err := stream.Send(ev); err != nil { return err }
    }
    return nil
}

// client streaming
func (s *server) Upload(stream pb.Svc_UploadServer) error {
    for {
        chunk, err := stream.Recv()
        if err == io.EOF { return stream.SendAndClose(&pb.Summary{}) }
        if err != nil { return err }
        // processar
    }
}

// bidi
func (s *server) Chat(stream pb.Svc_ChatServer) error {
    for {
        msg, err := stream.Recv()
        if err == io.EOF { return nil }
        if err != nil { return err }
        if err := stream.Send(reply); err != nil { return err }
    }
}
```

### 3.2 Python

```python
def Watch(self, request, context):
    for ev in feed:
        if context.is_active():
            yield ev
```

### 3.3 Java

```java
public void watch(WatchRequest req, StreamObserver<Event> obs) {
    for (Event e : feed) { obs.onNext(e); }
    obs.onCompleted();
}
```

### 3.4 Rust (tonic)

```rust
type WatchStream = ReceiverStream<Result<Event, Status>>;
async fn watch(&self, _: Request<WatchRequest>) -> Result<Response<Self::WatchStream>, Status> {
    let (tx, rx) = mpsc::channel(16);
    tokio::spawn(async move { let _ = tx.send(Ok(Event::default())).await; });
    Ok(Response::new(ReceiverStream::new(rx)))
}
```

### 3.5 Dart

```dart
Stream<Event> watch(ServiceCall call, WatchRequest request) async* {
  for (final e in feed) { yield e; }
}
```

## 4. Flow control (HTTP/2)

- gRPC usa as **janelas de fluxo** do HTTP/2 (por stream e por conexão).
- Controla a memória de buffers; produtores bloqueiam quando a janela zera
  (**backpressure**).
- Em server streaming, mensagens rápidas sem consumidor lento podem encher o
  buffer; o runtime aplica backpressure.
- Ajuste `initial_window_size`/`connection_window_size` para cenários de alto
  throughput, com cautela (memória).
- Um consumidor lento pode reduzir o throughput do stream, não da conexão
  (outros streams multiplexados seguem).

## 5. Backpressure e buffering

- Não acumule mensagens em memória sem limite; aplique fila limitada.
- Em servidores, produza sob demanda (evite `for over 1M` sem respeito ao
  consumidor).
- Se o cliente for mais lento, o `Send` pode bloquear (Go) ou aplicar
  backpressure (Rx/Streams).
- Escolha tamanho de buffer pensando em latência × memória.

## 6. Half-close e encerramento

```go
// client streaming: cliente fecha o envio e aguarda resposta
stream.CloseAndRecv()
// server streaming: servidor encerra com status
return nil // OK
```

- Após half-close do cliente, o servidor pode enviar a resposta final.
- Em bidi, o fim de uma direção não obriga o fim da outra.
- O **status** é o último evento do stream.

## 7. Erros em streaming

- Um erro no meio do stream encerra ambos os lados com aquele status.
- No servidor, retornar erro após enviar mensagens: o cliente recebe as
  mensagens já enviadas e então o status de erro.
- Valide no início quando possível; evite enviar metade dos dados e falhar.
- `Send`/`Recv` retornam erro de transporte quando a conexão cai
  (`UNAVAILABLE`).

## 8. Quando usar streaming

| Use streaming | Evite streaming |
|---|---|
| Fluxos contínuos/longos | Listas pequenas (use `repeated`) |
| Dados gerados incrementalmente | Operações atômicas |
| Uploads grandes | Payloads minúsculos frequentes |
| Chat/duplex | CRUD simples |

Streaming troca simplicidade por eficiência: só use quando há ganho real.

## 9. Limites e tuning

- `max_receive_message_length`/`max_send_message_length` (por mensagem).
- Em streaming, o **total** do stream não é limitado por esses parâmetros —
  aplique limites de negócio (nº de mensagens/bytes).
- Heartbeats/keepalive para detectar streams mortos (`10`).
- Considere timeouts de inatividade no servidor.

## 10. Checklist

- [ ] Modo de streaming adequado ao caso.
- [ ] Backpressure respeitada; buffers limitados.
- [ ] Erros encerram o stream com status correto.
- [ ] Encerramento (half-close/status) tratado.
- [ ] Limites por mensagem e de negócio definidos.
- [ ] Recursos liberados ao cancelar/terminar.
