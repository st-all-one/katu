# ADR 0014 — Orçamento de latência do provider: gate offline determinístico

- **Estado:** aceite
- **Data:** 2026-09-30
- **Decisões fundacionais:** DF5 (evidência tipada), DF8 (provider commodity), DF9 (instrumentação)
- **Épicos:** E12-T07, E15-T02, F4/E18-T04
- **Relaciona:** [ADR 0011](0011-porta-provider-e-builtin-opencode.md),
  [ADR 0013](0013-cache-de-prefixo-e-compressao-de-pedido.md),
  [plan/19](../plan/19-otimizacao-profunda.md), [plan/13](../plan/13-providers.md)

## Contexto

Faltava **fechar** o orçamento de latência de E12-T07: publicar o número com base tipada e artefacto
cru (DF5) e travar regressões (E15/E18 §0.3). Uma medição *live* (TTFT contra o `llama-server` ou o
gateway `opencode`) é a que o utilizador sente, mas depende da máquina, da rede e do gateway — não
serve como portão de CI. O `ureq` é bloqueante e a stack é HTTP/1.1; nada aqui é assíncrono.

## Decisão

1. **O gate é offline e determinístico.** `xtask gate:provider` dirige o built-in sobre o
   `MockTransport` com um corpus SSE canónico (512 deltas + tool call + usage) e mede o
   **overhead de cliente por turno** (encode + decode + eventos). É reprodutível e trava
   regressões grosseiras (ex.: reintroduzir um clone/buffer O(história) por turno).
2. **O orçamento é um dado versionado**, não uma constante de código:
   `bench/providers/budget.toml` (`client_overhead_p95_nanos`). Regressão acima dele **falha**; a
   folga evita um portão de hardware (o alvo é adotar-ou-reverter).
3. **A latência *live* é publicada como artefacto**, não como limite: `bench/providers/latency.json`
   com `os`/`arch`, percentis *nearest-rank* e `usage` (`provider_reported`). TTFT p50 em
   `published.toml` cita esse artefacto.
4. **O negativo fica visível:** a compressão do pedido pouparia bytes mas os endpoints rejeitam-na
   (`unpriced`, ADR 0013); HTTP/2 fica bloqueado pelo transporte.
5. **A razão de acerto de cache** entra no artefacto com base `provider_reported`
   (`cached`/`input`), sem inventar percentis.

## Alternatives considered

1. **Gate sobre a medição *live*.** Rejeitada: rede/gateway/máquina tornam o portão instável; o
   artefacto live continua a existir para inspeção, mas não trava.
2. **Gate por *ratio* contra um baseline commitado.** Rejeitada: o baseline é de outra máquina
   (mesmo `os`/`arch` varia); um orçamento absoluto com folga é mais honesto e simples.
3. **`criterion`/`hyperfine`/`dhat` já (E18-T10).** Adiada: o harness estatístico completo é uma
   frente própria; o instrumento determinístico cobre o essencial de E12-T07 agora.
4. **Não publicar o TTFT live (só offline).** Rejeitada: o número que o utilizador sente é o live;
   publicá-lo com base `measured` e artefacto é o que DF5 exige.
5. **Medir também o gateway `opencode` no gate.** Rejeitada aqui: exige chave e rede; corre-se
   manualmente (`bench-provider --live --provider opencode-go`) e regista-se como artefacto.

## Consequências

- **Positivas:** `make check` inclui um portão de latência barato, determinístico e honesto; todo
  número publicado tem base e artefacto; o negativo da compressão está visível.
- **Negativas / dívida:** o alerta é grosseiro (não deteta regressões finas); `criterion`/`dhat` e
  a matriz por dialeto/transporte ficam para E18-T10; a comparação entre máquinas exige ler o
  artefacto.
- **Travas:** `cargo test -p katu-providers`, os testes de `report.rs` (corpus/percentis),
  `gate:bench` e `gate:provider` no `make check`; `scripts/check_file_length.sh` mantém o
  instrumento sob 300 linhas.
