# Q-11 · Confiança por artefacto (F6) — protocolo e resultado

## Pergunta

`Enforced` é uma categoria **declarada** no TOML. O log **sustenta-a**? E quanto custa prová-la?

## Fórmula

Por regra, um ensaio de Bernoulli é registrado **sempre que a regra recusa** (`Denied`, ou
`Unavailable` com `rule_id`, ou `ApprovalGranted`):

```
sucesso = a chamada recusada NÃO aparece executada sob o mesmo CallId
LB      = limite inferior do intervalo de Wilson unilateral (z = 1,645)
média   = (1 + sucessos) / (2 + ensaios)        (posterior Beta com prior uniforme)
```

Veredicto (`katu_policy::verdict`):

| condição | veredicto |
|---|---|
| `n = 0` | `unmeasured` — declarada, não medida |
| `n ≥ n_min` **e** `LB ≥ θ` | `enforced` — provada |
| caso contrário | `advisory` — com a evidência no motivo |

`θ = 900` milésimos (0,90), `n_min = 5`, `z = 1 645`. O limiar é **dado** (`Threshold::DEFAULT`),
nunca derivado dos dados (DF8).

Uma **contradição medida** é `n ≥ n_min` e `LB < θ` com ≥ 1 falha: há evidência suficiente e ela
**não** sustenta a categoria. Poucas observações (`n < n_min`) são *não provado*, não contradição.

## Calibração (C3/W8-2)

Além do LB, publica-se **ECE** e **Brier** do LB face à frequência empírica do próprio log (base
`inferred`; medida *in-sample* — mede o **conservadorismo** do limite, não o acerto do modelo):

```
previsto_i = LB_i (limite inferior de Wilson, milésimos)
desfecho_i = sucessos_i / ensaios_i
Brier      = Σ [ s_i·(1−p_i)² + (n_i−s_i)·p_i² ] / Σ n_i
ECE        = Σ_baldes (n_b / N) · |previsto_b − desfecho_b|      (10 baldes de 100 milésimos)
```

`calibrate` ignora veredictos sem observações (`n = 0`) e é determinístico (sem RNG; a ordem não
altera o resultado). O diagrama de fiabilidade (`bins`) acompanha o artefacto.

## Como correr

```sh
# artefacto determinístico (fixtures sintéticas sobre as funções de produção)
KATU_CONFIDENCE_OUT=$PWD/bench/e18/confidence/raw.json \
KATU_CONFIDENCE_FIXTURE=$PWD/bench/e18/confidence/fixture.v1.jsonl \
  cargo test -q -p katu-core --lib -- --ignored ab_confidence_by_artifact

# operacional: lê os logs reais (todos os `.katu/sessions/*/session.v1.jsonl`)
cargo run -q -p xtask -- policy:confidence
# ou um log específico (é assim que a fixture demonstra a contradição medida)
cargo run -q -p xtask -- policy:confidence bench/e18/confidence/fixture.v1.jsonl
```

## Resultado (artefacto `raw.json`)

| | valor |
|---|---|
| ensaios que **provam** a regra com registo perfeito | **n = 25** (LB `902 ≥ 900`) |
| o mesmo a `n = 24` | LB `899` (não prova) |
| 20 ensaios perfeitos | LB `881` |
| 20 ensaios com **1** violação | LB `804`, `contradiction = true` |
| sem observações | `unmeasured`, `contradiction = false` |
| determinismo (mesma entrada ⇒ mesmo resultado) | `true` |
| ECE do registo perfeito, n = 1 → 30 | `730‰` → `83‰` (**decresce**) |
| amostra mista (perfeito n ∈ {5,10,15,20,25,40,60} + 1 violação a n = 20) | `ECE = 98‰`, `Brier = 18‰` (195 ensaios) |

A fixture (`fixture.v1.jsonl`, 20 recusas + 1 execução sob o mesmo `CallId`) faz o comando
operacional **falhar**, com a evidência:

```
policy:confidence: 1 contradição(ões) medida(s):
  contain-read-outside-workspace: LB 804 < θ 900 com n = 20 (1 falhas em 20 ensaios): demovida a Advisory
```

## Dados reais (snapshot no fecho de Q-11)

`cargo run -q -p xtask -- policy:confidence` sobre as 35 sessões do repositório: **8** regras
`Enforced`, 178 eventos, **0** recusas registradas ⇒ os 8 veredictos são `unmeasured`. É honesto: o
modelo local não emite *tool calls* nativas (declarado em `bench/e18/REPORT.md`), pelo que não há
negações para medir. O mecanismo é medido nas fixtures; a medição real acumula à medida que houver
recusas. O comando entrou no `check` (`xtask/src/check.rs`), portanto uma violação real passa a
**falhar** o `make check`.

## Custos

- **Nenhum token de prompt**: nada aqui é model-visible (é auditoria).
- O comando operacional lê os logs (178 eventos em 35 sessões ⇒ ms). Não corre no arranque.
- Uma passagem O(eventos) com dois `BTreeSet`/`BTreeMap` (ids de chamadas executadas e ensaios).

## Limites

- Mede o invariante **recusa ⇒ não correu**, não o sucesso da tarefa. Um kernel correto dá sempre
  1,0: o valor da medida é ser um **detetor de regressão** (um bypass, uma capacidade indevida).
- Não mede se o **remédio** (Q-08) ensina: isso é comportamento do modelo e é do Q-12.
- O LB é unilateral a 95 % e **não** tem controlo de múltiplas comparações (C5 do Anexo A fica em
  aberto): com muitas regras, o erro família é maior do que 5 %.
- `unmeasured` não é um defeito: um projeto novo não tem ensaios. Só a contradição medida falha.
