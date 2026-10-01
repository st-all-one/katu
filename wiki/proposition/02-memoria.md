# 02 · Memória local e persistente

> Pilar 2 da [tese](00-tese.md). A afirmação concreta: **a memória do agente é um store local,
> persistente, auditável e substituível** — e o sistema não arranca sem ela.

## 1. A porta e a razão de existir uma porta

O núcleo não conhece o `knudge`. Define uma porta, `Memory`, com um vocabulário de tipos do katu
(`Recall`, `Remember`, `Query`), implementada por um adaptador. Isso dá três coisas que um cliente
embutido não dá:

1. **Substituibilidade verificada por gate** — `xtask check-memory-swap` corre a mesma suite contra o
   fake e contra o adaptador real; se um dia o armazenamento mudar, o contrato não muda.
2. **Testabilidade sem I/O** — o loop de turnos é testado com `Memory::fake` e sem rede.
3. **Uma fronteira de porta legível** — o `katu-core` não depende de nenhum motor de busca; depende
   de uma interface.

## 2. Abertura *fail-closed*: a decisão de project

`Runtime::open` monta sessão + adaptador + regras e **recusa arrancar** sem memória saudável. O
efeito é observável: sem adaptador, os comandos falham com **exit 10**, e não com um erro genérico
nem com um agente que "funciona mas não se lembra de nada".

Isto é uma escolha de tese, não um detalhe de implementação. Um agente sem memória não é um agente
degradado: é um agente diferente, com promessas diferentes. Preferimos não o ter.

## 3. Duas IAs, uma só verdade

O projecto distingue:

- **a IA que age** — o provider de chat, com o seu modelo, tier e catálogo;
- **a IA que indexa** — `embeddings.url`/`model`, uma segunda IA que produz vectores para o recall
  semântico.

A segunda IA é **opcional e explícita**: se `embeddings.url` estiver ausente, o estado é `off` e o
recall semântico não é fingido. [`memo doctor`](../_ref/docs/CLI_TUI_SURFACE.md) publica o estado
para que o utilizador saiba em que regime está.

A consequência de desenho é que "pesquisa semântica" não é uma função separate: é uma **etapa do
recall**, com o seu custo medido (`e18.embeddings.p50_ms = 16,1 ms` por nota). Um dreno de N notas
custa N × 16 ms, e isso está escrito para que ninguém o descubra tarde.

## 4. Recall: por que RRF e não uma só estratégia

O recall funde resultados heterogéneos (exactos, por tag, por vizinhança, por janela temporal) com
**Reciprocal Rank Fusion**:

```
score(d) = Σ_r  1 / (k + rank_r(d)),      k = 60
```

O RRF é escolhido por uma razão específica: **não tem parâmetros para calibrar**. Não há um peso
por estratégia a ajustar contra um gold standard que não existe, logo não há sobreajuste nem uma
decisão escondida. A alternativa (rank learning) foi avaliada e rejeitada: PPR exige um grafo, e o
rank actual é plano — o número está em
[04 · falsabilidade](04-falsabilidade.md).

## 5. Persistência: o log é a fonte, o índice é derivado

O modelo de dados é append-only com uma separação explícita:

| artefacto | papel | quem o pode reescrever |
|---|---|---|
| log (segmentos) | **verdade** do turno | só o motor, append-only, com hash encadeado |
| snapshot | estado reconstruído | derivado, substituível |
| índice invertido (`KAI1`) | accelerate a pesquisa | derivado, reconstruível |
| `.knudge/` | store de conhecimento do adaptador | derivado (`.idx/` pode ser apagado) |

O que não é derivado **não se apaga**: o `memo drain` apaga `.idx/` e redigere tudo de raiz, porque
o índice não tem nada que o log não tenha.

Medido ([`bench/e18/audit/`](../../bench/e18/audit/PROTOCOL.md) e `xtask bench-audit`):

- índice **−76 / −78 / −79 %** face à tabela de texto, a 1 000 / 10 000 / 100 000 eventos
  (4,85 MB contra 23,9 MB a 100 000);
- `decode` é ~8× mais rápido que `build` — a leitura é o caminho quente, e é a que foi
  optimizada.

## 6. Retomar é barato porque o estado é derivado

Retomar uma sessão **não** reproduz o log: lê o snapshot a partir do offset e a cauda
(`q15.resume.bounded = 321 µs` para 20 000 turnos, contra `23 839 µs` de replay total — **−98,65 %**).
O custo residual é o que o nome diz: *bounded*, com uma cauda de até 125 244 B.

A alternativa — replay integral, que era o estado anterior do projecto — é mais simples de provar e
100× mais lenta. Foi mantida como **modo** (o artefacto mede os dois) e não como padrão.

## 7. A memória é conteúdo não confiável

Uma nota é escrita pelo agente ou por um comando; pode conter instrução. O taint do
[guardrails](01-guardrails.md) aplica-se à mesma: o conteúdo da memória entra no prompt como
`<katu:untrusted>`, com o contrato ensinado no prime. Não é um detalhe: é o que impede que uma nota
se promova a instrução por acumulação.

## 8. O que este pilar não prova

- **Qualidade.** O `knudge-core` tem utilidade demonstrada nos seus próprios testes; a ponte
  "memória melhorou a resposta" **não** está medida. A fixture de confiança é sintética e o veredicto
  sobre dados reais é `unmeasured` — declarado no ledger, não escondido.
- **Escala.** Não há medição de recall com 10⁵ ou 10⁶ notas. A complexidade é sublinear no índice,
  mas o ponto de viragem não foi medido.
- **Esquecimento.** Não há política de decaimento no katu; o que existe é o decaimento do motor
  ([ADR 0023](../_ref/adr/0023-contexto-e-duas-ias.md)). Um store que só cresce é um passivo
  esperando uma factura de contexto.

Continua: [03 · performance](03-performance.md).