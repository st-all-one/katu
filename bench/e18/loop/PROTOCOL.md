# Q-12 · Guard de loop (F7) — protocolo e resultado

## Pergunta

O turno em ciclo patológico é cortado **antes** do teto global de passos, sem cortar um turno
normal? (Brecha (h) do `OPTIMIZATION_PLAN.md` §1.4: "só o kill switch global — queima orçamento
devagar".)

## Fórmula

Por passo com chamadas, a assinatura é `FNV-1a(nome ‖ 0x1f ‖ JSON canónico dos argumentos)` — sem
RNG, sem relógio, estável entre execuções. Duas estatísticas clássicas de deteção de mudança:

```
novidade  = |assinaturas novas| / |assinaturas do passo|        (milésimos)
repetição = 1 − novidade
CUSUM     S ← max(0, S + repetição − k)      alarme se S ≥ h
SPRT      Λ ← Λ + log(p₁/p₀)  (passo todo repetido)
          Λ ← Λ + log((1−p₁)/(1−p₀))  (caso contrário)
          alarme se Λ ≥ log((1−β)/α)
```

Parâmetros (`GuardParams::DEFAULT`): `k = 0,5`, `h = 2,0`, `p₀ = 0,1`, `p₁ = 0,6`, `α = 0,01`,
`β = 0,10`, `min_steps = 3`. São **dados** (DF8), não derivados da observação.

**Progresso reinicia**: um passo com uma chamada **exclusiva** (altera o workspace: `write`, `edit`,
`move`, `trash`, `bash`, `memory`...) zera `S`, `Λ` e a memória de assinaturas. É esta a razão pela
qual um *polling* legítimo (`bash "make test"` em ciclo) **não** é cortado — só o ciclo de leitura
sem progresso é.

O corte é **nunca silencioso**: emite `agent.loop` (id estável no catálogo), fecha o turno
(`TurnEnd`) e devolve um erro `LoopDetected` (categoria `conflict`, exit code 5) com o motivo e a
evidência.

## Cenário

Turnos sintéticos determinísticos, alimentados pelo detector de **produção**:

- **normal**: 12 passos, cada um com uma leitura de um caminho **novo** e, a cada 5 passos, uma
  escrita (progresso);
- **loop**: a mesma leitura, sempre.

## Como correr

```sh
KATU_LOOP_OUT=$PWD/bench/e18/loop/raw.json \
  cargo test -q -p katu-core --lib -- --ignored ab_loop_guard
```

O caminho de produção tem teste de integração próprio (provider *fake*, sem rede):
`cargo test -p katu --bin katu agent::tests::guard`.

## Resultado (artefacto `raw.json`)

| | valor |
|---|---|
| falsos positivos em **200** turnos normais de 12 passos | **0** (taxa 0,000) |
| primeiro alarme num turno normal | **nenhum** (`S = 0`, `Λ < 0`) |
| primeiro alarme num turno em ciclo puro | **passo 4** (SPRT) |
| teto global de passos (referência) | 12 |
| determinismo | mesma entrada ⇒ mesmo alarme |

O 1.º passo é novidade e contribui **contra** o SPRT (`log(0,4/0,9) = −811`), pelo que o alarme cai
no 4.º passo (`3 × 1 792 − 811 = 4 565 ≥ 4 500`); o CUSUM, se fosse o único detetor, precisaria de
**5** repetições (`5 × 500 ≥ 2 000`). O corte acontece **8 passos** antes do teto de 12 — cada passo
poupado é um pedido ao provider inteiro.

## Custos

- **Nenhum token de prompt**: o guard é invisível ao modelo (é um travão do lado do kernel).
- Custo por passo: um `BTreeSet` de assinaturas (uma por chamada), um `String` temporário por
  chamada para o hash e duas contas em ponto flutuante (`ln`). Medido no caminho de produção pelos
  testes; nenhum I/O.
- Um falso positivo custa um turno (o utilizador repete o pedido); daí o critério ser **zero** em
  200 turnos normais, e não uma taxa pequena.

## Limites

- As sequências são sintéticas: o modelo local **não** emite *tool calls* nativas (declarado em
  `bench/e18/REPORT.md`), pelo que um A/B com um modelo real não corre nesta máquina. O que se mede é
  o **detetor** (falso positivo, alarme cedo, determinismo), não o comportamento de um modelo.
- A memória de assinaturas é por turno e não tem teto explícito: um turno com centenas de chamadas
  distintas guarda centenas de `u64` (bytes, não KB) — aceite e declarado.
- Não distingue "o modelo está a repetir porque não tem informação" de "o modelo está a repetir
  porque o *prompt* é ambíguo": o corte diz o que se repetiu, não a causa.
- Um ciclo que **escreve** (progresso) não é cortado: é uma escolha explícita (o *polling* legítimo
  tem precedência) e está documentada no módulo.
- Sem *anytime-valid* / e-values (C1 do Anexo A fica em aberto): o `α` do SPRT é nominal, não
  corrigido para parada opcional.
