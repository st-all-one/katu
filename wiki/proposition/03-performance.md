# 03 · Performance: o programa de optimização

> Pilar 3 da [tese](00-tese.md). A afirmação concreta: **nenhuma melhoria entra sem fórmula,
> artefacto, teste e decisão escrita** — e o que se ganhou foi medido, não recordamos.

## 1. O método antes do resultado

O programa segue uma regra que o projecto impôs a si próprio (DF5, ver
[05 · método](05-metodo.md)): um número só publica se tiver **base tipada** e **artefacto que o
produziu**, e `unpriced` vale necessariamente zero. Consequência prática: existem ~85 números no
ledger e **nenhum** deles é uma opinião.

Duas regras de higiene explicam quase todos os números abaixo:

- **A/B antes da adopção.** Cada item tem uma pergunta, um método e um critério; o resultado é
  escrito mesmo quando é negativo.
- **Item rejeitado deixa o número, não deixa código.** O conformal (C2) foi implementado, medido,
  rejeitado — e o código saiu de `src/`. A fórmula vive no bench que a mediu.

## 2. O retrato inicial (o que havia antes de mexer)

O baseline E18 ([`bench/e18/`](../../bench/e18/PROTOCOL.md)) mediu um turno e2e de **360 ms**, com:

- **72,3 ms (201 ‰) do turno** do projecto — `log.append` 18,3 ms, escritas atómicas 25,8 ms,
  `session.open` 8,5 ms;
- prompt de **3265 tokens**, com cache de prefixo a 99,7 % no 2.º turno;
- provider a dominating: TTFT local 68 ms, remoto 2 304 ms.

A leitura honesta dessa fase: **o projecto nunca foi o gargalo do relógio**. Era o gargalo do
*custo por chamada* (5 fsync por evento) e do *tamanho do prompt*.

## 3. Ganhos por eixo

### 3.1 Bytes para o modelo

| item | antes → depois | ganho |
|---|---|---|
| prompt total do turno | 3265 → **2057** tokens | **−37 %** |
| prompt de sistema (Q-20) | 2439 → 1014 tokens | −51 % |
| catálogo de skills (Q-05) | 4636 → 1211 B | −74 % |
| `AGENTS.md` (Q-13/Q-19) | 1590 → 1175 B | −26 % |
| ledger (W-9.2) | — | model-visible = 31,8 % do input |
| TOON colunar v3 (P-02) | — | −33 % (release) |
| payload de `edit` (Q-07) | — | −80 % |

O mecanismo não é "encurtar o texto": é **não repetir**. O ledger devolve o excedente por ponteiro
em vez de o truncar; o catálogo de skills é denso e cacheado por *fingerprint*; o TOON é colunar
sem headers no *stream*.

### 3.2 Latência

| item | antes → depois | ganho |
|---|---|---|
| lote paralelo de tools (B-01) | 472 → 112 ms (8 leituras) | **+76,3 %** (reproduzido: 76,0 %) |
| retomar sessão (Q-15) | 23 839 → 321 µs | **−98,65 %** |
| prewarm de prefixo (Q-14) | 32 383 → 86 ms | 377× |
| SSE (P-04) | 37 924 → 27 099 ns | −28,5 % |
| append de log (P-01) | 18,3 ms → **~20–40 µs** por evento | o fsync-por-evento desapareceu |
| fracção não-provider do turno | 201 ‰ → **57 ‰** | o projecto deixou de ser o caminho crítico |

O último número é o que importa mais: o `log.append` deixou de ser um fsync e o Projecto passou a
ocupar 5,7 % do turno. O resto é provider.

### 3.3 Memória e disco

| item | resultado |
|---|---|
| índice de auditoria (100k eventos) | −79 % (4,85 MB vs 23,9 MB) |
| retomar 20 000 turnos | 321 µs (bounded) |
| render por quadro | p95 ≈ 0,7 ms; **1137 alocações / ~154 kB** por quadro (medido, ver [04](04-falsabilidade.md)) |

## 4. Rejeições com número

O programa publicou mais rejeições do que adopções — e essa é a parte que sustenta o resto:

| item | pergunta | resultado | decisão |
|---|---|---|---|
| **DPP** (submodular) | a seleção de contexto é submodular? | sim (0,877), mas MMR já cumpre a diversidade; as 81 trocas restantes são de **utilidade**, não de redundância | rejeitado |
| **C2 conformal** | o intervalo tem cobertura? | 5 de 8 linhas; a cobertura cai 37 ‰ abaixo do nominal com resíduos correlacionados | rejeitado, código removido |
| **E18-T05 restante** | partilhar estrutura no log? | a retomada já é 321 µs; a partilha não tem ganho demonstrável | rejeitado |
| **E18-T08 / T09** | PERT/CPM e PPR | o plano tem **0 arestas** e o rank de memória é **plano** | rejeitado |
| **E1 (USL)** | o teto de paralelismo está certo? | a máquina não escala (2 threads não dão 1,5×) | medido, **sem mudança** |

Cada uma está escrita no `OPTIMIZATION_PLAN` com o número e a condição de revisão.

## 5. O número que falta, e porquê está declarado

O ledger publica `e18.turn.*` desde o **baseline**, e não havia uma leitura de "hoje" — o que é uma
falha de higiene, não de performance. Corrigido em [`bench/e18/pos/`](../../bench/e18/pos/PROTOCOL.md),
com gerador reprodutível (`scripts/bench-pos.sh`), que dá:

- o que é **atribuível** (tokens, contagens de eventos) **entra** no ledger;
- o que é **latência entre máquinas diferentes** **não entra** — o número existe, está no artefacto,
  e fica de fora porque comparar 360 ms num Ryzen 5 com 78 ms num Ryzen 7 é uma inferência, não uma
  medição.

Este é o critério aplicado a todo o programa: *se a comparação não é defensável, o número fica fora
do ledger, visível no artefacto.*

Continua: [04 · falsabilidade e limites](04-falsabilidade.md).