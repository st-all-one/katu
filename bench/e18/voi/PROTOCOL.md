# A3/W8-4 · Gate de Value of Information (VOI) para tool calls — protocolo e resultado

## Pergunta

Há tool calls que não vale a pena fazer — o valor esperado da informação que trariam é **menor que
o custo** — e o gate pode evitá-los **sem nunca saltar o irreconstruível**?

## Fórmula

O valor é a **informação marginal** que a chamada traz ao turno. Para uma chamada de só-leitura
(`read`/`grep`/`find`/`ls`), a informação já está no contexto **se e só se** a sua impressão
(nome + argumentos canónicos) já foi executada **e** não houve mutação desde então:

```
impressão  = FNV-1a(nome ‖ 0x1f ‖ JSON canónico dos argumentos)   (determinística)
VOI(call)  = 0   se a impressão já está satisfeita e não houve mutação
           = >0  caso contrário
custo      > 0   (um pedido ao provider, I/O, tokens)
decisão    = Skip     se VOI = 0 < custo  (só-leitura já satisfeita)
           = Execute  se VOI > 0  ou  a chamada é irreconstruível
```

**Irreconstruível** = toda a chamada que altera o workspace (`write`/`edit`/`move`/`trash`/`bash`/
`plan`/`memory`): **nunca** é saltada. A mutação **invalida** a informação cacheada (conservador:
pode re-executar leituras que não foram afetadas, mas **nunca** serve informação obsoleta).

## Cenário (canónico, determinístico)

Cinco cenários sintéticos, alimentados pelo gate de produção (`agent::turn::voi::Voi`):

| cenário | chamadas | evitadas |
|---|---|---|
| `duplicate_read` | `read a`, `read a` | **1** |
| `distinct_reads` | `read a`, `read b` | 0 |
| `irreconstructible_write` | `write a`, `write a` | 0 (ambas executam) |
| `mutation_invalidates` | `read a`, `write b`, `read a` | 0 (a mutação invalida a 1.ª leitura) |
| `repeated_grep` | `grep fn main`, `grep fn main` | **1** |

**Total:** 11 chamadas, **2 evitadas**, 9 executadas.

## Como correr

```sh
KATU_VOI_OUT=$PWD/bench/e18/voi/raw.json \
  cargo test -q -p katu --bin katu -- --ignored ab_voi_by_artifact
```

O invariante (evitar a duplicada, nunca o irreconstruível) é asserção de **CI** (sem `#[ignore]`):
`the_gate_avoids_duplicates_and_keeps_the_irreconstructible`.

## Decisão (escrita)

O *default* fica **off** (`behavior.tool_voi = false` no arranque) até haver **A/B com um modelo que
emita *tool calls* nativas** — o mesmo precedente de Q-02b/Q-03 (o critério cumpre-se no proxy, o
*default* histórico mantém-se até o A/B). Sem esse A/B, ligar o gate às cegas podia esconder
informação que o modelo precisava; o proxy mede o que se **poupa**, não o que se **perde**.

## Limites (o que este A/B não diz)

- **Não mede sucesso de tarefa:** o modelo local não emite *tool calls* nativas, pelo que não há como
  saber se a informação saltada faria falta. O cenário `mutation_invalidates` mostra a conservadoria
  (re-executa em vez de servir obsoleto), mas um A/B real é que decidiria o *default*.
- **Só poupa só-leitura:** o gate nunca toca no irreconstruível; o ganho é limitado a duplicados
  (`read`/`grep`/`find`/`ls`), que são a classe que o modelo mais repete.
- **O estado é por turno:** a informação cacheada vive no turno; um *resume* recomeça do zero.
- **O custo é positivo mas não quantificado:** o proxy conta chamadas evitadas, não nanos poupados
  (esse número depende do I/O real e do modelo).
