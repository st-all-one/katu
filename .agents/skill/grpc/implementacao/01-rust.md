# 01 — Rust: protobuf + gRPC

## 1. Escolha da biblioteca

| Necessidade | Crate |
|---|---|
| Mensagens (mainstream) | `prost` + `prost-build` |
| gRPC | `tonic` + `tonic-build` |
| Runtime async | `tokio` |
| Observabilidade | `tracing`, `tracing-subscriber`, `opentelemetry` |
| Mensagens (oficial Google) | `google-protobuf` + `google-protobuf-codegen` (v4+, sucessor de `protobuf`) |
| Alternativas | `buffa`, `grpc` (grpc-rust), `connectrpc` |

`prost` + `tonic` é o caminho mais produtivo e maduro para gRPC em Rust.
O crate oficial `google-protobuf` é focado em mensagens (via upb/C++), não traz
gRPC; combiná-lo com `tonic` exige conversões. Para novos serviços gRPC, prefira
`prost`+`tonic`; para máxima paridade com a implementação C++, avalie
`google-protobuf`.

> Versões: fixe `prost`, `prost-build`, `tonic`, `tonic-build` compatíveis entre
> si (ex.: `0.13.x`/`0.14.x`). O `protoc` usado pelo `prost-build` deve ser
> compatível com o schema (editions exigem `protoc` recente).

## 2. `Cargo.toml`

```toml
[package]
name = "catalog"
version = "0.1.0"
edition = "2024"

[dependencies]
tonic = "0.13"
tonic-reflection = "0.13"
prost = "0.13"
prost-types = "0.13"
tokio = { version = "1", features = ["macros", "rt-multi-thread", "signal"] }
tokio-stream = { version = "0.1", features = ["net"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
thiserror = "2"

[build-dependencies]
tonic-build = "0.13"
prost-build = "0.13"
```

## 3. `build.rs` (geração)

```rust
use std::{env, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .file_descriptor_set_path(out_dir.join("catalog_descriptor.bin"))
        .compile_protos(&["proto/acme/catalog/v1/catalog.proto"], &["proto"])?;

    println!("cargo:rerun-if-changed=proto");
    Ok(())
}
```

Isso gera o módulo Rust a partir do `.proto`. Para WKT, habilite
`prost-types` (Timestamp, Duration, etc.).

## 4. Incluindo o código gerado

```rust
// src/lib.rs
pub mod catalog {
    pub mod v1 {
        include!(concat!(env!("OUT_DIR"), "/acme.catalog.v1.rs"));
    }
}
```

Alternativa sem `build.rs`: `tonic_prost_build` no build, ou o CLI `prost-build`.

## 5. Servidor

```rust
use tonic::{transport::Server, Request, Response, Status};

use catalog::v1::catalog_service_server::{CatalogService, CatalogServiceServer};
use catalog::v1::{GetProductRequest, ListProductsRequest, ListProductsResponse,
                  Product, WatchProductsRequest, UploadSummary, Status as PStatus};

pub mod catalog { pub mod v1 { tonic::include_proto!("acme.catalog.v1"); } }

#[derive(Default)]
struct CatalogSvc;

#[tonic::async_trait]
impl CatalogService for CatalogSvc {
    async fn get_product(
        &self,
        req: Request<GetProductRequest>,
    ) -> Result<Response<Product>, Status> {
        let id = req.into_inner().id;
        if id.is_empty() {
            return Err(Status::invalid_argument("id vazio"));
        }
        Ok(Response::new(Product {
            id,
            name: "Café".into(),
            price_minor: 1999,
            currency: "BRL".into(),
            status: PStatus::Active as i32,
        }))
    }

    async fn list_products(
        &self,
        _req: Request<ListProductsRequest>,
    ) -> Result<Response<ListProductsResponse>, Status> {
        Ok(Response::new(ListProductsResponse {
            products: vec![],
            next_page_token: String::new(),
        }))
    }

    type WatchProductsStream =
        tokio_stream::wrappers::ReceiverStream<Result<Product, Status>>;

    async fn watch_products(
        &self,
        _req: Request<WatchProductsRequest>,
    ) -> Result<Response<Self::WatchProductsStream>, Status> {
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        tokio::spawn(async move {
            // produz eventos
            let _ = tx.send(Ok(Product::default())).await;
        });
        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }

    async fn upload_products(
        &self,
        mut stream: tonic::Streaming<Product>,
    ) -> Result<Response<UploadSummary>, Status> {
        let mut accepted = 0;
        while let Some(item) = stream.message().await? {
            if !item.id.is_empty() { accepted += 1; }
        }
        Ok(Response::new(UploadSummary { accepted, rejected: 0 }))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("info,tonic=info")
        .init();

    let addr = "0.0.0.0:50051".parse()?;
    // reflection opcional:
    let reflection = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(
            include_bytes!(concat!(env!("OUT_DIR"), "/catalog_descriptor.bin")),
        )
        .build_v1()?;

    Server::builder()
        .add_service(CatalogServiceServer::new(CatalogSvc))
        .add_service(reflection)
        .serve(addr)
        .await?;
    Ok(())
}
```

## 6. Cliente

```rust
pub mod catalog { pub mod v1 { tonic::include_proto!("acme.catalog.v1"); } }

use catalog::v1::catalog_service_client::CatalogServiceClient;
use catalog::v1::GetProductRequest;
use std::time::Duration;
use tonic::transport::{Channel, Endpoint};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let channel = Endpoint::from_static("http://127.0.0.1:50051")
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(5))   // deadline default
        .connect()
        .await?;

    let mut client = CatalogServiceClient::new(channel);
    let resp = client
        .get_product(GetProductRequest { id: "p-1".into() })
        .await?;
    println!("{:?}", resp.into_inner());
    Ok(())
}
```

