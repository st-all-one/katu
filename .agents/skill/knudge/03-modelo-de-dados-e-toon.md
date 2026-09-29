# 03 — Modelo de dados e contrato de bytes (TOON)

> Contrato de bytes (D04/D06/D95): mudar isto exige golden — e, se for decisão,
> um `Dxx` novo. `SCHEMA_VERSION = 2`.

## 1. A nota

Uma nota é **frontmatter TOON + corpo Markdown**, serializada em
`notas/<tipo>/<id>.md`. O arquivo é a verdade; índice, grafo e derivados são
projeções.

- **Sem nota parcial** (D05): ou a nota é válida e existe, ou não existe.
  Campos opcionais são **omitidos**, nunca `null`/vazio/`~`.
- **Estrito na forma, tolerante na operação** (D05/D16/D17): chave desconhecida
  é rejeitada no `write` e vira *warning* no `read`; tipo desconhecido **rejeita
  a nota** (por-nota, sem derrubar a leitura inteira).

Formato do arquivo:

```
---
<frontmatter TOON>
---
<corpo Markdown opcional>
```

## 2. Identidade endereçada por conteúdo

| Elemento | Fórmula |
|---|---|
| `normalize(s)` | NFC + trim + colapso de whitespace |
| `body_hash` | `hex8(SHA-256(normalize(statement) + LF + normalize(body)))` |
| `id` | `<prefixo>_<base36(8)>(SHA-256(type + U+001F + normalize(statement)))` |
| hash curto | SHA-256 truncado aos **4 primeiros bytes** (`u32` BE) |

Consequências:

- **Idempotência sob retry** (D01): mesmo conteúdo → mesmo `id`.
- **Prefixo histórico** (D02): reclassificar o `type` **não** reescreve o `id`;
  a linhagem fica explícita via `superseded_by`/`replaces`.
- `body_hash` cobre `statement` + corpo: editar o corpo muda o hash e invalida
  vetores/índice.
- `id`/`body_hash` seguem **NFC**. Fold de diacríticos (D172) e stemming (D206)
  são **só do índice derivado** — `notas/` nunca muda.

## 3. Chaves canônicas (`CANONICAL_KEYS`, 31)

Nesta ordem — **a ordem é contrato** (D04/D13):

```
id · type · statement · created_at · body_hash · schema_version ·
tags · source · superseded_by ·
references · depends_on · contradicts · supports · extends · replaces · rejects · results_in ·
same_as · broader · narrower · related ·
revision · outcomes · classification · anchors · status · scope · checks · evidence ·
claims · provenance
```

- `REQUIRED_KEYS` (5): `id`, `statement`, `created_at`, `body_hash`,
  `schema_version`. `type` é obrigatório para espécies mas **omitido** quando
  `scope=epic` (tipo efetivo derivado — D149).
- As **12 arestas** ficam logo após `superseded_by`; `superseded_by` é o
  ponteiro reverso (id único) de `replaces` (D98).
- `expires_at`/`not_before` **saíram** (D135): expiração deriva da
  `classification`. `confidence` **saiu** (D142): sempre derivada.

## 4. Enums fechados

Tipos abertos degradam o retrieval, então são fechados (evoluir exige bump de
`schema_version`, D14).

| Enum | Valores | Nota |
|---|---|---|
| `NoteType` | `fact decision question task def error snippet link meta risk` (**10**) | `epic` é derivado de `scope=epic`, nunca gravado (D149) |
| `Scope` | `epic issue task` | nível do item de trabalho (D134) |
| `Classification` | `foundational tactical observational` | rege shelf-life (D44) |
| `Status` | `active in_progress blocked closed superseded forgotten` | `VISIBLE` = active/in_progress/blocked/closed (D176) |
| `EdgeKind` | 12 (8 + ontologia) | `is_ontology`/`is_symmetric`/`inverse`/`is_supersession` (D207) |

Chave desconhecida: rejeita no `write`, warning no `read`. Tipo desconhecido:
rejeita a nota.

## 5. Extensões semânticas (D207, aditivas, schema 1→2)

- **Ontologia leve (SKOS-lite):** `same_as` (identidade — simétrica), `broader`
  / `narrower` (hierarquia, inversos), `related` (associação — simétrica).
  Inferência derivada (classes de equivalência, clausura, ciclo) em
  `graph/ontology.rs`.
- **`claims`:** lista de mapas `{subject, relation, object}` (SPO), strings
  trimadas, ≤ 120 escalares, relação de mundo aberto. Habilita contradição
  **precisa** (`claim_conflicts`): mesma `(subject, relation)` com objetos
  divergentes.
- **`provenance`:** mapa PROV-lite `{entity, activity, agent}`.

Notas v1 seguem válidas; o rebuild é byte-idêntico.

## 6. Evidência, âncoras, checks

- **`outcomes[]`** (D48/D103): evidência de execução (`status`/`duration`/`agent`/
  `notes`/`recorded_at`) para **qualquer** nota. A confirmação é **derivada**
  (D87/D189).
