# B-06 · Escalação de sandbox one-shot — protocolo e resultado

## Pergunta

Quando o agente precisa de escalar o sandbox (ex.: correr um comando fora da workspace), a aprovação
é **one-shot** — não é herdada pela próxima escalação?

## Fórmula

A aprovação de escalação de sandbox é **one-shot** (B-06): concede a capacidade mínima derivada da
regra, exige justificação (`reason` + `granted_by`), e é **revogada depois de usada**. A próxima
escalação exige nova aprovação.

```
approve(rule, capability, reason, granted_by)   → capacidade adicionada ao estado
execute(call)                                    → a capacidade destranca a operação
revoke_approval(capability)                       → capacidade removida do estado
próxima escalação                                 → exige nova aprovação (não herdada)
```

O mecanismo é o par de eventos `ApprovalGranted` / `ApprovalRevoked` (append-only, auditável). A
revogação é **persistente** — sobrevive ao replay do log.

## Cenário (canónico, determinístico)

Dois cenários sintéticos, alimentados pelo `Session` de produção:

| cenário | descrição | resultado |
|---|---|---|
| `one_shot_revoked` | aprovação → uso → revogação | capacidade removida |
| `non_reuse` | segunda escalação sem nova aprovação | exige nova aprovação |

**Total:** 2 cenários, 2 critérios cumpridos.

## Como correr

```sh
KATU_APPROVAL_OUT=$PWD/bench/e18/approval/raw.json \
  cargo test -q -p katu-core --lib -- --ignored ab_approval_by_artifact
```

O invariante (revogação + não-reutilização) é asserção de **CI** (sem `#[ignore]`):
`one_shot_approval_is_revoked_after_use`, `one_shot_approval_does_not_survive_replay`,
`a_second_escalation_requires_a_new_approval`, `approval_revoked_removes_the_capability`.

## Decisão (escrita)

O *default* fica **on** — a aprovação one-shot é o mecanismo de escalação de sandbox. A motivação
de segurança: uma aprovação persistente de escalação de sandbox seria uma capacidade permanente de
correr fora da workspace; a one-shot limita a janela de exposição a uma única operação. O custo é
uma aprovação por escalação (mais uma interação humana), que é o preço da contenção estrita.

## Limites (o que este A/B não diz)

- **Não mede a execução real:** o proxy mede a revogação e a não-reutilização, não a execução do
  comando fora da workspace. A escalação de sandbox real (E17) continua a ser fail-closed.
- **A revogação é por capacidade:** se o humano aprovar a mesma capacidade duas vezes, a segunda
  aprovação re-adiciona-a (a revogação remove-a, mas uma nova aprovação concede-a de novo). A
  não-reutilização é entre aprovações, não uma proibição global.
- **O `reason` é obrigatório:** uma aprovação sem justificação é recusada (fail-closed, E07-T05).
