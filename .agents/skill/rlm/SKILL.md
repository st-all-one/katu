---
name: rlm
description: >
  Recursive Language Model (RLM): process contexts larger than an LLM window via
  a 3-layer architecture (Root LLM, REPL, Sub-LLM). Minimal and full
  implementations, REPL environments, LLM clients, prompt engineering, RL
  training, advanced patterns. Load when handling massive documents/logs,
  near-infinite contexts, or recursive delegation to sub-LLMs.
category: ai
version: "1.0"
tags: [rlm, recursive-language-model, llm, agents, repl, context-window, sub-llm, rl, training]
license: MIT
---

# RLM — Recursive Language Model

Replace `llm.completion(prompt)` with `rlm.completion(context, query)`: a Root LLM orchestrates a Python REPL and delegates semantic work to cheaper Sub-LLMs, dividing and conquering contexts that exceed the window.

## Use When
- Documents/logs/data larger than the model context window
- Programmatic semantic search (needle-in-a-haystack, aggregated summaries)
- Agents that write/run Python over their own context
- Delegating extraction/classification/summarization to sub-LLMs
- Implementing the minimal or full architecture
- RL training/eval of RLM-capable models
- Comparing RLM vs RAG, Map-Reduce, static-tool agents

## Avoid When
- Context fits the model window (use a direct prompt)
- Retrieval is already indexed with embeddings (RAG is simpler)
- The task is agent configuration rather than inference (out of scope)

## Core Rules
- Root LLM orchestrates; Sub-LLM analyzes chunks; the REPL is the "infinite canvas".
- Persist state across iterations; expose `llm_query()`/`rlm_query()` in the REPL.
- Use a cheaper Sub-LLM for scans; the Root decides and aggregates.
- Prefer programmatic search over chunk+embed when there is no index.
- Bound `max_iterations` and define a stop criterion (`FINAL(...)`).
- Sandbox the REPL (Docker/isolated) when executing generated code.
- Log every iteration for reproducibility/debugging.
- Manage cost: cap sub-calls and context passed per call.

## Core Pattern
```python
from rlm.rlm_repl import RLM_REPL

rlm = RLM_REPL(
    model="gpt-4o",              # Root LLM (orchestrator)
    recursive_model="gpt-4o-mini",  # Sub-LLM (analyst)
    max_iterations=10,
)
result = rlm.completion(context=huge_text, query="What is the magic number?")

# Internally:
#   iteration N: Root writes Python over `context`
#                -> llm_query("...") delegates to Sub-LLM
#                -> buffers results
#   stop:        Root returns FINAL(answer)
```

## File Map
| File | Content |
|---|---|
| `00-overview.md` | Concept, 3-layer architecture, comparisons |
| `01-architecture.md` | Root LLM, REPL, Sub-LLM, flow, state |
| `02-minimal-implementation.md` | ~360-line reference implementation |
| `03-full-implementation.md` | Production implementation |
| `04-environments.md` | REPL environments (local, Docker, IPython, Modal) |
| `05-clients.md` | LLM backends (OpenAI, Anthropic, Gemini, Azure) |
| `06-prompts.md` | Root/Sub prompt engineering |
| `07-training.md` | RL training (verifiers, prime-rl) |
| `08-advanced-patterns.md` | Advanced patterns and custom tools |
| `09-practical-examples.md` | End-to-end examples |
| `10-api-reference.md` | API reference |
| `11-rust-implementation-analysis.md` | Rust implementation analysis |
| `12-pi-rlm-proposal-analysis.md` | Integration proposal analysis |

## Read Order
`00`→`01`→`02` (minimal) → `03`+`04`+`05` (production) → `06`→`08`.

## Prereqs
Python 3.10+, an LLM backend, prompt-engineering basics; RL optional.
