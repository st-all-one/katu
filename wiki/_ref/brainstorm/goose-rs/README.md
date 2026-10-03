# Dossiê goose-rs

> Análise factual e em profundidade de **como o `_REF/goose/` está implementado, de fato** — arquitetura, crates, tipos, fluxos e decisões de engenharia — com lições aplicáveis ao projeto `katu`.

**Alvo analisado:** `_REF/goose/` · commit `4dea9b4` · branch `main`
**Projeto:** goose — *"your native open source AI agent — desktop app, CLI, and API"*
**Organização:** Agentic AI Foundation (AAIF), Linux Foundation
**Licença:** Apache-2.0
**Linguagem:** **Rust** (workspace Cargo) + Electron/TypeScript (desktop)
**Versão do workspace:** `1.52.0` · toolchain `1.96.1` · edition 2021

> **Observação importante:** diferente do Pi (TypeScript → proposta de reescrita), o goose **já é um projeto Rust**. Este dossiê, portanto, não propõe uma reescrita: documenta fielmente a implementação existente e extrai as decisões de engenharia que valem ser replicadas ou evitadas em `katu`.

---

## Números-chave

| Métrica | Valor |
|---|---|
| LOC Rust (`crates/*/src`) | **278.298** linhas |
| LOC desktop (`.ts`/`.tsx`) | **89.240** linhas |
| Crates no workspace | **15** |
| Testes (`#[test]` + `#[tokio::test]`) | **≈3.972** |
| Providers declarativos embutidos (JSON) | **48** |
| Arquivo mais pesado (Rust) | `agents/agent.rs` — 6.164 linhas |
| Storage | **SQLite** (via `sqlx`), 3 tabelas centrais |

---

## Sumário

| # | Documento | Conteúdo |
|---|-----------|----------|
| 01 | [Visão geral](01-visao-geral.md) | O que é o goose, produto, componentização, ciclo de vida |
| 02 | [Workspace e crates](02-workspace-e-crates.md) | Grafo de crates, o que é GDK, dependências centrais |
| 03 | [Runtime do agente](03-runtime-do-agente.md) | `Agent`, pipeline `reply`, loop legado vs. state machine |
| 04 | [Máquina de estados](04-maquina-de-estados.md) | Crate `goose-agent`, `Step`/`Operation`/`Inference`/efeitos, ordem do pipeline |
| 05 | [Providers](05-providers.md) | Trait `Provider`, providers declarativos, formats, registro canônico, `toolshim` |
| 06 | [Extensões e MCP](06-extensoes-mcp.md) | `McpClientTrait`, `ExtensionConfig`, extension manager, servidores MCP embutidos |
| 07 | [Sessão e storage](07-sessao-e-storage.md) | `SessionManager`, schema SQLite, modelo de `Message`, ledger de uso |
| 08 | [Contexto e compaction](08-contexto-e-compaction.md) | Limites, sumarização, tool-pair compaction, contagem de tokens |
| 09 | [CLI](09-cli.md) | Subcomandos, `session`/`run`, formatos de saída, integração de terminal |
| 10 | [ACP e protocolos](10-acp-e-protocolos.md) | ACP server/providers, `gateway`, `roaming` (iroh), `serve` HTTP |
| 11 | [GDK e SDK](11-gdk-e-sdk.md) | Crates publicados, bindings UniFFI, `sdk-types` |
| 12 | [Recipes, skills, hooks, plugins](12-recipes-skills-hooks-plugins.md) | Automação declarativa e sistema de permissões/segurança |
| 13 | [Desktop UI](13-desktop-ui.md) | Electron + React, `goose serve`, modelo de processo |
| 14 | [Testes e qualidade](14-testes-e-qualidade.md) | Suítes, cenários, self-test, replay MCP |
| 15 | [Decisões e lições](15-decisoes-e-licoes.md) | O que copiar, o que evitar, aplicação em `katu` |
| — | [GOOSE_LOOP](GOOSE_LOOP.md) | **Deep dive:** o loop, os turnos, a sessão e o I/O do modelo, código a código |
| — | [GOOSE_VS_KATU_LOOP](GOOSE_VS_KATU_LOOP.md) | Comparação com o loop do `katu`: lacunas e o que incorporar (interrupção, fim de turno, sessão) |

---

## TL;DR da arquitetura

```
┌──────────────── Interfaces ────────────────┐
│  CLI (clap)        Desktop (Electron/React) │
│  ACP server (stdio/HTTP/WS)                 │
└───────────────┬─────────────────────────────┘
                │  AgentEvent stream
┌───────────────▼─────────────────────────────┐
│              Agent (crates/goose)           │
│  ┌────────────────────────────────────────┐ │
│  │  StateMachine (goose-agent, genérico)  │ │
│  │  Step = Operation | Inference          │ │
│  │  OperationResult → Effects             │ │
│  └────────────────────────────────────────┘ │
│  Providers · Extensions(MCP) · Hooks        │
└───────────────┬─────────────────────────────┘
                │
┌───────────────▼─────────────────────────────┐
│  SessionManager (SQLite: sessions,          │
│  messages, usage_ledger)                    │
└─────────────────────────────────────────────┘
```

O goose é um **runtime de agente orientado a eventos e persistido a cada passo**, onde o loop do agente está migrando de um método monolítico (`Agent::reply`, 6k linhas) para uma **máquina de estados reutilizável** (`goose-agent`) composta por operações plugáveis e executada sobre a conversa persistida.
