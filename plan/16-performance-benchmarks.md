# E15 — Performance, benchmarks e honestidade numérica

> **Transversal — fecho do eixo.** O katu quer ser um agente em que se confia; uma propriedade
> necessária de confiança é que ele **não possa fabricar os números** que descrevem o seu próprio
> desempenho (§65).
>
> **Decisões:** DF5. **Depende de:** E05.
> **Gate permanente:** nenhum número sem o artefacto que o produziu.
>
> **Matemática do hot path.** Contexto, transporte, confiança e anomalia vivem em
> [`19-otimizacao-profunda.md`](19-otimizacao-profunda.md) (E18); aqui fica o **método** e a
> honestidade numérica.

---

## Metas (do §6 da brainstorm)

| Métrica | Meta | Como se mede |
|---|---|---|
| Startup | **< 50 ms** | `hyperfine` sobre `katu --version`/`--help`, release, máquina declarada |
| Overhead por turno | mínimo, sem regressão | benchmark `criterion` do caminho `tool_call → policy → resultado` |
| Seleção de regras | microssegundos para 30 regras | `criterion` em `katu-policy` |
| Render por frame | dentro do orçamento | benchmark da TUI (E10-T03) |

Cada meta tem um **artefacto** (saída crua commitada) e a base de evidência tipada (DF5).

> **Playbook do substrato.** O `knudge-core` já documenta os erros a evitar na integração —
> derivar `Index`/`Graph` a cada consulta em vez de uma passada por invalidação, varrer o projeto
> sem `Project::ignored_dirs`, drenar embeddings no caminho quente, ou não reusar o índice
> persistido (`.idx/`). Ver
> [`knudge/wiki/integration/13_performance_e_otimizacao.md`](../knudge/wiki/integration/13_performance_e_otimizacao.md).
> O benchmark do katu mede a **integração** (uma passada, filtros antes do BM25, `rewind` com
> orçamento) e não redescobre essas regras.

---

## Tarefas

### E15-T01 ☐ `criterion` e gate de performance no CI
- **Entregáveis:** benchmarks do hot path; gate obrigatório de PR em Linux com entradas sintéticas
  e orçamentos de tempo/heap/escala (§44).
- **Aceite:** uma regressão > X% falha o job de benchmark; os números do CI e do README vêm do
  mesmo artefacto.

### E15-T02 ☐ Portão de publicação de números
- **Entregáveis:** `Metric` (E09-T05); `xtask gate:bench` com a checklist de 7 itens; insumos
  contados mesmo quando o resultado é rejeitado ("spend occurred"); overhead da própria ferramenta
  contado.
- **Aceite:** build falha se um valor publicado não tiver base; o artefacto commitado é referenciado
  a partir do número.

### E15-T03 ☐ Tabela de recuo fail-safe
- **Entregáveis:** as 10 linhas do §63 (modo desconhecido, rota desconhecida, input malformado,
  saída maior, store cheio, transformação não suportada, classe desconhecida, preço ausente, falta
  de recuperação, processo estranho) — cada uma termina em "encaminhar o original" ou recusa.
- **Aceite:** "uma falha de transformação **não** se torna sucesso sintético"; cada linha tem teste.

### E15-T04 ☐ Determinismo de prefixo (quando houver contexto)
- **Entregáveis:** mapeamento determinístico original→substituto; `Simulate` que **não** autoriza
  transformação real.
- **Aceite:** o mesmo bloco produz os mesmos bytes substitutos em turnos diferentes; simular não
  altera o mundo.

### E15-T05 ☐ Instrumentação do prefixo (opcional, **cortar primeiro**)
> **Corta primeiro:** é o item de menor valor deste épico. Se o orçamento apertar, sai antes de
> qualquer outro — não é necessário para a tese nem para o core.

- **Entregáveis:** capacidade de medir o que o agente **lê** antes de cada chamada (cf.
  `subagent-tax`), com labels de variante, redação na escrita e a recusa de comparar pisos com
  configs reais.
- **Aceite:** nenhuma comparação cross-harness sem erro adicional declarado.

### E15-T06 ☐ Negativos e no-op permanecem
- **Entregáveis:** benchmark com casos positivos, negativos e no-op; a linha em que o katu **não**
  ganha fica visível.
- **Aceite:** remover um caso negativo ou no-op falha o gate.

---

## Definition of Done (permanente)

- [ ] Metas de startup/por-turno/seleção medidas e dentro do orçamento.
- [ ] Todo número no README/ADRs tem artefacto e base tipada.
- [ ] Tabela de recuo coberta por testes.
- [ ] `xtask gate:bench` verde; job `msrv` verde em Rust 1.97.0.

## Anti-checklist (§65)

- Não publicar percentagens sem a execução crua commitada.
- Não deixar uma base de evidência mudar durante uma agregação.
- Não marcar nada como `verified` sem método imposto e nomeado.
- Não adivinhar preços: zero + `unpriced`.
- Não transformar um erro do provider num sucesso sintético.
- Não manter uma otimização que não mediu uma melhoria — reverter.
