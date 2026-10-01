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
e-value   log Λ ← log Λ + log(p₁/p₀)  (passo todo repetido)
          log Λ ← log Λ + log((1−p₁)/(1−p₀))  (caso contrário)
          alarme se log Λ ≥ log(1/α)
```

Parâmetros (`GuardParams::DEFAULT`): `k = 0,5`, `h = 2,0`, `p₀ = 0,1`, `p₁ = 0,6`, `α = 0,01`,
`min_steps = 3`. São **dados** (DF8), não derivados da observação.

**Anytime-valid (C1/W8-3).** A razão de verosimilhança `Λ_n` é um **martingale não-negativo** sob
`H0`; por **Ville**, `P_{H0}(∃n: log Λ_n ≥ log(1/α)) ≤ α` para **qualquer** regra de parada. É este
o corte formal — o SPRT clássico (`log((1−β)/α)`) só garante `α` a `n` fixo (o limite teórico é
`α/(1−β)`). O CUSUM mantém-se como sinal para a repetição parcial, onde a e-value não avança.

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
| primeiro alarme num turno em ciclo puro | **passo 5** (e-value) |
| teto global de passos (referência) | 12 |
| erro tipo I sob parada opcional (DP exata) | **≤ α** (`4‰` em 12 passos; fronteira nominal do SPRT: `6‰`) |
| determinismo | mesma entrada ⇒ mesmo alarme |

O 1.º passo é novidade e contribui **contra** (`log(0,4/0,9) = −811`), pelo que a e-value cruza
`log(1/0,01) = 4 605` no 5.º passo (`4 × 1 792 − 811 = 6 357`); a fronteira nominal do SPRT
(`4 500`) cruzaria no 4.º, mas sem a garantia anytime-valid. O corte acontece **7 passos** antes do
teto de 12 — cada passo poupado é um pedido ao provider inteiro.

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
- A e-value é **conservadora**: paga um passo a mais no alarme em troca da garantia de cobertura a
  qualquer `n`; a fronteira nominal do SPRT continua disponível como sinal, não como corte.
