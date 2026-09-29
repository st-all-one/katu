# 15 — Evolução de contrato e versionamento

O maior valor do protobuf é evoluir o schema sem quebrar peers. Este arquivo
consolida as regras de compatibilidade, versionamento e migração.

## 1. Compatibilidade no wire

O parser trata a mensagem como um conjunto de campos identificados por número.
A **tag** = `(número << 3) | wire_type`. Portanto:

- Número é a identidade binária (imutável).
- Campos desconhecidos são preservados (unknown fields).
- A ordem não importa.

## 2. Tabela de mudanças

| Mudança | Wire | JSON/Texto | Veredito |
|---|---|---|---|
| Adicionar campo (novo número) | OK | OK | ✅ compatível |
| Remover campo + `reserved` | OK | OK | ✅ compatível |
| Remover campo sem `reserved` | perigoso | perigoso | ⚠️ evite |
| Renomear campo | OK | ❌ quebra | ⚠️ só se controlar JSON |
| Mudar comentário | OK | OK | ✅ |
| `int32`↔`int64`↔`uint32`↔`uint64`↔`bool` | OK | OK | ✅ (varint) |
| `int32`↔`sint32` | ❌ | ❌ | ⛔ incompatível |
| `int32`↔`fixed32`/`float` | ❌ wire type | ❌ | ⛔ |
| `string`↔`bytes` | OK | ❌ JSON difere | ⚠️ |
| `enum`↔`int32` | OK | ⚠️ | ⚠️ |
| Adicionar valor a enum aberto | OK | OK | ✅ |
| Remover valor de enum | ⚠️ | ⚠️ | ⚠️ (reserve o número) |
| `repeated`→singular ou vice-versa | ❌ | ❌ | ⛔ |
| Adicionar campo a `oneof` | OK | OK | ✅ |
| Mover campo para dentro/fora de `oneof` | ❌ semântica | ❌ | ⛔ |
| Mudar presença implícito↔explícito | OK wire | ⚠️ API/merge | ⚠️ versione |
| Campo `optional`→`required` | ❌ | ❌ | ⛔ |
| Reutilizar número | ❌ corrupção | ❌ | ⛔ |
| Trocar nome do pacote | ❌ tipo muda | ❌ | ⛔ (crie novo pacote) |

## 3. Regras de ouro

1. **Nunca reutilize número.** Ao remover, `reserved N;` e `reserved "nome";`.
2. **Nunca mude o tipo de wire** de um campo existente.
3. **Prefira adicionar** campos novos a mutar os existentes.
4. **Renomeações** só são seguras se você não expõe JSON/TextFormat ou
   controla ambos os lados.
5. **Enum**: preserve o valor `0` e o significado de cada valor; adicione
   novos; nunca renumere.
6. **Presença**: tratar implícito↔explícito como mudança de contrato.
7. **Pacote versionado**: `acme.orders.v1`; uma nova versão incompatível vira
   `v2`, não muta `v1`.

## 4. Versionamento

### 4.1 Estratégia por pacote

```proto
package acme.orders.v1;   // versão no pacote
```

- Compatível: adicione ao `v1`.
- Incompatível/semântica nova: crie `v2`; mantenha `v1` por um período.
- Nunca numere versão no nome do campo (`field_v2`) — use pacote.

### 4.2 Schema como artefato

- Publique `.proto` (não gencode) versionado em registry (BSR/Git).
- SemVer para o **módulo de schema**: breaking = major.
- Se publicar gencode: siga SemVer, suba major ao mudar edição, e pine o
  runtime à versão exata.

### 4.3 Deprecação

```proto
message Order {
  string legacy_id = 7 [deprecated = true];
  reserved 8;
}
```

- Marque `deprecated = true` **antes** de remover.
- Remova só após o período de suporte; então `reserved`.
- Ferramentas (`buf`) detectam uso de itens deprecados.

## 5. Compatibilidade de presença

- No wire, mudar entre `EXPLICIT`/`IMPLICIT` é compatível.
- Na API/merge, **não é**: campos default deixam de ser mesclados (implícito)
  ou passam a ser (explícito). Um round-trip por um peer implícito pode
  "perder" a presença.
- Se a presença é parte do contrato, **não mude**; versione.

## 6. Compatibilidade de enums

- **Aberto**: valores desconhecidos ficam no campo — seguro para adicionar.
- **Fechado**: valores desconhecidos vão para unknown fields — o consumidor
  pode perdê-los ou reordenar.
- Não remova nem renumere valores usados; `reserved` no enum.
- Nunca reutilize um número de valor de enum com semântica diferente.

## 7. Migração de sintaxe para editions

1. Confirme suporte de editions na matriz de runtimes.
2. Rode o migrador oficial (`prototiller`/`protoc`) de proto2/proto3 → 2023.
3. Revise features emitidas; o wire format **não muda**.
4. Escolha a edição-alvo (a mais nova suportada pelo runtime mais antigo).
5. Valide com `buf breaking` + round-trip/interop.
6. Suba a versão major do pacote de schema, se quebrar API gerada.

Dicas:
- `EDITION_LEGACY` não é uma edição; é o bucket de defaults antigos.
- Features de linguagem não devem ser forçadas em schemas compartilhados.
- `protoc` rejeita edições futuras; mantenha toolchain atualizado.

## 8. Detecção automática

```bash
buf breaking --against '.git#branch=main'
```

Categorias: `FILE`, `PACKAGE`, `WIRE`, `WIRE_JSON`. Escolha conforme o risco
(o `WIRE_JSON` é mais rígido). Rode em CI e bloqueie merges.

Regras que o `buf` detecta incluem: remoção de campo/mensagem/serviço/método,
mudança de tipo/número, reutilização de número, mudança de cardinalidade,
alteração de `oneof` e remoção de valor de enum.

## 9. Estratégias de migração de dados

- Leia com o schema **novo**, escreva com o **antigo** durante a transição
  (dual-read): o unknown field preserva dados que o leitor antigo não entende.
- Nunca reescreva dados antigos com schema novo antes de todos os leitores
  atualizarem.
- Para mudanças de tipo incompatíveis, crie um campo novo, dual-write, migre
  leitores, depois remova o antigo (`reserved`).
- Use versões de pacote para mudanças semânticas grandes.

## 10. Checklist de evolução

- [ ] Nenhum número reutilizado; `reserved` em uso.
- [ ] Tipos de wire preservados.
- [ ] Enum `0` e valores existentes mantidos.
- [ ] Presença tratada como contrato.
- [ ] Versão no pacote; mudanças incompatíveis em novo pacote.
- [ ] Itens removidos deprecados antes de removidos.
- [ ] `buf breaking` no CI.
- [ ] Dual-read/write em migrações de dados.
- [ ] Documentação/changelog do contrato.
