# `katu-providers`

**Épico:** E12 · **Fase:** 8 · **Único crate que fala com modelos** (DF8).

A camada de **providers**: o caminho built-in first-party é nosso; o resto é commodity.

## Responsabilidade

- Built-in: gateway `opencode go/zen` (hot path, stateless) + `llama.cpp` local (opcional).
- Demais providers via **GDK/declarativo**, ou ativamente ignorados.
- O seam é um **endpoint de modelo**, nunca o agente.
- Latência > compressão no hot path (keep-alive, SSE incremental, prefix-cache — E18/F4).

## Fronteira

- É **cliente** do plano de dados, não substrato do loop (firewall LLM-free).
- `katu-core`/`katu-policy`/`katu-tools` nunca dependem deste crate.
