# ADR 0013 — Cache de prefixo por modelo e compressão de pedido (medida)

- **Estado:** aceite
- **Data:** 2026-09-30
- **Decisões fundacionais:** DF8 (provider commodity), DF5 (evidência tipada — o negativo conta),
  DF9 (instrumentação)
- **Épicos:** E12 (T06/T07/T10), E18/F4 (latência)
- **Relaciona:** [ADR 0011](0011-porta-provider-e-builtin-opencode.md),
  [ADR 0012](0012-catalogo-dialetos-e-providers-declarativos.md), [plan/13](../../plan/13-providers.md)

## Contexto

Depois de parametrizar o wire por modelo (ADR 0012), faltava decidir **como explorar o cache de
prefixo** — o maior ganho de latência em conversas longas — e se valia a pena **comprimir o corpo
do pedido**. O modelo em causa é o `deepseek-v4.1-flash` (opencode go/zen), cuja tabela de custo
expõe `cache_read`, ou seja, o gateway serve prefixos em cache e reporta-os em `usage`. Medimos
ao vivo com a chave de desenvolvimento e um prefixo de ~1000 tokens.

## Decisão

1. **O cache de prefixo é capacidade do modelo, não do adaptador.** `prompt_cache` e
   `prompt_cache_retention` são campos declarativos (`ModelEntry`/`ProviderSpec`); o
   `prompt_cache_key` deriva da sessão de afinidade. Só os modelos que o declaram o recebem.
2. **Cabeçalhos de afinidade extra** (`affinity_headers`, ex. `x-client-request-id` e
   `x-session-affinity`) são declarativos e alinhados com o `pi`; o built-in ativa-os para o
   `opencode` (paridade com o seam do gateway).
3. **A compressão do corpo do pedido (`gzip`) fica opt-in e desligada por omissão.** É uma
   capacidade do `UreqTransport` (`with_request_compression`), mas **nenhum** dos endpoints
   built-in a aceita: o `opencode` responde `401` (o gateway não descomprime e perde o `model`) e o
   `llama-server` responde `415`. O negativo fica registado (DF5); não se liga no runtime.
4. **O dialeto `chat/completions` serializa direto** (structs `serde`), sem construir a árvore
   `serde_json::Value` intermédia — corta alocações/CPU sem alterar os bytes do pedido (prova: os
   testes de wire continuam verdes e a ordem dos campos é preservada).
5. **HTTP/2 fica explicitamente bloqueado.** O `ureq` 3 é HTTP/1.1; a troca de stack
   (`hyper`/`reqwest`) não se justifica para **um só stream** (sem multiplexagem) e pagaria
   `tokio`. A rota fica tipada (`Unsupported`) em vez de silenciosa.

## Alternatives considered

1. **Enviar `prompt_cache_key`/`prompt_cache_retention` a todos os modelos.** Rejeitada: campos
   desconhecidos nalguns gateways e nenhum ganho onde o modelo não tem cache; o `pi` só o faz em
   `api.openai.com` ou retenção longa.
2. **Guardar o cache só por `x-opencode-session` (sem campos).** Rejeitada: perde-se o
   `prompt_cache_retention` explícito onde ele é suportado; o custo é zero quando o modelo não o
   declara.
3. **Comprimir o pedido por omissão.** Rejeitada por medição: `opencode` devolve `401` e
   `llama-server` `415`; a "poupança" de bytes não paga a falha.
4. **Trocar o transporte para HTTP/2 agora.** Adiada: sem multiplexagem a ganhar, com `tokio` e uma
   reescrita do adaptador; o custo supera o ganho medido.
5. **Manter a árvore `Value` na codificação.** Rejeitada: o caminho quente paga alocações
   desnecessárias por turno; a serialização direta é estritamente melhor e testável.

## Consequências

- **Positivas:** o cache de prefixo é exercido e **provado** (`deepseek-v4.1-flash`, 2.º turno:
  `cached=896/1004`); a decisão é por modelo e declarativa; a codificação gasta menos CPU; o
  negativo da compressão está registado com evidência.
- **Negativas / dívida:** o TTFT do `opencode` continua dominado pelo gateway/modelo (fase de
  raciocínio), não pelo cliente; `prompt_cache_retention` não foi observado ao vivo (só a chave);
  HTTP/2 e WebSocket pendentes.
- **Travas:** testes de encode/decode por dialeto, `delta_reasoning_accepts_all_field_variants`,
  `endpoint_adds_affinity_headers`, `gzip_roundtrips_and_shrinks` e
  `prepare_compresses_only_above_the_threshold`; `make check` mantém o firewall e a instrumentação.
