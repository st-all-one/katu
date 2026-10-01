# Q-07 · `edit` multi-bloco atómico — protocolo e resultado

## Pergunta

Um refactor canónico de 5 *hunks* custa menos com **uma** chamada atómica do que com 5 chamadas
separadas? E o que acontece ao ficheiro quando a 3.ª falha?

## Fórmula

O que é **determinístico** (e por isso medível sem modelo):

```
ganho_chamadas_pct = (1 - chamadas_depois / chamadas_antes) * 100
ganho_payload_pct  = (1 - bytes_depois / bytes_antes) * 100
atomicidade        = bytes gravados quando a 3.ª substituição falha
```

`chamadas` conta pedidos de tool (cada um é um `ToolCall` + `ToolResult` no log, uma avaliação de
política e um *delta* no pedido seguinte); `bytes` é o `arguments` que o **modelo** tem de escrever,
serializado com `serde_json` nas duas formas.

O que **não** é determinístico: o número de **turnos**. Com B-02 (várias calls por passo) o modelo já
pode agrupar as 5 chamadas num passo; com uma call por passo, a forma antiga custa 5 passos. O A/B de
turnos com o modelo **não corre localmente** (o modelo local não emite tool calls nativas) — está
declarado, não escondido.

Base **`measured`**: contagens e bytes exatos. O critério de adoção do plano (≥ 20 % menos turnos) é
avaliado sobre o proxy determinístico (chamadas), com a limitação escrita.

## Cenário

Um ficheiro Rust com 5 funções, cada uma com `let allowed = policy.authorize(&use_)?;`. O refactor
renomeia a chamada em todas (`authorize` → `check`), com contexto suficiente para cada âncora ser
**única** (é o caso canónico de refactor multi-sítio).

## Como correr

```sh
KATU_EDIT_OUT=$PWD/bench/e18/edit/raw.json \
  cargo test -q -p katu-tools --lib -- --ignored ab_multi_block_edit
```

## Resultado

| | antes (N chamadas) | depois (1 chamada) | ganho |
|---|---|---|---|
| chamadas | 5 | **1** | **−80,0 %** |
| bytes do payload do modelo | 763 B | **611 B** | **−19,9 %** |
| eventos de log (`ToolCall`+`ToolResult`) | 10 | **2** | −80 % |
| ficheiro final | idêntico | idêntico | — |

**Quando a 3.ª substituição falha:**

| | estado do ficheiro |
|---|---|
| forma antiga | **375 B** (2 de 5 aplicadas — refactor a meio) |
| forma atómica | **383 B** = o original, **intocado** |

O critério (`≥ 20 %`) é cumprido no proxy de chamadas (−80 %) e a atomicidade é uma propriedade
**medida**, não uma promessa: a forma antiga deixa o ficheiro num estado intermédio (375 B) e a nova
não escreve nada (383 B = original).

## Custos

- **Prompt**: o par `old`/`new` passou a lista no JSON Schema → o wire das tools cresceu
  **+162 B / +45 tokens** (5 013 → 5 175 B; `bench/e18/prompt/raw.json`). Não é um parâmetro novo (a
  superfície continua com 11 tools), mas **é** um custo e está contabilizado no `gate:prompt`.
- **Aprendizagem**: o modelo tem de escrever `["…"]` em vez de `"…"`. A forma única continua
  **aceite** pelo roteador (um texto é uma lista de um), pelo que nenhum comportamento antigo
  quebra; o que muda é o que o schema **declara**.

## Limites

- Mede chamadas/bytes/atomicidade, **não** turnos nem sucesso de tarefa: o proxy de chamadas só se
  traduz em turnos quando o modelo não agrupa calls no mesmo passo.
- O cenário é um refactor de 5 *hunks* num ficheiro; refactors multi-ficheiro continuam a exigir uma
  chamada por ficheiro (a atomicidade é **por ficheiro** — não há transação entre ficheiros).
- As âncoras sugeridas (Q-08) são heurísticas determinísticas (prefixo comum + unicidade), não um
  *match* semântico: podem não incluir a linha certa quando o trecho procurado não partilha prefixo
  com nada no ficheiro.
