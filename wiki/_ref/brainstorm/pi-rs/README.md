# Dossiê: Reconstrução do Pi em Rust (`pi-rs`)

Este diretório contém a análise do monorepo TypeScript `_REF/pi` (Pi Agent Harness) e a proposta de como reconstruí-lo em Rust, preservando comportamento, contratos de dados e experiência de uso.

## Sumário

### Núcleo conceitual (leia primeiro)

| # | Documento | Conteúdo |
|---|-----------|----------|
| **00** | [**A Filosofia do Pi**](00-filosofia-do-pi.md) | Tese central, leis, confiabilidade, performance e tradução para Rust |
| **00b** | [**Playbook: Confiabilidade e Performance**](00b-playbook-confiabilidade-performance.md) | Como aplicar em Rust: tipos, hot path, verificação, checklist, anti-padrões |

### Dossiê técnico

| # | Documento | Conteúdo |
|---|-----------|----------|
| 01 | [Análise do codebase](01-analise-codebase.md) | Estrutura, pacotes, métricas, dependências e fluxos |
| 02 | [Arquitetura Rust proposta](02-arquitetura-rust.md) | Workspace Cargo, camadas, decisões estruturais |
| 03 | [Mapeamento de pacotes](03-mapeamento-crates.md) | Pacote TS → crate Rust, responsabilidades |
| 04 | [Camada de IA (`pi-ai`)](04-camada-ai.md) | Providers, streaming, tools, auth, catálogo de modelos |
| 05 | [Runtime do agente](05-runtime-agente.md) | Agent loop, eventos, harness, compactação, sessões |
| 06 | [TUI (`pi-tui`)](06-tui.md) | Rendering diferencial, alt-screen, componentes, native |
| 07 | [Ferramentas (tools)](07-ferramentas.md) | read/bash/edit/write/grep/find/ls, contrato e schemas |
| 08 | [CLI e modos](08-cli-e-modos.md) | Args, interactive, print, JSON, RPC |
| 09 | [Extensões e plugins](09-extensoes-e-plugins.md) | Sistema de extensões TS → alternativas em Rust |
| 10 | [Sessões e storage](10-sessoes-e-storage.md) | Formato JSONL em árvore, pendulum, durable, SQLite |
| 11 | [Protocolo e servidor](11-protocolo-e-servidor.md) | CBOR, framing, cliente/servidor remoto |
| 12 | [Dependências → crates](12-dependencias-rust.md) | Tabela de substituição de cada dependência npm |
| 13 | [Roadmap de migração](13-roadmap-migracao.md) | Fases, marcos e critérios de saída |
| 14 | [Estratégia de testes](14-estrategia-testes.md) | Conformance, fixtures, golden files, evals |
| 15 | [Riscos e decisões (ADRs)](15-riscos-e-decisoes.md) | Riscos técnicos e decisões arquiteturais |

## Resumo executivo

> **A filosofia em uma frase:** o Pi trata um agente de IA como um sistema durável de um nó com efeitos externos incertos — modela a incerteza explicitamente em vez de tentar eliminá-la, e ganha performance respeitando a economia do provider (contexto append-only / KV cache) em vez de micro-otimizar código. Ver [00-filosofia-do-pi.md](00-filosofia-do-pi.md).

O Pi é um **harness de agente de codificação auto-extensível**. O valor do produto não está em um único componente, mas na composição de quatro camadas:

1. **`pi-ai`** — API unificada multi-provider de LLM (OpenAI, Anthropic, Google, Bedrock, Mistral, xAI, Groq, OpenRouter, etc.), com streaming normalizado, tool calling, OAuth, custo/tokens.
2. **`pi-agent-core`** — runtime do agente: loop de turnos, execução de tools, eventos, sessão, compactação, harness durável.
3. **`pi-tui`** — biblioteca de TUI com rendering diferencial, alt-screen, imagens inline e componentes.
4. **`pi-coding-agent`** — o CLI que amarra tudo: ferramentas de arquivo/shell, extensões, skills, prompts, temas, pacotes e os modos interactive/print/json/rpc.

A reconstrução em Rust é viável e faz sentido por: binário único sem runtime Node, eliminação de N-API/native prebuilds, concorrência real com `tokio`, e uma base de tipos mais rígida para um protocolo binário (CBOR) e para o formato de sessão.

O maior custo **não** é o Rust, e sim a **superfície de providers** (~45), o **catálogo gerado de modelos** e o **sistema de extensões dinâmicas em TypeScript**. A recomendação é começar pelo núcleo (`ai` + `agent` + `tui` + tools + CLI print/interactive) e deixar extensões dinâmicas e o runtime remoto/`chord` para fases posteriores.

- **Estimativa de esforço total**: ~185k LOC de TS a portar (fora testes/docs), com reuso conceitual alto.
- **Escopo inicial recomendado (MVP)**: `pi-rs` executando um agente com `read`, `bash`, `edit`, `write`, streaming multi-provider (OpenAI + Anthropic + Google), sessão JSONL em árvore e modo print/interactive.
- **Licença de origem**: MIT (Copyright 2025 Mario Zechner). A reimplementação deve manter atribuição.
