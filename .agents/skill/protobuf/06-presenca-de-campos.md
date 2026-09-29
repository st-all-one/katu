# 06 — Presença de campos

> Baseado em `docs/field_presence.md` e `edition-zero-features.md`.
> Presença é a noção de **se um campo foi definido**, independentemente do seu
> valor.

## 1. Dois modelos

| Disciplina | API | Serialização | Merge |
|---|---|---|---|
| **Sem presença** (`IMPLICIT`) | só valores | default não é serializado | default não é mesclado |
| **Presença explícita** (`EXPLICIT`) | `has_`/clear + valor | default definido é serializado | default definido é mesclado |

O wire format **não carrega informação de ausência**: só representa valores
presentes. A diferença de presença está na API gerada e no comportamento de
merge/serialização.

## 2. Matriz de presença

| Tipo de campo | proto2 | proto3 sem rótulo | proto3 `optional` | editions default |
|---|---|---|---|---|
| Escalar singular | explícita | — | explícita | explícita |
| Enum singular | explícita | — | explícita | explícita |
| `string`/`bytes` | explícita | — | explícita | explícita |
| Mensagem singular | explícita | **explícita** | explícita | explícita |
| `repeated` | — | — | — | — |
| `map` | — | — | — | — |
| `oneof` | explícita | explícita | explícita | explícita |

Notas:
- Mensagem sempre tem presença explícita (mesmo em proto3 implícito).
- `repeated`/`map` nunca têm: vazio == ausente.
- Em editions, `EXPLICIT` é o default; `IMPLICIT` é opt-in.
- `optional` em proto3 gera a mesma API de proto2 para aquele campo.

## 3. Comportamento

### 3.1 Sem presença (`IMPLICIT`)

- Default não é serializado.
- Default não é mesclado.
- "Limpar" = atribuir o default.
- Não há como distinguir "nunca definido" de "definido como default".

```proto
edition = "2023";
message M {
  int32 a = 1 [features.field_presence = IMPLICIT];
}
```

```cpp
M m;
m.set_a(0);       // não serializa; parece "não definido"
```

### 3.2 Presença explícita (`EXPLICIT`)

- Valor definido é sempre serializado (inclusive default).
- Não definido não é mesclado.
- `has_a()`/`clear_a()` disponíveis.
- Definido como default (ex.: `0`) é mesclado normalmente.

```proto
edition = "2023";
message M {
  int32 a = 1;                          // EXPLICIT (default)
  optional int32 b = 2;                 // proto3: explícita
}
```

```cpp
M m;
m.set_a(0);
assert(m.has_a());    // true
m.clear_a();
assert(!m.has_a());
```

### 3.3 `LEGACY_REQUIRED`

Equivale a `required` proto2: wire-required + API-presença. Exige allowlist
(`proto:allow_required`) e **não deve** ser usado em novos contratos. Deserializar
uma mensagem sem o campo é erro.

## 4. Por que presença importa

### 4.1 Patches e updates parciais

Sem presença, um update para o valor default não é representável (o default é
ignorado no merge). Com presença explícita, `set_x(default)` marca o campo como
definido e o merge o aplica.

```text
Cenário: PATCH { "status": 0 }  (0 = UNSPECIFIED)
- Sem presença: não é possível; precisa de FieldMask ou wrapper.
- Com presença: set_status(0) → has_status()=true → mescla.
```

### 4.2 Round-trips

Um valor definido como default pode "desaparecer" após passar por um peer com
presença implícita. Isso quebra contratos que dependem de `has_`.

### 4.3 Diferenciação semântica

- `0` pode significar "desconhecido" ou "zero".
- `""` pode significar "não informado" ou "vazio".
- `false` pode significar "não definido" ou "desligado".

Presença explícita remove a ambiguidade.

## 5. oneof e presença

`oneof` sempre tem presença: o *case* indica qual membro está definido.

```proto
oneof method {
  string card = 1;
  string pix = 2;
}
```

```cpp
if (m.method_case() == M::kCard) { ... }
```

Definir `pix` limpa `card`. "Última ocorrência vence" ao parsear duplicatas de
um mesmo membro; membros diferentes no wire resultam no último definido.

## 6. Merging

Regras de `MergeFrom` (ou equivalente):

- Campos com presença explícita definidos no origem substituem os do destino
  (inclusive default).
- Campos sem presença com valor default são ignorados.
- Mensagens são mescladas recursivamente (campo a campo).
- `repeated`: concatena.
- `map`: mescla por chave (origem vence).
- `oneof`: se definido no origem, substitui o case do destino.

## 7. Como habilitar presença explícita

1. **proto3**: adicione `optional` ao campo.
   ```proto
   syntax = "proto3";
   message M { optional int32 tracked = 1; }
   ```
   Padrão desde protoc 3.15 (antes exigia
   `--experimental_allow_proto3_optional`).

2. **editions**: é o default (`features.field_presence = EXPLICIT`); use
   `IMPLICIT` para desligar.

3. **proto2**: sempre explícita (exceto `repeated`).

Depois, no código, use `has_x()`/`clear_x()` em vez de comparar com o default.

## 8. FieldMask como alternativa

Quando o protocolo precisa distinguir "não atualizar" de "atualizar para
default", `google.protobuf.FieldMask` é a solução idiomática — ele lista os
caminhos a atualizar explicitamente (ver `08`). Isso é independente da presença
do campo.

## 9. Decisão prática

| Situação | Recomendação |
|---|---|
| Campo sempre obrigatório por convenção | presença explícita + validação |
| Campo opcional em update parcial | presença explícita ou `FieldMask` |
| Contador/flag com default semântico 0 | sem presença (mais compacto) |
| Compatibilidade com proto2 legado | manter explícita |
| Wrapper (`google.protobuf.Int32Value`) | substituir por `optional int32` |

## 10. Pegadinhas

- **`optional` em proto3** cria presença, mas não é o mesmo que "nullable":
  a mensagem é sempre um objeto concreto; só há distinção definido/não.
- **`optional` dentro de `oneof`** é ilegal (o oneof já implica presença).
- **`required`** pode causar falha em deserialização de dados antigos —
  nunca use.
- **Comparar `== default`** para inferir ausência é bug quando o default é um
  valor válido.
- **Mudar de implícito para explícito** é compatível no wire, mas muda a API
  gerada e o comportamento de merge — trate como mudança de contrato e version.
- **Enums sem valor zero** (proto2) tornam o default ambíguo; em editions
  abertos, prefira `0 = *_UNSPECIFIED`.