### Interceptors (auth, tracing, retry)

```rust
use tonic::{Request, Status};
use tonic::service::Interceptor;

#[derive(Clone)]
struct AuthInterceptor { token: String }

impl Interceptor for AuthInterceptor {
    fn call(&mut self, mut req: Request<()>) -> Result<Request<()>, Status> {
        let v = format!("Bearer {}", self.token)
            .parse().map_err(|_| Status::internal("token inválido"))?;
        req.metadata_mut().insert("authorization", v);
        Ok(req)
    }
}

let channel = Endpoint::from_static("http://...").connect().await?;
let client = CatalogServiceClient::with_interceptor(channel, AuthInterceptor {
    token: std::env::var("SERVICE_TOKEN")?,
});
```

Para tracing distribuído, use `tonic-tracing`/`tracing-opentelemetry` e
`TraceContextPropagator`.

## 7. Integração com Axum (gRPC + HTTP no mesmo servidor)

```rust
use axum::{routing::get, Router};
use tonic::transport::Server;

let grpc = Server::builder()
    .add_service(CatalogServiceServer::new(CatalogSvc))
    .into_service();

let http = Router::new().route("/healthz", get(|| async { "ok" }));
let app = Router::new().route_service("/acme.catalog.v1.CatalogService/*rest", grpc);

let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
axum::serve(listener, app).await?;
```

## 8. Segurança

- **TLS/mTLS** via `tonic::transport::ServerTlsConfig`/`ClientTlsConfig`:

  ```rust
  use tonic::transport::{Identity, ServerTlsConfig, Certificate};
  let identity = Identity::from_pem(cert_pem, key_pem);
  Server::builder()
      .tls_config(ServerTlsConfig::new().identity(identity))?
      .add_service(...)
      .serve(addr).await?;
  ```

- Tokens/credenciais em **metadata**, nunca no payload.
- Limite de mensagem: `Server::builder().max_frame_size(...)` /
  `CatalogServiceServer::new(svc).max_decoding_message_size(4<<20)`.
- Deadlines e cancelamento via `Request`/`tokio::time::timeout`.
- Valide entradas no servidor (ids, faixas, UTF-8).
- Reflection só em ambiente controlado.

## 9. Logs e observabilidade

```rust
#[tracing::instrument(skip(self, req))]
async fn get_product(&self, req: Request<GetProductRequest>) -> ... {
    let span = tracing::Span::current();
    span.record("product.id", req.get_ref().id.as_str());
    // ...
}
```

- Nunca logue o payload bruto; registre IDs e status.
- Métricas: `tonic` + `metrics`/`opentelemetry` (contadores por método/status).
- Redação: defina campos sensíveis e mascare antes de logar.
- Use `tracing-subscriber` com `EnvFilter`.

## 10. Performance

- Reutilize `Channel`/clientes (não crie por chamada).
- Use `tokio` multi-thread (`rt-multi-thread`) para servidores.
- Streaming para volumes grandes; evite mensagens gigantes (>4 MiB).
- `prost` gera structs enxutos; evite clones desnecessários (`into_inner()`,
  `std::mem::take`).
- Habilite `prost` `bytes` para campos `bytes` sem cópia (`bytes::Bytes`).
- `tonic` usa HTTP/2 com flow control; ajuste para throughput.
- Compressão: `tonic::codec::CompressionEncoding::{Gzip, Zstd}` no cliente e
  servidor (devem concordar).
- `LTO`/`codegen-units=1` no perfil release para binário final.

## 11. Testes

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn roundtrip() {
        let p = Product { id: "x".into(), price_minor: 10, ..Default::default() };
        let mut buf = Vec::new();
        prost::Message::encode(&p, &mut buf).unwrap();
        let p2 = Product::decode(&*buf).unwrap();
        assert_eq!(p, p2);
    }

    #[tokio::test]
    async fn service_get() {
        let svc = CatalogSvc;
        let resp = svc.get_product(Request::new(GetProductRequest { id: "p".into() }))
            .await.unwrap();
        assert_eq!(resp.into_inner().id, "p");
    }
}
```

- Teste o serviço chamando os métodos diretamente (rápido).
- Para integração, suba em `TcpListener` efêmero e conecte com `tonic`.
- Adicione fuzzing no decode (`libfuzzer`/`cargo-fuzz`).
- Valide `buf breaking` no CI junto de `cargo test`.

## 12. Pegadinhas

- `protoc` incompatível com `prost`/`tonic` → código gerado não compila.
- `include_proto!` exige `OUT_DIR` correto e `build.rs` no crate certo.
- `Status` vs `io::Error`: converta corretamente; não vaze detalhes internos.
- `enum` no Rust é `i32` no wire; use `Status::try_from(i32)` ao ler.
- Reflection exige registrar o `FileDescriptorSet` (veja `build.rs`).
- `tokio` feature `rt-multi-thread` necessária para servidores concorrentes.
- Versões do crate oficial (`google-protobuf`) mudam API entre majors (v4+);
  leia o changelog. Não misture com `prost` no mesmo tipo.

## 13. Referências

- `protobuf_docs/rust/README.md`, `rust/release_crates/*`.
- `protobuf_docs/docs/upb/*` (implementação C que baseia o crate oficial).
- `grpc_docs/CONCEPTS.md`, `doc/statuscodes.md`.
