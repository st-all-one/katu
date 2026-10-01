# 0025 — Decodificação estruturada por JSON Schema: opt-in, fail-open

- **Estado:** aceite
- **Épico:** W8-1 / B1 (Anexo A)
- **Supersedes:** —
- **Relação:** estende o catálogo de dialetos (ADR 0012) e a parametrização do wire (ADR 0013);
  materializa B1 do Anexo A do `OPTIMIZATION_PLAN.md`

## Contexto

Hoje o modelo **declara** uma tool call como texto JSON (`function.arguments`) e o katu descodifica-o
em `wire::parse_arguments`. Se o JSON vier truncado ou inválido, o turno morre em
`ProviderError::Decode` — uma classe de falha que nenhuma política pode recuperar, porque o dano já
aconteceu no wire. O `llama-server` e os gateways `OpenAI`-compatíveis aceitam um campo
`response_format` com um JSON Schema (`json_schema`), que restringe a geração **por construção**.

Duas tensões obrigam a decidir por escrito:

1. **O custo é real e cresce com o catálogo.** O schema é derivado das tools do pedido; com três
   tools representativas o corpo passa de **970** para **1 932** bytes e a codificação p95 de
   `80,8 µs` para `178,2 µs` (`bench/e18/grammar/raw.json`, 200 repetições, IC 95 % do harness de W7).
   Ligá-lo por omissão mudaria o contrato de bytes de quem já corre o produto.
2. **Nem todos os endpoints o aceitam.** Um gateway remoto pode recusar o campo com `400`. Um
   provider que o enviasse sempre ficaria dependente de suporte que não controla.

## Decisão

**A decodificação estruturada é um campo opcional, derivado das tools do pedido, opt-in por
provider e fail-open.**

- **Forma:** com `structured_output` ligado e tools no pedido, o dialeto `chat/completions`
  acrescenta `response_format: {type: "json_schema", json_schema: {name: "katu_tool_call",
  strict: true, schema: {oneOf: [...]}}}`. Cada variante liga o **nome** (`const`) aos
  `parameters` da tool — o schema que `katu_tools::schema` já valida no CI. O modelo não pode
  gerar argumentos que não validem: a classe `Decode` fecha-se **por construção**.
- **Opt-in:** `provider.structured_output` (config fechada, default `false`) para o `llama` local,
  `structured_output` no JSON declarativo, `ModelEntry::with_structured_output` e
  `LlamaConfig::with_structured_output`. Nenhum provider o liga sozinho.
- **Fail-open:** um `400` do endpoint faz o pedido ser repetido **sem** o campo; o comportamento
  volta a ser o atual. Desligado, o corpo é **byte a byte** o de hoje (o campo é omitido, não
  enviado a `null`).
- **Não muda o plano de dados:** muda como o modelo *declara* a chamada; a política, a sandbox e o
  pipeline de tools ficam iguais.

## Alternatives considered

1. **Ligar por omissão em todos os providers `chat/completions`.** Rejeitada: +962 bytes por pedido
   e ~4× a codificação com três tools; um endpoint que recuse o campo passaria a falhar turnos que
   hoje passam. O custo é medido e a decisão fica com quem o paga.
2. **Ligar só quando o catálogo declarar suporte e nunca fazer fallback.** Rejeitada: um catálogo
   desatualizado torna o turno impossível; o fail-open mantém o comportamento atual em vez de
   transformar uma capacidade em requisito.
3. **Gramática GBNF em vez de JSON Schema.** Rejeitada: é específica do `llama.cpp`, não viaja nos
   dialetos `OpenAI` e obrigaria a manter duas representações. O JSON Schema deriva da fonte única
   (`katu_tools::schema`).
4. **Validar/reaver os argumentos depois de chegarem (`parse_arguments` com reparação).** Rejeitada:
   repara o sintoma, não a causa, e pode *inventar* argumentos que o modelo não pediu — contra a
   honestidade do log.
5. **`tool_choice: "required"` ou `strict` nas funções sem schema de topo.** Rejeitada como
   substituto: não restringe os argumentos por tool; fica como controlo ortogonal, se um dia for
   medido.

## Consequências

- **Positivas:** a classe de falha "JSON inválido" deixa de poder acontecer quando ligada; a
  capacidade é dado explícito (config/TOML), não uma convenção enterrada no wire; o default não
  muda o contrato de bytes.
- **Negativas / dívida:** o pedido cresce com o catálogo (custo declarado no artefacto); o
  `strict: true` e o `oneOf` são o dialeto `OpenAI`, não uma garantia universal — depende do
  fail-open. A adoção por omissão exige ≥ 20 % dos turnos falhados evitados em turnos reais, número
  que **ainda não é medível** aqui (o modelo local não emite tool calls nativas); fica `unpriced`.
- **Travas:** `structured_output_is_off_by_default` (desligado = bytes atuais),
  `structured_output_adds_a_schema_derived_from_the_tools` (o `oneOf` liga nome a parâmetros),
  `the_schema_excludes_malformed_arguments` (o fixture truncado não é gerável) e
  `a_rejected_structured_request_falls_back_to_the_current_bytes` (fail-open). O artefacto é
  regenerável por `KATU_GRAMMAR_OUT=... cargo test -- --ignored ab_grammar_by_artifact` e validado
  por `gate:bench`.
