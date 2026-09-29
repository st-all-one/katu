# 04 — Tipos, números e escolhas de campo

## 1. Tipos escalares

| Tipo | Wire type | Bytes | Faixa / precisão | Default |
|---|---|---|---|---|
| `double` | 1 (I64) | 8 | IEEE-754 double | `0` |
| `float` | 5 (I32) | 4 | IEEE-754 single | `0` |
| `int32` | 0 (varint) | 1–10 | −2³¹ … 2³¹−1 | `0` |
| `int64` | 0 (varint) | 1–10 | −2⁶³ … 2⁶³−1 | `0` |
| `uint32` | 0 (varint) | 1–5 | 0 … 2³²−1 | `0` |
| `uint64` | 0 (varint) | 1–10 | 0 … 2⁶⁴−1 | `0` |
| `sint32` | 0 (zigzag varint) | 1–5 | −2³¹ … 2³¹−1 | `0` |
| `sint64` | 0 (zigzag varint) | 1–10 | −2⁶³ … 2⁶³−1 | `0` |
| `fixed32` | 5 (I32) | 4 | 0 … 2³²−1 | `0` |
| `fixed64` | 1 (I64) | 8 | 0 … 2⁶⁴−1 | `0` |
| `sfixed32` | 5 (I32) | 4 | −2³¹ … 2³¹−1 | `0` |
| `sfixed64` | 1 (I64) | 8 | −2⁶³ … 2⁶³−1 | `0` |
| `bool` | 0 (varint) | 1 | `false`/`true` | `false` |
| `string` | 2 (length) | variável | UTF-8 | `""` |
| `bytes` | 2 (length) | variável | bytes arbitrários | `b""` |

Referências de tipos: mensagem (por nome), enum (por nome), `map<K,V>`,
`group` (legado).

## 2. Escolha de tipo

### 2.1 Inteiros com sinal

- `int32`/`int64`: bom para valores **quase sempre positivos**. Negativos são
  codificados como varint de 10 bytes (sign-extend), então `-1` custa 10 bytes.
- `sint32`/`sint64`: use quando o campo **costuma ser negativo**. Aplicam
  **zigzag** (`0→0, -1→1, 1→2, -2→3`), custando poucos bytes.

```proto
int32 temperature_c = 1;   // pode ser negativo com frequência → prefira sint32
sint32 delta = 2;          // correto para deltas negativos
```

### 2.2 Inteiros sem sinal

- `uint32`/`uint64`: valores nunca negativos; varint sem sinal.
- `fixed*`/`sfixed*`: sempre 4/8 bytes. Prefira quando os valores são grandes e
  quase sempre exigem varint longo (ex.: IDs, hashes, contadores, timestamps
  nanossegundos), pois evitam overhead de varint.

### 2.3 Ponto flutuante

- `float`: 7 dígitos decimais (~ precisão); use em gráficos/3D/telemetria.
- `double`: 15–16 dígitos; default para cálculos gerais.
- **Nunca** use `float`/`double` para dinheiro — use inteiro em unidade mínima
  (`int64 amount_minor`) + moeda. IEEE-754 não representa exatamente decimais.
- `NaN`/`Infinity` são válidos no binário, mas **inválidos em JSON**
  (ver `09-json-e-textformat.md`).

### 2.4 Strings e bytes

- `string`: sequência UTF-8. Com `utf8_validation = VERIFY` (default em
  proto3/editions), o parser rejeita UTF-8 inválido.
- `bytes`: dados opacos (cripto, mídia, protobuf aninhado serializado).
- **Nunca** coloque binário em `string`; quebra validação e JSON.
- Use `bytes` para chaves criptográficas, hashes, tokens opacos.

### 2.5 Enums vs bool vs string

| Necessidade | Preferir |
|---|---|
| Estado com 2 valores estáveis | `bool` pode ser aceitável, mas... |
| Estado extensível (pode crescer) | `enum` com `0 = *_UNSPECIFIED` |
| Conjunto fechado e binário | `bool` |
| Categorias/rótulos | `enum` |
| Texto livre | `string` |
| Dinheiro/tempo | inteiro + escala / `Timestamp` |

Evite `bool` para estados que provavelmente ganharão novos valores; a mudança
para enum é incompatível sem novo campo.

## 3. Números de campo

### 3.1 Faixas e custo