- **`anchors`** (D86/D135): único link externo canônico (`path`); o
  `content_hash` é derivado em `.idx/anchors.jsonl`, nunca no frontmatter.
- **`checks`** (D54/D99): conceito de **tarefa** — catálogo em
  `.knudge/validators.toml`.
- **`evidence`** (D55): registro do fechamento de tarefa.

## 7. Data contract por tipo (soft, D191)

`write`/`doctor` conferem **slots mínimos de corpo** por espécie, casados por
cabeçalho/rótulo com fold de diacríticos. Sem chave nova e sem bump de schema:
aviso em `warnings[]`; `invalid_input` só sob `strict`.

| Tipo | Slots esperados |
|---|---|
| `decision` | Alternativas / Por quê / Consequência |
| `error` | Causa / Correção |
| `risk` | Probabilidade / Impacto |
| `def` | Significado |
| `snippet` | Linguagem + âncora |
| `question` | âncora ou `depends_on` |

O `--dry-run` expõe `missing_slots`.

## 8. TOON — gramática do frontmatter (D74/D75)

Subconjunto **documentado** de TOON, *byte-sensitive*, travado por golden e
proptest. Parser/emissor em `knudge-core/src/toon/`.

### 8.1 Princípios

- **Raw UTF-8**, sem BOM. Sem normalização implícita (a normalização é explícita
  em `normalize`).
- **Canônico na emissão, tolerante na leitura** (Postel).
- **Nada de `null`** — opcionais ausentes são omitidos; `null`/`~` são strings.
- **Ordem é contrato**: a emissão sempre segue a ordem canônica.

### 8.2 Gramática (resumo)

```
document      = { entry } ;
entry         = key ":" [ " " value ] newline | key ":" newline block ;
block         = { indented_entry | indented_item } ;
indented_item = indent "- " ( value | pair { continuation } ) newline ;
value         = scalar | flow_list | flow_map ;
scalar        = quoted | bare ;
quoted        = '"' { char | escape } '"' ;
escape        = "\" ( '"' | "\" | "n" | "r" | "t" | "0" | "u{" hex "}" ) ;
indent        = 2 * " " ;   (* múltiplos de 2; tab é erro *)
```

- Comentário: `#` **fora de aspas**, no início da linha ou após espaço. O
  emissor **nunca** emite comentário.
- Linhas em branco são ignoradas; **chave duplicada é erro**.
- **Aninhamento máximo: 32 níveis** (proteção contra input hostil).

### 8.3 Escalares

| Entrada | Leitura |
|---|---|
| `true` / `false` | `Bool` |
| dígitos (`-`/`+` opcional) | `Int(i64)` |
| contém `.`/`e`/`E` e parseia finito | `Float(f64)` |
| `"..."` | `Str` (com escapes) |
| qualquer outro | `Str` cru |

- **Inteiros nunca saem como `.0`** (D09): `1.0` emite `1` e relê `Int(1)`.
- Aspas só quando necessário (espaço nas pontas, `true`/`false`, número,
  `" \ # , [ ] { }` ou controles). `:` é permitido cru em valores.
- Controles viram `\n`, `\r`, `\t`, `\0` ou `\u{XXXX}`.

### 8.4 Corpo e bytes

- `split_frontmatter` separa as partes; sem `---` inicial, o frontmatter é vazio.
- **Lista/mapa vazio é omitido**; documento vazio → **zero bytes** (D11).
- Newline final normalizado para `LF` (D12).

### 8.5 Claims (exemplo)

```toon
claims:
  - subject: embeddings
    relation: porta
    object: "8889"
```

## 9. Hash e versionamento

- Hash curto = SHA-256[0:4] (`u32` BE) — D95.
- `body_hash` = `hex8(SHA-256(normalize(statement) + LF + normalize(body)))` — D06.
- `id` = `<prefixo>_<base36(8)>(SHA-256(type + U+001F + normalize(statement)))`.
- `schema_version` (atual `2`) é lido **on-read com defaults**; **sem aliases**
  (D14). Bump 1→2 adicionou ontologia leve + `claims`/`provenance`.
- Rebuild do índice é disparado quando o **formato do índice** muda, nunca
  quando o frontmatter muda.

## 10. Validação

`Frontmatter::validate()` checa chaves conhecidas, tipos, limites
(`statement ≤ 120` escalares Unicode, D08) e a forma de `claims`/`provenance`.
O `doctor` usa o mesmo núcleo via os checks `integrity`/`body`.

## 11. Escrita e leitura seguras

- **Escrita atômica** (D20): tmp + rename no mesmo diretório; `fsync` em batch
  (D22). A porta `Fs` é a única via e **não segue symlink**.
- **Ordem de commit:** **nota primeiro, evento depois** (D21).
- **Lock advisory** por arquivo-alvo (`O_CREAT|O_EXCL`, stale 30 s, reclaim por
  rename, ordem externo=container/interno=nota, RAII — D23–D25).
- **Rebuild double-buffer** (D27): escreve `.idx.new/` e troca por rename —
  leitor vê o antigo ou o novo, nunca parcial.
