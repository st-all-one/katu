# 01 · Guardrails: a política como kernel

> Pilar 1 da [tese](00-tese.md). A afirmação concreta: **nada acontece sem decisão registada**, e a
> decisão precede o efeito. Este documento explica o mecanismo, o que ele garante, o que ele não
> garante, e onde estão as arestas.

## 1. Onde a decisão é tomada

```
ToolUse (do modelo)
   ↓  kernel::step — função pura, sem I/O
   ├─ 1. ledger de conteúdo (contenção: o modelo não controla tudo o que entra)
   ├─ 2. policy::evaluate(ToolUse, State) → Effect
   │      Effect = Allowed { ToolOutcome } | Denied { rule_id, evidence } | NeedsApproval
   └─ 3. log.append(State → Event)   ← a decisão entra no log *antes* do efeito
   ↓
   Agent executa o efeito (o Fs já foi canonicalizado)
   ↓
   Event::ToolResult  →  `ToolReport::to_delta()`  →  o modelo vê o delta, o log guarda o delta
```

Três propriedades estruturais, e não convenções:

1. **`step` é puro.** A mesma entrada dá a mesma saída; o kernel não toca em ficheiros, relógio ou
   rede. Logo, um teste de política é um teste de função pura — não precisa de mocks.
2. **A decisão precede o efeito e entra no log.** Se o processo morrer entre os dois, o log mostra
   uma decisão sem resultado (não um resultado sem decisão), e a retomada fecha o turno de forma
   determinística.
3. **O texto do resultado tem uma só função.** `ToolReport::to_delta()` alimenta o evento **e** o
   provider. Não há duas implementações por onde o texto possa divergir — foi a razão pela qual o
   *taint* (ver §4) foi construído em `to_delta()` e não nos quatro adaptadores.

## 2. O vocabulário fechado

A política não avalia linguagem natural; avalia um vocabulário **fechado** de capacidades e de
atributos de conteúdo, versionado ([ADR 0003](../_ref/adr/0003-vocabulario-v2-contencao.md)):

- **8 regras** em `policy/` (5 de memória, 3 de contenção), cada uma com regra de topo que a liga a
  um teste — o `check-rule-coverage` falha se uma regra ficar sem teste.
- **Tiering de capacidade** ([ADR 0012](../_ref/adr/0012-catalogo-dialetos-e-providers-declarativos.md)):
  o que o modelo pode fazer é função do modelo e do tier, não do pedido.
- **Symlink resolvido antes do veredicto**: `Fs::canonicalize` + `katu-tools::resolve`, com teste de
  escape. Uma regra que visse o caminho *declarado* seria contornável por um link.

O resultado de uma recusa é **estruturado**: `rule_id` + evidência. A evidência é o que permite
que a TUI, o log e o modelo vejam a mesma razão — e o que permite o teste invertido (remover o
enforcement tem de dar vermelho).

## 3. Aprovação humana como capacidade mínima

Quando a política devolve `NeedsApproval`, o pedido não é um booleano: é uma **capacidade mínima**
calculada por `katu_policy::capability_for`, assinada pelo humano (`override_reason` + `granted_by`)
e registada como `ApprovalGranted` no log.

Duas propriedades merecem destaque:

- **Não herda.** Uma aprovação concede **uma** operação, com a evidência dessa operação. Não
  é um token que o agente reutilize.
- **É revogável e one-shot** ([`bench/e18/approval/`](../../bench/e18/approval/PROTOCOL.md)): a
  medição `w93.approval.one_shot_revoked = 1` é a prova de que uma aprovação não sobrevive à
  operação que autorizou.

Isto é o oposto do padrão comum (perguntar «vou apagar tudo, ok?» e continuar), e é a
razão pela qual o katu não tem um `y/n` genérico.

## 4. Taint: marcar dados não confiáveis noWire

Um tool call é **entrada não confiável**: o conteúdo de um ficheiro, o output de um comando ou o
corpo de um resultado HTTP pode conter texto que imita instrução. O D1 embrulha o delta nesse
envelope:

```
<katu:untrusted kind="read" bytes="0002041">…conteúdo…</katu:untrusted>
```

