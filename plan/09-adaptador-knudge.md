# E08 — Adaptador MCP (FUTURO · fora do escopo atual)

> **Não faz parte do escopo atual.** O knudge é integrado **in-process** em E03 (G4). Este épico
> existe apenas para registrar a segunda implementação da porta `Memory` e o teste de sanidade que
> a mantém possível. **Não construir agora.**
>
> **Decisões:** DF6, G4, G7. **Depende de:** E03 (e E05 passar).
> **Estado:** ⏸️ `deferred` — nenhuma tarefa ativa.
>
> Ver [`00b-objetivos.md`](00b-objetivos.md) §7 e [`04-contrato-da-porta-memory.md`](04-contrato-da-porta-memory.md).

---

## Por que fica fora

- **G4:** o knudge é a memória do agente, in-process — não um sidecar MCP. Construir MCP agora
  seria uma dualidade de backend sem consumidor atual (anti-YAGNI, §45.19).
- **G7:** o katu tem CLI + TUI e só. Um cliente MCP stdio é infraestrutura de transporte sem
  necessidade presente.
- **Lição do arags (§20):** "nova capacidade = replicar o molde inteiro" é como a complexidade se
  multiplica. Um segundo backend de memória pagaria custo sem valor enquanto o primeiro funciona.
- **Lição do maxima (§49.5, §51.11):** manter dois caminhos "como fallback" é dívida que sobrevive
  anos. Se MCP voltar, volta como decisão registada — não como fallback silencioso.

---

## O que a porta já garante (sem construir MCP)

1. **Tipos do katu:** nenhum tipo do knudge na API pública (E03-T01).
2. **Adaptador isolado:** `xtask check-layers` falha se `knudge-core` sair do adaptador.
3. **Conformidade reutilizável:** a suíte de contrato (E03-T05) está pronta para um segundo
   backend.
4. **Teste de sanidade:** `xtask check-memory-swap` prova que trocar de adaptador não toca o
   kernel.

**Teste de sanidade (§13):** "se amanhã voltarmos ao MCP, quantos ficheiros do katu mudam?" Se a
resposta for "só o adaptador da porta", o desenho está certo — e **é isso que se valida hoje, sem
escrever MCP.**

---

## Tarefas (bloqueadas até haver decisão registada)

> Estas tarefas **não** entram em nenhuma fase atual. Ficam como especificação para o caso de uma
> `DF` futura autorizar o adaptador MCP.

### E08-T01 ⏸️ Cliente MCP stdio
- Mapear as 4 tools do knudge (`pre_write`, `pre_edit`, `session_end`, `status`) para a porta;
  framing; timeout; reconexão explícita.
- **Aceite futuro:** cumpre a suíte de conformidade contra um `knudge-mcp` real.

### E08-T02 ⏸️ Paridade de backends
- Correr a suíte de E03-T05 contra in-process e MCP; comparar veredictos e documentar divergências.
- **Aceite futuro:** mesmo cenário → mesmo veredicto (ou diferença explicada e documentada).

### E08-T03 ⏸️ Teste de sanidade com o linkage real
- Compilar com `memory-mcp` e `memory-in-process`; `git diff --stat` demonstra que só o adaptador
  difere.
- **Aceite futuro:** a verificação de substituibilidade mantém-se (DF6).

---

## Pré-requisitos de uma futura retomada

- **Decisão registada** (nova `DF` ou ADR) com `## Alternatives considered` explicando por que o
  in-process deixou de bastar.
- **Consumidor atual** identificado (G4/filtro de `00b` §4): quem precisa de MCP e por quê.
- **Sem dualidade ativa:** se MCP entrar, o in-process continua primário e o MCP é opt-in
  explícito; nada de manter os dois por inércia.

---

## Definition of Done (deste épico)

- [ ] Nada construído — o épico permanece `deferred`.
- [ ] A porta e os testes de sanidade continuam verdes **sem** MCP.
- [ ] Qualquer retomada passa pelo filtro de escopo e por uma decisão registada.
