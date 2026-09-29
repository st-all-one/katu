# 17 — Troubleshooting

## 1. Erros de compilação (`protoc`)

| Mensagem | Causa | Correção |
|---|---|---|
| `Field number 0 is illegal` | Campo com número 0 | Use ≥ 1 |
| `Field number N is reserved` | Nº em 19000–19999 | Escolha outra faixa |
| `Field "x" uses reserved number N` | Nº reservado | `reserved` colidindo; remova |
| `"x" is already defined in "Y"` | Nome duplicado no pacote | Renomeie |
| `"x" is already defined in file "z.proto"` | Import duplicado/mesmo tipo | Ajuste pacote/nome |
| `File not found` | `--proto_path` errado | `-I` correto / import path lógico |
| `Import "x.proto" was not found` | Import fora do path | Adicione a raiz |
| `Import "x.proto" is unused` | Lint estrito | Remova o import |
| `Expected ";".` | Erro de sintaxe | Verifique o ponto e vírgula |
| `Expected top-level statement` | Conteúdo fora de mensagem/serviço | Corrija a estrutura |
| `"optional" is not allowed in proto3 (sem edition)` | `optional` em contexto errado | Use `edition = "2023"` ou remova |
| `Edition "2027" is not supported` | Edição futura | Atualize `protoc` |
| `Feature "x" is not supported` | Feature desconhecida | Atualize toolchain / remova |
| `Option "x" unknown` | Typo em opção | Verifique o descritor/import |
| `Message "X" has no field named "y"` | Nome errado | Corrija |
| `Multiple files with same name` | Caminho lógico inconsistente | Padronize `--proto_path` |
| `java_outer_classname` conflito | Dois arquivos, mesmo outer classname | Defina nomes distintos |
| `Option "go_package" missing` | Go exige | Adicione `option go_package` |

## 2. Erros de runtime

| Sintoma | Causa | Correção |
|---|---|---|
| `Unable to parse` / `DecodeError` | Bytes corrompidos/truncados | Verifique integridade, limites, versão |
| Dados "somem" após round-trip | Presença implícita vs explícita; unknown fields | Use presença explícita; preserve unknown |
| Campo com valor errado tipo | Número reutilizado / tipo trocado | Nunca reutilize número; reserve |
| `has_x()` sempre false | Campo não definido / proto3 sem `optional` | Defina presença explícita |
| `MergeFrom` não aplica default | Sem presença | Use presença explícita |
| `Enum` com valor fora da faixa | Enum aberto | Valide; considere `CLOSED` |
| `Any` unpack falha | `type_url` errado/desconhecido | Valide e registre o tipo |
| `Type not found` (dynamic) | Descritor não carregado | Carregue o `FileDescriptorSet` |
| Gencode/runtime mismatch | Versões diferentes | Alinhe as versões exatas |
| `Arena` use-after-free (C++) | Objeto fora da arena | Mantenha tudo na mesma arena |
| `ASSERT: ... field ...` | Uso de API incorreta | Consulte a doc da versão |

## 3. JSON / TextFormat

| Sintoma | Causa | Correção |
|---|---|---|
| `unknown field` no parse JSON | Campo inexistente | Corrija o nome/descritor; só ignore se seguro |
| int64 perde precisão | JS number | Trate como string |
| `Invalid wire type` | Bytes não são protobuf | Verifique formato/UTF-8 |
| `NaN`/`Infinity` rejeitados | JSON não suporta | Use `double` nativo ou string |
| `bytes` inválido | base64 malformado | Use base64 padrão |
| Enum impresso como número | Opção `print_enums_as_ints` | Ajuste a opção |
| Campos faltando no JSON | Não definidos/default | Use `always_print_*` se necessário |

## 4. gRPC

| Status/erro | Causa | Correção |
|---|---|---|
| `RESOURCE_EXHAUSTED` | Mensagem > limite (4 MiB) | Aumente limite alinhado ou fragmente |
| `UNIMPLEMENTED` | Método/serviço não registrado | Registre o serviço/plug-in |
| `DEADLINE_EXCEEDED` | Timeout | Aumente deadline ou otimize |
| `UNAVAILABLE` + `too_many_pings` | Keepalive agressivo | Alinhe keepalive cliente/servidor |
| `INTERNAL` (parse) | Payload inválido | Valide/limite |
| `UNAUTHENTICATED` | Metadata/credencial ausente | Configure credenciais TLS |
| `PERMISSION_DENIED` | Sem autorização | Verifique políticas |
| `UNKNOWN` | Exceção não tratada no handler | Trate e mapeie status |
| `CANCELLED` inesperado | Deadline/cancelamento do cliente | Propague contexto |
| Reflexão retorna vazio | Reflection não habilitada | Habilite (com restrição) |

## 5. Build systems

| Sistema | Problema | Correção |
|---|---|---|
| Bazel | import não resolve | Verifique `strip_import_prefix`/deps |
| Bazel | código gerado duplicado | `proto_library` única por `.proto` |
| CMake | `protobuf_generate` não gera | Habilite `protobuf_BUILD_...` / `find_package` |
| Buf | lint falha | `buf format -w`, ajuste `buf.yaml` |
| Buf | breaking falha | Versione em novo pacote/reserve |
| Buf | plugin remoto indisponível | Pin de plugin/versão, cache |

## 6. Diagnóstico de wire

```bash
# Ver estrutura sem schema
protoc --decode_raw < payload.bin

# Decodificar com schema
protoc -I. --decode=acme.v1.Order order.proto < payload.bin

# Listar números livres
protoc --print_free_field_numbers order.proto

# Comparar tamanhos
ls -l payload.bin
```

Erros comuns de wire:
- **Bytes começando com 0x00**: possivelmente campo desconhecido de wire type
  inválido.
- **Tamanho inesperado**: campo renumberado ou tipo de wire diferente.
- **Campos extras**: schema antigo vs novo (normal; unknown fields).

## 7. Erros de edições/features

| Sintoma | Causa | Correção |
|---|---|---|
| `edition` rejeitada | Ferramenta antiga | Atualize `protoc`/runtimes |
| `optional` não reconhecido | Edição removeu rótulos | Use `features.field_presence` |
| Feature não surte efeito | `retention = SOURCE` | Não inspecionável em runtime; é esperado |
| Enum se comporta diferente por linguagem | Quirk de `CLOSED` | Defina `features.enum_type` explícito |
| Nomes rejeitados | `STYLE2024/2026` ativo | Renomeie para o padrão |

## 8. Depuração de compatibilidade

1. Confirme que número e tipo de wire não mudaram.
2. Use `buf breaking` para detecção automática.
3. Serialize/parseie entre as versões (dual-read/write).
4. Verifique unknown fields preservados.
5. Teste presença explícita se o campo default importa.
6. Congele golden files por versão.

## 9. Checklist de troubleshooting

- [ ] `protoc --version` e runtime alinhados?
- [ ] `--proto_path` consistente?
- [ ] Números únicos/não reservados?
- [ ] Imports usados e resolvíveis?
- [ ] Presença explícita onde necessário?
- [ ] Limites gRPC (4 MiB) e keepalive alinhados?
- [ ] Payload é realmente protobuf binário?
- [ ] Descritor carregado para tipos dinâmicos?
- [ ] `Any.type_url` correto/allowlist?
- [ ] `buf lint`/`breaking` passando?
