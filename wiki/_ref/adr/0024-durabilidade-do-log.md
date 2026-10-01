# 0024 — Durabilidade do log: `fsync` por evento ou por turno (opt-in)

- **Estado:** aceite
- **Épico:** E18-T05 / P-01
- **Supersedes:** —
- **Relação:** materializa DF7 (durabilidade explícita) e o §42 (ordem de escrita)

## Contexto

O log da sessão é a **fonte da verdade** (ADR 0008): cada evento é anexado com `sync_data()` antes de
a operação ser dada como concluída. Isso dá a garantia mais forte possível — um evento aceite está no
disco — ao preço de **um `fsync` por evento**. O E18 mediu **18,3 ms/turno** só em `log.append`
(≈ 5 eventos/turno), dentro de um overhead fora do provider de 72,3 ms/turno.

O `fsync` por evento não é a única política defensável. A fronteira natural do trabalho do agente é o
**turno**: o que o modelo vê e o que o utilizador espera é "o turno ficou registado". Uma janela de
um turno é o que os sistemas transacionais chamam *group commit*: várias escritas, uma barreira.

Duas coisas precisam de ser ditas por escrito antes do código:

1. **Quem escolhe a política.** Uma janela de durabilidade não se pode tornar *default* sem
   regressão silenciosa: quem já confia no `fsync` por evento não pode passar a perder um turno por
   causa de uma atualização.
2. **O que acontece a um registo rasgado.** Um `write` interrompido por um crash pode deixar a última
   linha incompleta. O leitor de log é **falha-fechado** e hoje recusa abrir a sessão — ou seja, um
   crash no momento errado deixa a sessão irrecuperável. Isso já é verdade com `fsync` por evento (a
   barreira é depois da escrita); a janela maior só a torna mais provável.

## Decisão

**A política é dado, não constante.** `behavior.durability` (config fechada) aceita:

- `event` — **default**, comportamento histórico: `append` sincroniza antes de devolver;
- `turn` — o `append` escreve sem sincronizar e a barreira acontece na **fronteira do turno**
  (`TurnEnd`), no fim do turno e antes de qualquer snapshot.

O contrato, explícito no `katu config list` e no `MODULE.md`:

| Modo | Janela de perda num crash |
|---|---|
| `event` | o evento em voo |
| `turn` | **os eventos do turno em curso** (os turnos anteriores estão duráveis) |

**Recuperação de cauda rasgada.** O leitor descarta a **última** linha quando ela está incompleta
(sem `\n` final ou sem JSON válido) e regista `log.recovered` no diagnóstico; a sequência continua
verificada (`seq` contíguo) e qualquer corrupção a meio do ficheiro continua a ser erro. Sem isto, a
política por turno trocaria `fsync` por sessões irrecuperáveis — um mau negócio.

## Alternatives considered

1. **`fsync` por evento, sem alternativa.** Rejeitada: paga ~15 ms/turno por uma garantia que o
   utilizador não pediu; o custo está medido e a alternativa é reversível (é dado).
2. **Tornar `turn` o default.** Rejeitada: muda a janela de durabilidade de quem já corre o produto
   sem o pedir. Um default que perde dados em silêncio não é uma otimização.
3. **`fsync` em intervalo (ex.: 100 ms).** Rejeitada: não tem fronteira semântica — o utilizador não
   consegue dizer o que está durável, e a janela depende da carga.
4. **Só `sync_file_range`/`fdatasync` assíncrono.** Rejeitada por agora: depende de plataforma e não
   reduz o número de barreiras (que é o custo medido).
5. **Descartar o registo rasgado em vez de o recuperar (estado atual).** Rejeitada: transforma um
   crash numa sessão irrecuperável. A recuperação de cauda é o complemento obrigatório da política.

## Consequências

- O ganho medido (P-01, `bench/e18/durability/`) é de **um `fsync` por turno** em vez de um por
  evento: a barreira deixa de escalar com o número de tool calls.
- `behavior.durability = turn` é **opt-in**; o `event` continua a ser o default e o
  comportamento por omissão não muda.
- A porta `Fs` ganha `append_unsynced`/`sync` (com default conservador: `append_unsynced` cai em
  `append` e `sync` é no-op), pelo que um backend que não saiba adiar continua correto.
- O leitor de log passa a recuperar de uma cauda rasgada; a deteção de corrupção **a meio** e de
  saltos de `seq` mantém-se falha-fechado.
- Uma sessão retomada depois de um crash em modo `turn` pode ter perdido o último turno — o
  `snapshot`/`checkpoint` não o menciona, e o `verify()` continua a valer (o log é a verdade do que
  ficou).