Propriedades medidas ([`bench/e18/taint/`](../../bench/e18/taint/PROTOCOL.md)):

- **6 de 6 ataques bloqueados, 0 escapados** — fecho de tag forjado, indentação, tag partida,
  impersonação de role, quebra de cerca.
- **Sem aumento de comprimento**: o escape substitui `<` por `[` apenas antes de um marker `katu:`,
  pelo que o teto total de 8 KiB continua a valer exactamente.
- **Custo**: 71 B por resultado, 0,87 % num delta de 8 KiB.
- **O contrato é ensinado ao modelo** (o prime versão 5 diz que `<katu:untrusted>` é dado, nunca
  instrução) — porque a confiança não pode vir só da estrutura.

Limite honesto: isto **marca**, não **impede**. Um modelo que decida obedecer ao conteúdo do ficheiro
continua a poder fazê-lo. O que o D1 compra é que a fronteira é explícita e detectável, e não uma
promessa implícita de obediência.

## 5. Detecção de ciclos: parar antes do tecto

Um agente que repete o mesmo passo queima orçamento devagar. O guard usa CUSUM (média, fração de
repetição), SPRT (binário, passo inteiramente repetido) e uma **e-value de Ville** — o que dá erro
tipo I ≤ α para *qualquer* n, incluindo paragem opcional
([ADR 0026](../_ref/adr/0026-modo-batch-declarativo-manter-loop-nativo.md) e
[`bench/e18/loop/`](../../bench/e18/loop/PROTOCOL.md)).

Medido: **0 falsos positivos** em 200 turnos normais de 12 passos; alarme no **5.º passo** de um
ciclo puro, contra um tecto de 12 — ou seja, corta com 7 passos de margem. Um passo com chamada
exclusiva conta como progresso e reinicia o detector: o *polling* legítimo de `bash` não é cortado.

A acção é **cortar antes de executar**, com `agent.loop` no catálogo, erro `LoopDetected` e turno
**fechado** — nunca um turno que fica aberto a fingir que terminou.

## 6. Confiança: a política precisa de evidência, não de opinião

Uma regra que nunca foi testada não está "cumprida", está *não medida*. O veredicto tem três
estados — `Enforced`, `unmeasured`, `contradicted` — e cada transição exige evidência:

- O limite inferior é o **Wilson unilateral a 95 %**, não a média: com um log perfeito a regra
  prova-se a **n = 25** (LB `902 ≥ 900`); a `n = 24` fica em `899`.
- Uma violação em 20 derruba o LB de `881` para `804` e marca `contradiction`.
- **C5 (Benjamini–Hochberg)**: a família de regras é testada em conjunto, com p exacto e `q = 5 %`.
  A consequência medida: **40 regras com 25 honras cada** são promovidas pelo limiar bruto e
  **nenhuma** pelo BH. Preço pago, e escrito: para provar `Enforced` com correcção familiar o `n`
  sobe de 25 para 29.

O gate é **fail-closed**: a evidência só pode **remover** promoções, nunca criá-las. E o log real
tem hoje **0 ensaios medidos** — logo o resultado honesto é `unmeasured` para quase tudo, e é isso
que o `xtask policy:confidence` imprime.

## 7. Onde esta pilar não chega

- **Contenção é soft.** O plano em torno do utilizador não é uma fronteira: um binário que o
  utilizador executa tem o mesmo acesso que o katu. O jail real (bwrap/Landlock/seccomp) está em
  [E17](../_ref/plan/18-jail-futuro.md) e é decisão de não fazer agora.
- **Symlinks e resolução**: resolvidos antes do veredicto, mas a janela entre a decisão e o efeito
  é um TOCTOU clássico. Mitigação actual: a escrita atómica e o `ChangedFiles` relativo na
  verificação (`E09-T03`) — o gate de verificação detecta a divergência, não a impede.
- **O modelo continua soberano quanto ao *conteúdo***. Os guardrails restringem acções, não
  intenções; por isso a segunda IA e a memória entram como *contexto* com taint, nunca como
  instrução.

Continua: [02 · memória](02-memoria.md).