| Faixa | Bytes da tag | Uso |
|---|---|---|
| 1–15 | 1 | Campos mais frequentes |
| 16–2047 | 2 | Demais campos comuns |
| 2048–536870911 | 3–5 | Raros / extensões |
| 19000–19999 | — | **Reservado pelo protobuf** (erro) |

O wire format codifica a tag como `(número << 3) | tipo`. Números pequenos
geram tags menores; por isso os campos mais escritos devem receber `1`–`15`.

### 3.2 Regras

- Números são **imutáveis** após publicação.
- Únicos por mensagem (a faixa de `extensions` reserva o intervalo).
- Máximo `536.870.911` (`2^29 − 1`).
- Sempre `reserved` ao remover.

### 3.3 Exemplo de alocação

```proto
message Metric {
  string name = 1;               // quente
  int64 value = 2;               // quente
  google.protobuf.Timestamp ts = 3;
  repeated string tags = 4;
  map<string, string> labels = 15;
  // reservados no fim
  reserved 16 to 31;
  reserved "old_unit";
}
```

## 4. Repetidos, empacotamento e ordem

- `repeated T x = N;` preserva a ordem de inserção.
- Escalares podem ser `PACKED` (default em proto3/editions via
  `features.repeated_field_encoding = PACKED`) ou `EXPANDED`.
- `string`, `bytes` e mensagens são sempre um valor length-delimited por item.
- Repetidos **não** têm presença: vazio == ausente.
- Para grandes listas de números pequenos, `PACKED` reduz drasticamente o
  tamanho.

```proto
repeated int32 samples = 1 [features.repeated_field_encoding = PACKED];
repeated int32 legacy  = 2 [features.repeated_field_encoding = EXPANDED];
```

## 5. Maps

```proto
map<string, int32> counters = 1;
map<int64, Project> projects = 2;
```

- Chave: integral, `bool` ou `string` (não `float`/`double`/`bytes`/enum/msg).
- Valor: qualquer tipo.
- Sem ordem garantida; duplicatas → última vence.
- No wire: `repeated MapEntry { key=1; value=2; }`.
- Não use map quando precisar de ordenação — use lista + campo de ordenação.

## 6. oneof

```proto
oneof payload {
  string text = 1;
  bytes  blob = 2;
  PayloadV2 v2 = 3;
}
```

- Um campo por vez; `has_`/case disponível.
- Não pode ser `repeated` nem `map`.
- Bom para variantes mutuamente exclusivas e evolução de payload.

## 7. Tipos bem-conhecidos vs tipos próprios

| Use WKT para | Use tipo próprio para |
|---|---|
| `Timestamp` (instante UTC) | — |
| `Duration` (intervalo) | — |
| `Any` (tipo dinâmico) | Variante fechada conhecida (`oneof`) |
| `Struct`/`Value` (JSON dinâmico) | Modelo tipado |
| `Int32Value`/etc (presença de wrapper) | Campo `optional` (prefira) |
| `FieldMask` (updates parciais) | — |

Detalhes em `08-tipos-bem-conhecidos.md`.

## 8. Defaults

- proto2: `[default = X]` permite default customizado.
- proto3/editions: default = zero do tipo; sem custom default em escala.
- O default **não** é a mesma coisa que "ausente" sob presença explícita.
- Enum default = primeiro valor (deve ser `0` em proto3/editions).

## 9. Erros de tipo comuns

| Erro | Consequência |
|---|---|
| `int32` ↔ `sint32` | Valores negativos trocam (incompatível) |
| `int32` ↔ `fixed32` | Wire type diferente (incompatível) |
| `string` ↔ `bytes` | Ambos length-delimited; compatível no wire, mas JSON difere |
| `int32` ↔ `int64`/`uint32`/`uint64`/`bool` | Compatível (varint) |
| `enum` ↔ `int32` | Compatível no wire |
| `float` ↔ `double` | **Incompatível** (I32 vs I64) |
| `repeated` ↔ singular | **Incompatível** |

## 10. Checklist de tipos

- [ ] Dinheiro = inteiro em unidade mínima + moeda (ISO-4217).
- [ ] Tempo = `Timestamp`/`Duration`; nunca `int64` "mágico" sem documentar.
- [ ] Negativos frequentes = `sint*`; IDs grandes = `fixed*`/`uint*`.
- [ ] Binário = `bytes`; texto = `string` (UTF-8).
- [ ] Estados extensíveis = `enum` com `0 = *_UNSPECIFIED`.
- [ ] Campos 1–15 para os mais usados; `reserved` para removidos.
- [ ] `map` só quando a ordem não importa.
