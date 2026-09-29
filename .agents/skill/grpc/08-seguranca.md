# 08 — Segurança

> Fontes: `grpc_docs/doc/server_side_auth.md`, `security_audit.md`,
> `ssl-performance.md`, `SECURITY.md`.

## 1. Modelo

gRPC herda a segurança do HTTP/2/TLS:
- **TLS** dá confidencialidade e integridade em trânsito.
- **mTLS** autentica mutuamente cliente e servidor.
- **ALPN** negocia `h2`.
- Metadados carregam credenciais de chamada (tokens), **não** o payload.

gRPC não cifra o payload por si; para integridade fim-a-fim (ex.: entre
intermediários), assine/hasheie com serialização **determinística**.

## 2. TLS e mTLS

```go
// servidor
creds, _ := credentials.NewServerTLSFromFile("server.crt", "server.key")
srv := grpc.NewServer(grpc.Creds(creds))

// cliente
tlsCreds, _ := credentials.NewClientTLSFromFile("ca.crt", "service.internal")
conn, _ := grpc.NewClient("dns:///service.internal:443",
    grpc.WithTransportCredentials(tlsCreds))
```

Regras:
- Nunca `insecure` em produção (exceto `localhost`/malhas isoladas explícitas).
- Valide o **nome** no certificado (SNI/hostname).
- Rotacione certificados; prefira TLS 1.3.
- mTLS para autenticar serviços internos.

## 3. Credenciais de chamada

| Método | Uso |
|---|---|
| Metadata `authorization: Bearer <token>` | JWT/OAuth2 |
| Credenciais por RPC (`PerRPCCredentials`, call creds) | tokens que acompanham a chamada |
| mTLS | identidade de serviço |
| Cloud IAM / workload identity | tokens de plataforma |

```go
// Go: call credentials
type tokenAuth struct{ token string }
func (t tokenAuth) GetRequestMetadata(ctx context.Context, _ ...string) (map[string]string, error) {
    return map[string]string{"authorization": "Bearer " + t.token}, nil
}
func (tokenAuth) RequireTransportSecurity() bool { return true }

creds := credentials.NewTLS(tlsConfig)
conn, _ := grpc.NewClient(addr,
    grpc.WithTransportCredentials(creds),
    grpc.WithPerRPCCredentials(tokenAuth{token}))
```

- `RequireTransportSecurity()` deve ser `true` para não vazar credenciais.
- Nunca emita credenciais sem TLS.

## 4. Autenticação e autorização

- **Autenticação** (quem): TLS client cert, JWT, mTLS, IAM.
- **Autorização** (pode): políticas por método/recurso; RBAC/ABAC; OPA/Rego,
  OpenFGA etc.
- Valide no **servidor**; nunca confie no cliente.
- Multi-tenancy: tenant em metadata assinado; valide isolamento.
- Compare segredos/tokens em tempo constante.

### Server-side auth (server_side_auth)

Além do transporte, valide credenciais no servidor:
- Extraia e valide o token no interceptor.
- Aplique autorização por método.
- Rejeite sem credencial (`UNAUTHENTICATED`) ou sem permissão
  (`PERMISSION_DENIED`).

## 5. Superfícies e riscos

| Risco | Mitigação |
|---|---|
| Payload acima do limite | `max_receive_message_length`; rejeitar |
| Profundidade de parse | limite de recursão (protobuf) |
| Reflection exposta | desabilitar/restritar em produção |
| Health check detalhado | retornar só status agregado |
| Metadata não validada | validar auth/tenant |
| Logs com PII | redação, sem payload |
| Type confusion (`Any`) | allowlist de `type_url` |
| Compressão (CRIME/BEAST) | desligar em dados sensíveis com entrada atacante |
| Proxy mal configurado | TLS fim-a-fim; validar `:authority` |

## 6. Isolamento

- Rode serviços não confiáveis em sandbox.
- Use limites de CPU/memória por handler.
- Isole tenants (banco, rede, quotas).
- Em ambientes hostis, considere implementações mais seguras (Java/JVM) ou
  isolamento por processo.

## 7. Supply chain

- Pin de versões do runtime gRPC, plugins e libs (OpenSSL/BoringSSL).
- Revise `.proto` e código gerado como código.
- Rode `buf breaking`/`lint` no CI.
- Atualize patches de segurança (gRPC/OpenSSL).
- Verifique origem de bins/plugins.

## 8. Auditoria

- Logue chamadas de auth (sucesso/falha), mudanças de política.
- Correlacione com `trace_id`.
- Não registre segredos.
- Mantenha trilha de quem acessa o quê (sem payload).

## 9. Configurações de TLS (C++/OpenSSL)

- `GRPC_SSL_CIPHER_SUITES`: lista de ciphers.
- `GRPC_DEFAULT_SSL_ROOTS_FILE_PATH`: raízes PEM.
- Prefira TLS 1.2+ com suites AEAD (GCM/ChaCha20).

Ver `../protobuf_guide/11-seguranca.md` para o modelo de ameaças do protobuf
(parsing de entrada hostil).

## 10. Checklist

- [ ] TLS/mTLS em produção; ALPN `h2`.
- [ ] Credenciais em metadata/call creds; `RequireTransportSecurity`.
- [ ] AuthN + AuthZ no servidor.
- [ ] Limites de mensagem/recursos.
- [ ] Reflection/health restritos.
- [ ] Logs redigidos; sem payload.
- [ ] Rotação de certificados e patches.
- [ ] Auditoria de acessos.
- [ ] mTLS entre serviços internos.
