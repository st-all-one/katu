# 04 · Falsabilidade e limites

> O perímetro da [tese](00-tese.md). Um projecto que só publica conformidade não está a medir nada; este documento lista o que o katu **não** prova, com o número que falta ao lado.

## 1. As afirmações que não fazemos

| não afirmamos | porquê | onde está o número |
|---|---|---|
| «o projecto é rápido» | 92,6 % do turno é o provider; o ganho próprio foi de **fracção** (201 ‰ → 57 ‰), não de relógio | [`bench/e18/pos/`](../../bench/e18/pos/PROTOCOL.md) |
| «a memória melhora a resposta» | a fixture de confiança é **sintética**; o veredicto em dados reais é `unmeasured` (0 ensaios por regra) | [`bench/e18/confidence/`](../../bench/e18/confidence/PROTOCOL.md) |
| «as guardrails são uma fronteira» | a contenção é *soft*: o jail real está adiado para E17; o `kill(2)` de grupo é o único `unsafe` | [ADR 0004](../_ref/adr/0004-sem-ffi-kill-grupo-e17.md) |
| «os intervalos são honestos» | o conformal foi **rejeitado**: 5 de 8 linhas, queda de 37 ‰ | [`bench/e18/conformal/`](../../bench/e18/conformal/PROTOCOL.md) |
| «o tecto de paralelismo está calibrado» | a máquina de medição não escalava (2 threads sem 1,5×); o número descreve o ambiente | [`bench/e18/pool/`](../../bench/e18/pool/PROTOCOL.md) |

## 2. Números que não entram no ledger

`unpriced` vale necessariamente zero. Hoje existem **quatro** linhas assim, e são todas
declarações de ausência de medição, não de ausência de efeito:

| métrica | valor | leitura |
|---|---|---|
| `mvk.cross_tool.gain_ratio` | 0,0 | o ganho **transversal** entre ferramentas nunca foi demonstrado |
| `provider.request_compression` | 0,0 | a compressão do pedido foi **rejeitada** pelos endpoints ([ADR 0013](../_ref/adr/0013-cache-de-prefixo-e-compressao-de-pedido.md)) |
| `w81.invalid_json_turns_avoided_ratio` | 0,0 | a gramática estruturada não foi comparada contra turnos perdidos |
| `q14.prewarm.*` (parcial) | — | o prewarm está medido, mas o efeito no turno real depende do servidor |

Um quarto número ficou **fora** por decisão metodológica, não por ausência: as latências de
[`bench/e18/pos/`](../../bench/e18/pos/PROTOCOL.md) estão no artefacto e **não** no `published.toml`,
porque a máquina de medição é outra. O gate não as chumbaria; a honestidade chumba.

## 3. Onde a medição é frágil

1. **A alocação do render.** O plano escrevia «falta verificar zero alocações». Medido: **1137
   alocações e ~154 kB por quadro** ([`bench/render/`](../../bench/render/PROTOCOL.md)). A afirmação
   era falsa por construção; o que se trava agora é o **teto de crescimento**, não o zero. Isto
   exigiu um `#[global_allocator]` — o **segundo** ponto `unsafe` do projecto, registado no gate com
   os seus próprios testes.
2. **Os Synthetic benches.** O conformal, o ledger, a distorção do contexto e a paralelelização são
   proxies determinísticos: medem a **estatística do mecanismo**, não o efeito no utilizador. Está
   escrito em cada `PROTOCOL.md`, mas é a forma mais fácil de enganar a si próprio.
3. **O modelo local não emite tool calls nativas.** Por isso os caminhos de tools em e2e não estão
   cobertos: o lote paralelo foi medido com um provider falso, e o ganho real num turno agêntico
   **não** está medido.
4. **A fracção não-provider é uma média de outro hardware.** 57 ‰ foi medido num Ryzen 7; o
   baseline é de um Ryzen 5. A fracção é comparável, os absolutos não.

## 4. O que quebraria a tese

Para que a tese seja falsificável, eis o que a derrubaria:

- Um caminho pelo qual um `ToolUse` chegue a executar **sem** `PolicyEvaluated` no log — o que
  quebraria T1.
- Uma divergência `derive_messages(log) ≠ prompt enviado` num cenário real (o taint torna isso
  mais difícil, não impossível).
- Uma medição de latência end-to-end na máquina declarada que **não** mostre a queda da fracção
  controlável — o que quebraria a leitura de [03](03-performance.md).
- Uma base de memória real que mostre que a nota **piora** o resultado (a fixture sintética não diz
  nada sobre isso).

Nenhuma destas quatro foi observada. Nenhuma delas está instrumentada de forma que a observássemos
sozinhas — é a próxima etapa honesta: um gate que corra o invariante `Model-visible ⟺ logged` sobre
um log real com tools, e não sobre uma fixture.

## 5. Decisões que vão contra a corrente

Registradas porque são as que um revisor entenderia primeiro:

- **`katu run` é um turno único.** O multi-turno é a TUI, síncrona. Um CLI de uma invocação não é
  mais simples de auditar.
- **Os argumentos crus do modelo não são logados.** O log guarda o `ToolUse` resolvido. É uma dívida
  declarada (E04/§42): o replay é fiel, a auditoria do *original* não.
- **Nada se escreve fora do projecto**, excepto a configuração global e a unidade systemd de
  utilizador (só com `--watch-service --install`). Isto é um gate (`check-paths`), não um documento.
- **A configuração global é a única excepção** à contenção de dados, porque é por definição
  partilhada entre projectos.

Continua: [05 · método](05-metodo.md).