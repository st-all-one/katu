# D2 · Deteção de adulteração na auditoria (cadeia de hash) — protocolo e resultado

## Pergunta

O log de auditoria (`.katu/audit/`) pode ser **adulterado** sem que o katu o detete?

## Fórmula

Cada segmento selado tem um **hash do conteúdo** (FNV-1a 64-bit, 16 hex) e um **ponteiro ao hash
do segmento anterior** — uma cadeia de hash (D2). Qualquer adulteração (conteúdo modificado,
segmento removido, ordem trocada) quebra a cadeia:

```
hash(seg)      = FNV-1a(conteúdo do .rec)
prev_hash(seg) = hash(seg anterior)   (vazio no primeiro)
verify()       = para cada segmento: hash(.rec) == hash(seg) E prev_hash(seg) == hash(anterior)
```

A verificação é **determinística** e **persistente** (a cadeia fica no manifesto, que é append-only).

## Cenário (canónico, determinístico)

Dois cenários sintéticos, alimentados pelo `AuditStore` de produção:

| cenário | descrição | resultado |
|---|---|---|
| `intact_chain` | 2 segmentos, cadeia intacta | `verify()` passa |
| `tamper_detected` | conteúdo do 1.º segmento adulterado | `verify()` falha (`AuditError::Tamper`) |

**Total:** 2 cenários, 2 critérios cumpridos.

## Como correr

```sh
KATU_AUDIT_OUT=$PWD/bench/e18/audit/raw.json \
  cargo test -q -p katu-core --lib -- --ignored ab_audit_by_artifact
```

O invariante (cadeia intacta + adulteração detetada) é asserção de **CI** (sem `#[ignore]`):
`verify_detects_tampered_segment`, `verify_detects_removed_segment`.

## Decisão (escrita)

O *default* fica **on** — a cadeia de hash é o mecanismo de deteção de adulteração. A motivação
de segurança: sem a cadeia, um atacante com acesso ao sistema de ficheiros podia modificar o log
de auditoria sem deixar rasto; com a cadeia, qualquer adulteração é detetada na verificação.

## Limites (o que este A/B não diz)

- **FNV-1a não é um MAC:** o hash é determinístico e sem chave — deteta adulteração acidental ou
  intencional, mas não resiste a um atacante que conheça o algoritmo e recompute o hash. Para
  segurança criptográfica, seria necessário um HMAC com chave (D3).
- **A verificação é opt-in:** o `verify()` é chamado explicitamente; não corre automaticamente em
  cada leitura. A integridade é verificada quando o katu arranca ou quando o utilizador pede.
- **O manifesto também precisa de proteção:** a cadeia protege os segmentos, mas o manifesto em
  si podia ser adulterado (removendo segmentos da lista). A deteção de remoção cobre este caso.
