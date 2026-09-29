# 16 — Boas práticas

## 1. Organização de arquivos e pacotes

```
proto/
  acme/
    orders/
      v1/
        order.proto
        order_service.proto
        common.proto
```

- Um domínio/versão por pasta: `acme/<domínio>/v<versão>/`.
- Nome do arquivo em `lower_snake_case.proto`, descritivo.
- Um tipo principal por arquivo; evite "arquivos deus".
- Declare a versão no `package`, nunca no nome do arquivo/campo:
  `package acme.orders.v1;`.
- Evite pacotes vazios; sempre declare `package`.

## 2. Estilo de nomes (STYLE2026)

| Entidade | Caso | Exemplo |
|---|---|---|
| Arquivo | `lower_snake_case` | `order_service.proto` |
| Pacote | minúsculas com pontos | `acme.orders.v1` |
| Mensagem | `PascalCase` | `LineItem` |
| Campo | `snake_case` | `created_at` |
| Enum | `PascalCase` | `OrderStatus` |
| Valor de enum | `SHOUTY_SNAKE_CASE` | `ORDER_STATUS_PAID` |
| Serviço | `PascalCase` | `OrderService` |
| Método | `PascalCase` | `GetOrder` |
| Oneof | `snake_case` | `payment_method` |

Regras estritas rejeitam underscores extras, maiúsculas em campos e caixa
incorreta. Ative `features.enforce_naming_style = STYLE2026` em novos protos.

## 3. Mensagens e campos

1. **Prefira `enum` a `bool`** quando o estado pode crescer.
2. **Enum começa em `0 = *_UNSPECIFIED`** (ou `*_UNKNOWN`).
3. **Presença explícita** quando "ausente ≠ default" importa.
4. **Não use `required`**.
5. **Não use `group`**, `weak`/`public` imports, nem extensions de dados.
6. **Campos `1`–`15`** para os mais usados; reserve os removidos.
7. **Documente cada campo** com `//` (vira doc no gencode).
8. **Evite campos "mágicos"** — use tipos dedicados (`Timestamp`, `Duration`).
9. **Listas**: não reutilize o mesmo campo para significados diferentes.
10. **Mensagens de request/response dedicadas** — não use a entidade como
    request.

## 4. Escolha de tipos (recap)

| Necessidade | Tipo |
|---|---|
| Dinheiro | `int64` unidade mínima + moeda (`string` ISO-4217) |
| Tempo | `Timestamp`/`Duration` |
| Negativos frequentes | `sint32`/`sint64` |
| IDs/hashes grandes | `fixed64`/`uint64` |
| Binário | `bytes` |
| JSON dinâmico | `Struct` (ciente das limitações) |
| Variante fechada | `oneof` |
| Presença de escalar | `optional T` |
| Update parcial | `FieldMask` |

## 5. Enums

```proto
enum OrderStatus {
  ORDER_STATUS_UNSPECIFIED = 0;
  ORDER_STATUS_PENDING = 1;
  ORDER_STATUS_PAID = 2;
}
```

- Sempre `0` = desconhecido/sentinela.
- Prefixo do enum nos valores (evita colisão no escopo C++).
- Não renumere; `reserved` ao remover.
- `allow_alias` só quando semanticamente equivalente.

## 6. Serviços (gRPC/REST)

- Requests/responses dedicados com sufixo `Request`/`Response`.
- Paginação: `page_size`, `page_token` (não `offset`).
- Filtros/ordenação explícitos.
- `field_mask` para updates parciais.
- Erros ricos: `google.rpc.Status` + `details`.
- Versão no pacote; não quebre métodos; deprecie e adicione novos.
- Anotações HTTP (`google.api.http`) quando exposto via gateway.

Padrões comuns (inspirados em AIPs): `Get`, `List`, `Create`, `Update`,
`Delete`, `BatchGet`, `Search`; nomes de recursos hierárquicos
(`projects/*/orders/*`).

## 7. Documentação no schema

```proto
// Um pedido de compra.
message Order {
  // Identificador único, estável e imutável.
  string id = 1;

  // Valor em unidade mínima da moeda (ex.: centavos).
  int64 amount_minor = 2;

  // Moeda ISO-4217 (ex.: "BRL").
  string currency = 3;
}
```

Comentários `//` são propagados ao código gerado e a ferramentas de doc.
Evite comentários enganosos; documente invariantes, unidades e faixas.

## 8. Higiene de codegen

- **Publique `.proto`, não gencode** como artefato primário.
- Se publicar gencode: SemVer, pine runtime exato, suba major ao mudar edição.
- Mantenha `protoc`/plugins/runtime na mesma versão.
- Não edite código gerado à mão.
- Marque arquivos gerados (`@generated`) e exclua de revisão excessiva/lint.
- Gere em diretório separado (`gen/`), não misture com o fonte.

## 9. Lint e CI

```yaml
# .github/workflows/proto.yml (exemplo)
- run: buf lint
- run: buf format --diff --exit-code
- run: buf breaking --against "https://github.com/org/repo.git#branch=main"
- run: buf generate
```

Categorias de lint: `MINIMAL`, `BASIC`, `STANDARD` (recomendado). Habilite
também comentários obrigatórios em itens públicos, se a sua equipe adotar.

## 10. Segurança por padrão

- `utf8_validation = VERIFY`.
- Limites de parse configurados.
- `Any.type_url` validado.
- Campos sensíveis com `debug_redact`.
- Entrada não-confiável em binário.
- Reflection/health restritos em produção.

Ver `11-seguranca.md`.

## 11. Performance por padrão

- Campos quentes em `1`–`15`; `sint*` para negativos; `PACKED` para listas.
- Arenas (C++); builders reutilizados (Java).
- Evite mensagens monolíticas; fragmente quando fizer sentido.
- Cache de serialização quando aplicável.
- Streaming para grandes volumes.

Ver `12-performance.md`.

## 12. Compatibilidade por padrão

- `reserved` ao remover/renomear.
- Nunca reutilize número.
- Enum add-only; `0` estável.
- Presença é contrato.
- `buf breaking` no CI.

Ver `15-evolucao-de-contrato.md`.

## 13. Antipadrões

| Antipadrão | Problema |
|---|---|
| `required` | Quebra deserialização futura |
| Reutilizar número | Corrupção silenciosa |
| Renomear campo exposto em JSON | Quebra clientes |
| `bool` para estado extensível | Difícil evoluir |
| `float` para dinheiro | Erro de arredondamento |
| Campo "genérico" reutilizado | Semântica ambígua |
| Mensagem deus | Acoplamento/payload enorme |
| `Any` sem validação | Type confusion |
| Logar payload bruto | Vazamento de PII |
| Gencode versionado desalinhado | Falha de runtime |
| `group`/`extensions` de dados | Legado/complexidade |
| Versão no nome do campo | Não escala |

## 14. Checklist final de revisão de `.proto`

- [ ] `edition`/`syntax` e `package` versionado.
- [ ] Nomes no estilo (`PascalCase`/`snake_case`/`SHOUTY`).
- [ ] Campos `1`–`15` priorizados; `reserved` onde necessário.
- [ ] Enums com `0 = *_UNSPECIFIED`.
- [ ] Presença explícita onde importa.
- [ ] Tipos corretos (dinheiro, tempo, inteiros).
- [ ] Documentação em cada campo.
- [ ] Sem `required`/`group`/extensions de dados.
- [ ] Sensíveis com `debug_redact`.
- [ ] `buf lint` e `buf breaking` passando.
- [ ] Testes de round-trip e interop.
