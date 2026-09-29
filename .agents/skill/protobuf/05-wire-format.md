# 05 — Wire format (codificação binária)

O wire format do protobuf é um **stream de pares (tag, valor)** auto-delimitados.
Esta é a base de toda compatibilidade: campo é identificado pelo **número** na
tag; o nome não existe no binário.

## 1. Tag (chave)

```
tag = (field_number << 3) | wire_type
```

- `field_number`: 1 … 2²⁹−1.
- `wire_type`: 3 bits (0–5):

| Wire type | Nome | Usado por |
|---|---|---|
| 0 | `VARINT` | `int32/64`, `uint32/64`, `sint32/64`, `bool`, enum |
| 1 | `I64` | `fixed64`, `sfixed64`, `double` |
| 2 | `LEN` (length-delimited) | `string`, `bytes`, mensagens, `repeated` packed, map |
| 3 | `SGROUP` | início de grupo (legado) |
| 4 | `EGROUP` | fim de grupo (legado) |
| 5 | `I32` | `fixed32`, `sfixed32`, `float` |

- Tags em `1`–`15` ocupam 1 byte (bom para campos quentes).
- `wire_type` desconhecido ⇒ erro; `field_number` desconhecido ⇒ **unknown field**
  (preservado e re-serializado).

## 2. Varint

Inteiros sem sinal, base 128, little-endian de grupos de 7 bits; o bit mais
significativo (`MSB`) indica continuação.

```
300 = 0b100101100 → [0xAC, 0x02]
```

- `int32`/`int64` negativos são **sign-extended para 64 bits** e codificados como
  varint de **10 bytes** (por isso `sint*` é melhor para negativos).
- Máximo 10 bytes para 64 bits.
- `bool`: `0`/`1`.

## 3. ZigZag (`sint32`/`sint64`)

Mapeia inteiros com sinal para varints curtos:

```
encoded = (n << 1) ^ (n >> 31)    // n com 32 bits
encoded = (n << 1) ^ (n >> 63)    // n com 64 bits
n = (encoded >>> 1) ^ -(encoded & 1)
```

| Original | ZigZag |
|---|---|
| 0 | 0 |
| −1 | 1 |
| 1 | 2 |
| −2 | 3 |
| 2147483647 | 4294967294 |
| −2147483648 | 4294967295 |

## 4. Tamanho fixo

- `I32` (wire 5): 4 bytes little-endian — `fixed32`, `sfixed32`, `float`.
- `I64` (wire 1): 8 bytes little-endian — `fixed64`, `sfixed64`, `double`.
- A tag e o valor são enviados juntos; sem length prefix.

## 5. Length-delimited

Formato: `tag` + `varint(tamanho)` + `tamanho` bytes.

Usado por:
- `string` (UTF-8), `bytes`.
- mensagens (o payload é um protobuf serializado aninhado).
- `repeated` empacotado (todos os elementos num único LEN).
- `map` (cada entrada é uma mensagem `key/value`).

```
message Outer { Inner inner = 1; }
// 0A <len> <bytes da Inner>
```

## 6. Repetidos empacotados vs expandidos

**Empacotado (`PACKED`):**

```
tag(LEN) len v1 v2 v3 ...
```
Um único par tag/valor para todos os elementos.

**Expandido (`EXPANDED`):**

```
tag v1 tag v2 tag v3 ...
```

- Default: proto3/editions empacotado; proto2 expandido.
- `string`, `bytes` e mensagens são sempre um LEN por elemento.
- Compatibilidade: decodificadores aceitam ambos os formatos para campos
  escalares repetidos; a codificação segue a feature. Ao mudar a feature, o
  conteúdo é equivalente.

## 7. Maps no wire

```proto
map<string, int32> counts = 1;
```

equivale a:

```proto
message CountsEntry { string key = 1; int32 value = 2; }
repeated CountsEntry counts = 1;
```

Cada entrada é um LEN com sua própria mensagem. Duplicatas: última vence.

## 8. Grupos (legado)

`SGROUP`/`EGROUP` delimitam por tags de início/fim em vez de comprimento,
permitindo aninhamento. Em editions, reproduza com
`features.message_encoding = DELIMITED`. Não use em contratos novos.

## 9. Ordem e determinismo

- A ordem dos campos na serialização **não é garantida** — parseie como
  conjunto não ordenado.
- **Serialização determinística**: quando habilitada, campos são emitidos em
  ordem de número e maps em ordem definida. É necessária para hashing/assinatura
  de payloads e testes golden.
  - C++: `SerializeToStringDeterministically` / `SetSerializationDeterministic`.
  - Java: `CodedOutputStream.useDeterministicSerialization()`.
  - Python: `SerializeToString(deterministic=True)`.
- Determinismo **não** é o default e pode não ser portável entre versões/idiomas.

## 10. Unknown fields

- Campos recebidos sem definição no schema são armazenados em *unknown fields*.
- São re-serializados ao reenviar (preservação forward-compatible).
- Comportamento configurável: manter, descartar (`DROP`) ou preservar
  (features/opções de runtime). Para dados intermediários que precisam
  sobreviver, mantenha o default.
- Enums **fechados** guardam valores fora da faixa em unknown fields; enums
  **abertos** os mantêm no campo.

## 11. Tamanho e limites

| Limite | Padrão / observação |
|---|---|
| Tamanho da mensagem | gRPC: 4 MiB (configurável) |
| Profundidade de aninhamento | 100 (runtimes/`protoc`) |
| Nº de campos/tag | 1 … 2²⁹−1 |
| Varint | até 10 bytes por valor de 64 bits |
| Bytes totais (parsing) | limite configurável; **aplique em entradas hostis** |

O wire format não impõe, por si, limite de tamanho — quem chama deve impor
(camada defensiva). Ver `11-seguranca.md`.

## 12. Exemplo de codificação

```proto
edition = "2023";
message Person {
  string name = 1;
  int32 id = 2;
  bool active = 3;
}
```

```
name="Ana", id=42, active=true
0A 03 41 6E 61      // campo 1 (LEN, 3 bytes): "Ana"
10 2A               // campo 2 (VARINT): 42
18 01               // campo 3 (VARINT): 1
```

## 13. Erros e armadilhas

- **Reutilizar número** faz o parser interpretar bytes antigos com semântica
  nova — corrupção silenciosa.
- **Mudar wire type** do mesmo número quebra a decodificação.
- **Assumir ordem** de campos/maps leva a conclusões erradas.
- **Comparar bytes serializados** sem determinismo gera falsos negativos.
- **`int32` negativo** custa 10 bytes; use `sint32` quando aplicável.
- **Profundidade maliciosa** (recursão/mensagens gigantes) é vetor de DoS —
  imponha limites.
- **Unknown fields** em dados de longa duração podem crescer e vazar conteúdo;
  avalie descartá-los em pontos de sanitização.

## 14. Resumo visual

```
+--------+---------------------------+
| tag    | (número << 3) | wire_type  |
+--------+---------------------------+
| VARINT | valor base-128            |
| I64    | 8 bytes LE                |
| I32    | 4 bytes LE                |
| LEN    | varint(len) + bytes       |
| SGROUP | início de grupo           |
| EGROUP | fim de grupo              |
+--------+---------------------------+
```
