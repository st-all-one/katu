# ADR 0010 — Memória de primeira classe e substituível (porta `Memory` + adaptador knudge)

- **Estado:** aceite
- **Data:** 2026-09-30
- **Decisões fundacionais:** DF6 (porta com tipos do katu), G4 (memória invariante), DF1, DF5
- **Épicos:** E03 (porta + adaptador), E10 (wiring do kernel), E14 (gate)
- **Relaciona:** [plan/04](../plan/04-contrato-da-porta-memory.md),
  [ADR 0008](0008-sessoes-identidade-e-retomada.md)

## Contexto

O knudge é o motor de memória do agente (G4), mas o **firewall LLM-free** (E01) proíbe que
`katu-core`/`katu-policy`/`katu-tools` dependam dele. Se a porta `Memory` fosse definida em
termos de `knudge_core::schema::Note`, trocar de backend deixaria de ser possível (DF6). Faltava
provar, de forma verificável, que "amanhã voltar ao MCP" muda **só** o adaptador.

Havia ainda duas dívidas funcionais no adaptador: o `pre_edit` respondia sempre `Update` (sem
decisão real de *supersede* em *dry-run*) e o `session_end` era um *hint* (OA8), além de a
memória não ser, ainda, uma **invariante** de produção (E03-T07).

## Decisão

1. **A porta é do katu.** `katu_core::memory` define `Memory: Send + Sync` com tipos próprios
   (`PreWriteReq/Outcome`, `PreEditReq/Outcome`, `SessionEndReq/Outcome`, `MemoryStatus`,
   `NoteRef`, `Anchor`, `Score` em pontos base). Nenhum tipo do knudge aparece na API.
2. **Adaptador in-process no binário.** `crates/katu/src/memory/` é o **único** sítio com o
   vocabulário do knudge, atrás da feature `memory-in-process` (**default**). A fachada `Knudge`
   (que é `!Sync`) vive sob um `Mutex`; o índice/grafo (caros) ficam em cache e são invalidados
   em cada `record`. `behavior.strict` é fixado na abertura.
3. **`pre_edit` é *dry-run* fiel ao `update`.** Muda a chave de conteúdo (`type` + `statement`) →
   `Supersede { target }` (novo `id` derivado por `note_id`); caso contrário → `Update`. Ids
   históricos (não-deriváveis) revisam no lugar quando a afirmação não muda (D01/D48).
4. **`session_end` é *hint*, não *commit*** (OA8): devolve `committed: []` e promove a aviso o
   fecho de `task`, que exige evidência por outro caminho. O commit é sempre explícito (`record`).
5. **Memória é invariante.** O build **default** inclui sempre o adaptador; `katu memory` falha
   fechado (`ErrorKind::Unavailable`, exit 10) quando ele não está compilado. O modo sem adaptador
   existe **só** para o gate de substituibilidade e nele a memória recusa.
6. **Gate `xtask check-memory-swap`** (E03-T06): estático e determinístico — o vocabulário do
   knudge só pode aparecer sob `katu/src/memory/`, a dependência é opcional atrás da feature e
   nenhum crate de núcleo a declara; `make memory-swap` acrescenta
   `cargo check -p katu --no-default-features`.

## Alternatives considered

1. **Linkar o knudge no `katu-core`.** Rejeitada: viola o firewall LLM-free e acopla o kernel a um
   backend concreto (DF6); E03-T01/T06 existem precisamente para o impedir.
2. **MCP (processo à parte) como primário.** Adiada (E08): melhor isolamento, mas exige protocolo,
   serialização e gestão de ciclo de vida; o in-process é o primário e o MCP fica deferido.
3. **Expor tipos do knudge na porta.** Rejeitada: tornaria a troca impossível e arrastaria o schema
   do knudge para dentro do katu.
4. **Só `FakeMemory` (sem memória real).** Rejeitada: o agente precisa de memória real; o fake
   serve os testes de kernel e a paridade de contrato.
5. **Adaptador sem `strict` fixo.** Rejeitada: validações *soft* (âncora/slots/claims) passariam
   silenciosas; o adaptador promove os avisos a erro.

## Consequências

- **Positivas:** memória real e de primeira classe sem quebrar o firewall; `pre_edit` decide
  *supersede* corretamente em *dry-run*; existe um gate que prova que trocar de backend toca
  apenas um diretório; a suíte de conformidade (`assert_contract`) corre contra o `FakeMemory` e
  contra o knudge, provando paridade.
- **Negativas / dívida:** o loop de **turnos** com provider ainda não corre (E12) — o `Runtime` já
  monta a sessão com o adaptador e expõe `recall`/`remember` pelo §42; `spawn_blocking` + timeout
  (E03-T04) só fazem sentido quando existir o caminho async; o gate é estático + `cargo check`
  (não instrumenta a troca em runtime).
- **Travas:** `check-memory-swap` no `make check`; `cargo check -p katu --no-default-features`;
  `assert_contract` no adaptador; `#![forbid(unsafe_code)]` e clippy `-D warnings`.
