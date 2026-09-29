# 11 — Segurança

> Baseado em `SECURITY.md` oficial. Protobuf **não** oferece confidencialidade
> nem integridade; é um formato de serialização. A segurança está em como você
> trata o schema, os limites e a entrada.

## 1. Modelo de ameaças (oficial)

O protobuf classifica as superfícies em três níveis:

| Nível | Significado | Exemplos |
|---|---|---|
| **Proativamente endurecido** | Suportado e endurecido contra entrada hostil; CVEs tratados com prioridade; usado sem sandbox | Parsing binário nas implementações padrão (Java, C++, Python com extensão C) |
| **Reativamente endurecido** | Recebe hardening, mas sem quebrar compatibilidade por issues menores | Alguns runtimes/formatos |
| **Fora do modelo de CVE (best-effort)** | Esperado com entrada confiável | JSON/TextFormat em alguns caminhos, reflection, ferramentas |

Ainda assim, aplique **defesa em profundidade**: limites, filtros e sandbox
quando a entrada for hostil.

## 2. Recomendações oficiais

1. **Trate `.proto` como código**: revise, versione e proteja como faria com
   `.java`/`.cc`. Schema é vetor de supply chain.
2. **Prefira o binário** para entradas não-confiáveis; JSON/TextFormat são
   secundários e menos endurecidos.
3. **Aplique limites defensivos** ao redor do parse (tamanho, profundidade,
   bytes totais). gRPC limita **4 MiB** por mensagem por padrão.
4. Use as implementações **padrão** de cada linguagem; as alternativas podem
   ter menos hardening.
   - **Java** é recomendado como melhor postura (memory safety da JVM).
   - **C++** é o mais rápido e extensamente fuzzed, mas exige cuidado com
     memória nativa.
   - Python/PHP: prefira o runtime com extensão C.
5. **Mantenha runtime e gencode na mesma versão**, sempre na última patch.
   Correções de segurança podem exigir atualizar ambos.

## 3. Limites defensivos

Aplique **antes** ou **durante** o parse:

| Limite | Sugestão |
|---|---|
| Tamanho máximo da mensagem | 4 MiB (gRPC default) ou menor |
| Profundidade de recursão | ≤ 100 |
| Bytes totais do stream | conforme o caso |
| Nº de campos/elementos | limitar listas/maps gigantes |
| Tempo de parse | timeout |

Exemplos por linguagem:
- C++: `CodedInputStream::SetTotalBytesLimit`, `SetRecursionLimit`.
- Java: `CodedInputStream` com limites; `setSizeLimit`.
- gRPC: `max_receive_message_length`/`max_send_message_length`.
- Go: `proto.UnmarshalOptions{RecursionLimit: ...}`.

## 4. Superfícies de risco

### 4.1 `Any` e type confusion

`Any` carrega `type_url` controlado por quem envia. Nunca faça unpack sem
validar contra allowlist de tipos esperados. Prefira `oneof` quando possível.

### 4.2 UTF-8 inválido

`string` com `utf8_validation = NONE` permite bytes inválidos, que podem
quebrar consumidores (ex.: banco, JSON, log). Mantenha `VERIFY`.

### 4.3 Enums abertos/fechados

Valores de enum fora da faixa podem vazar para a aplicação (aberto) ou ir para
unknown fields (fechado). Valide a faixa no domínio.

### 4.4 Unknown fields

Preservados por padrão. Em dados de longa vida ou ao sanitizar, considere
descartá-los para não carregar conteúdo inesperado.

### 4.5 JSON/TextFormat

- Profundidade/tamanho abusivos → DoS.
- Chaves desconhecidas → valide.
- `__proto__`/prototype pollution em JS.
- `NaN`/`Infinity` e int64 com perda de precisão.
- TextFormat **nunca** para entrada não-confiável.

### 4.6 Deserialização de dados persistidos

Dados antigos podem conter campos removidos/renumerados. Use descritor
versionado, `reserved` e nunca reutilize números.

## 5. Supply chain

- Publique e consuma `.proto` de fontes confiáveis (BSR/registry versionado).
- Emitir/assinar/imutar hashes de schemas.
- Revise diffs de `.proto` em code review (mudanças de número/tipo/presença).
- Pin de versões de `protoc`, plugins e runtime.
- Evite publicar código gerado (risco de mismatch gencode/runtime).
- Rode `buf breaking` e `buf lint` no CI.
- Cuidado com descriptores gerados que embutem source info/comentários.

## 6. Criptografia e transporte

Protobuf não cifra nem autentica. Para isso:

- **TLS** para confidencialidade/integridade em trânsito.
- **mTLS** para autenticação mútua de serviços.
- **Assinatura/hash** do payload quando integridade fim-a-fim for necessária —
  use serialização **determinística** para hashing estável.
- **Rede**: nunca exponha gRPC sem TLS; restrinja reflection e health check.
- **Em repouso**: cifre o armazenamento (o protobuf em si é opaco, mas não
  cifrado).

## 7. Autenticação e autorização

- Credenciais via metadata/headers (JWT, OAuth2), **nunca** nos campos da
  mensagem.
- Autorização no servidor por método/recurso; não confie no cliente.
- Multi-tenancy: propague tenant via metadata assinado; valide isolamento.
- Rate limiting e quotas por identificador.

## 8. Redação de dados sensíveis

- Marque campos sensíveis com a opção de redação:

  ```proto
  string email = 1 [debug_redact = true];
  ```

- Runtimes suportam `TextFormat`/debug com redação (`redact`).
- Nunca logue payload bruto; se precisar, aplique redação e limite.
- Cuidado com `Any`/`Struct`: podem conter PII arbitrária — evite logá-los.

## 9. Fuzzing e hardening

- O projeto usa fuzzing extensivo (OSS-Fuzz) nos parsers.
- Adote fuzzing nos seus próprios parsers/validações e em `Any`.
- Testes de entrada malformada (truncada, profundidade excessiva, varints
  longos, bytes UTF-8 inválidos).
- Prefira libs de parse das linguagens oficiais.

## 10. Checklist de segurança

- [ ] Entradas não-confiáveis sempre em binário.
- [ ] Limites de tamanho/profundidade/tempo aplicados.
- [ ] `utf8_validation = VERIFY`.
- [ ] `Any.type_url` validado contra allowlist.
- [ ] TLS/mTLS; credenciais em metadata.
- [ ] Runtime/gencode na mesma versão e atualizados.
- [ ] JSON com campos desconhecidos rejeitados por padrão.
- [ ] Campos sensíveis com `debug_redact`; logs sem payload bruto.
- [ ] `.proto` revisado e versionado; `buf breaking` no CI.
- [ ] Reflection e health check restritos em produção.
- [ ] Fuzzing/testes de entradas malformadas.
