# Análise: Implementação TypeScript (pi-rlm-proposal)

## Visão Geral

O `pi-rlm-proposal` (v0.1.3, por Mario Zechner) é uma **extensão Pi Agent** que implementa o RLM completo como tool registrado no ecossistema Pi. Diferente do Rust que é uma CLI standalone, este é um módulo nativo TypeScript que se integra diretamente ao agente.

```
┌─────────────────────────────────────────────────────────┐
│              Arquitetura Original (Python)               │
│  ┌──────────┐   socket    ┌───────────┐   API    ┌────┐ │
│  │ RLM      │◄───────────►│ LMHandler │─────────►│ LLM│ │
│  │ (orchestr│             └───────────┘          └────┘ │
│  │ + REPL)  │── executa Python com llm_query()         │
│  └──────────┘                                          │
└─────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────┐
│              Arquitetura Pi (TypeScript)                 │
│  ┌──────────┐  registerTool  ┌──────────┐   API  ┌────┐│
│  │ Pi Agent │◄──────────────►│ rlm tool │───────►│ LLM││
│  │ (host)   │                │ (engine) │        └────┘│
│  └──────────┘                │  ┌───────┤               │
│                              │  │planner│→ decompose    │
│                              │  │solver │→ solve         │
│                              │  │synth  │→ merge         │
│                              │  └───────┤               │
│                              └──────────┘               │
└─────────────────────────────────────────────────────────┘
```

**Resumo:** O Pi-RLM é o RLM completo (planner + solver + synthesizer) como extensão de um agente existente. Não tem REPL nem execução de código — delega ao Pi Agent para isso.

---

## Estrutura do Projeto

```
pi-rlm-proposal/
├── index.ts              (264 linhas)  — Extension entry point, registra tool "rlm"
├── src/
│   ├── types.ts          (82 linhas)   — Tipos core (RlmNode, RlmRunResult, etc.)
│   ├── schema.ts         (39 linhas)   — TypeBox schema para parâmetros do tool
│   ├── engine.ts         (573 linhas)  — THE CORE: loop recursivo RLM
│   ├── backends.ts       (833 linhas)  — Adaptadores LLM (SDK/CLI/tmux)
│   ├── prompts.ts        (86 linhas)   — Prompts planner/solver/synthesizer
│   ├── runs.ts           (132 linhas)  — RunStore: lifecycle de runs async
│   ├── cli.ts            (1029 linhas) — CLI wrapper standalone
│   └── utils.ts          (128 linhas)  — Utilitários compartilhados
├── bin/
│   └── pi-rlm.mjs        (794 linhas)  — Build JS do cli.ts
├── examples/
│   └── web-data-extraction/            — Exemplo com browser tools
└── .pi/settings.json                    — Configuração local Pi
```

**Total:** ~3.184 linhas TypeScript autorais

---

## Tipos Core

### RlmNode — A Árvore Recursiva

```typescript
interface RlmNode {
  id: string;                    // "n1", "n2", etc.
  depth: number;                 // 0 = root
  task: string;                  // O que este nó deve resolver
  status: NodeStatus;            // "running" | "completed" | "failed" | "cancelled"
  decision?: {                   // Decisão do planner
    action: "solve" | "decompose";
    reason: string;
    raw?: string;                // Output bruto do LLM
  };
  startedAt: number;             // timestamp
  finishedAt?: number;
  result?: string;               // Output final deste nó
  error?: string;
  children: RlmNode[];           // Sub-nós (se decomposto)
}
```

### PlannerDecision

```typescript
interface PlannerDecision {
  action: "solve" | "decompose";
  reason: string;
  subtasks?: string[];           // Apenas quando action="decompose"
}
```

### StartRunInput

```typescript
interface StartRunInput {
  task: string;                  // Tarefa original
  backend: RlmBackend;           // "sdk" | "cli" | "tmux"
  mode: RlmMode;                 // "auto" | "solve" | "decompose"
  async: boolean;                // Run assíncrono?
  model?: string;                // Modelo LLM a usar
  cwd: string;                   // Working directory
  toolsProfile: RlmToolsProfile; // "coding" | "read-only"
  maxDepth: number;              // Profundidade máxima da árvore
  maxNodes: number;              // Número máximo de nós
  maxBranching: number;          // Filhos máximos por nó
  concurrency: number;           // Nós concorrentes
  timeoutMs: number;             // Timeout por chamada
  tmuxUseCurrentSession?: boolean;
  piBin?: string;                // Path para binário Pi
}
```

---

## O Loop Recursivo RLM (engine.ts)

```
runNode({task, depth, lineage[], parentId}):
  1. GUARD: nodesVisited >= maxNodes? → skip
  2. Alocar nodeId, incrementar contadores
  3. CHECK abort signal → cancel se abortado
  4. CHECK forcedSolveReason:
     - depth >= maxDepth
     - nodesVisited >= maxNodes
     - remainingNodeBudget < 2
     - ciclo detectado (task normalizada na lineage)
     → Se forced OU mode="solve": solveNode(), return
  5. PLAN: planNode() → PlannerDecision
     - Se mode="decompose" e planner diz "solve": ERRO
     - Se planner diz "solve": solveNode(), return
     - Se planner diz "decompose": extrair subtasks
  6. SANITIZE subtasks: dedup, remover vazios, remover matching parent
  7. BUDGET: trim para maxBranching, depois para childBudget
  8. FALLBACK: se < 2 subtasks válidos:
     - Se mode="decompose": ERRO
     - Senão: fallback solveNode()
  9. RECURSE: mapConcurrent(subtasks, concurrency, subtask => runNode(...))
     - lineage cresce: [...parentLineage, normalizedTask]
     - depth = parentDepth + 1
  10. CHECK resultados dos filhos:
      - Todos cancelled → pai cancelled
      - Todos failed → pai failed
  11. SYNTHESIZE: synthesizeNode() mergeando outputs dos filhos
  12. Retornar nó com resultado
```

### Três Chamadas LLM

| Tipo | Prompt | Input | Output Esperado |
|------|--------|-------|-----------------|
| **Planner** | "You are a recursion controller..." | task + depth + budget | `{"action":"solve\|decompose","reason":"...","subtasks":["..."]}` |
| **Solver** | "You are a worker node..." | task (forçada, se aplicável) | Texto livre com a resposta |
| **Synthesizer** | "You are the synthesizer node..." | parent task + child outputs | Texto unificado com merge |

### `mapConcurrent` — Pool de Concorrência

```typescript
// Cria min(concurrency, items.length) workers
// Cada worker puxa de um índice compartilhado
// Espera todos completarem antes de retornar
```

---

## Três Backends LLM

### 1. SDK Backend (In-Process)

```typescript
// Cria AgentSession do Pi internamente
const session = createAgentSession({
  resourceLoader: new DefaultResourceLoader({
    cwd: request.cwd,
    agentDir: getAgentDir(),
    noExtensions: true,
    noSkills: true,
    noPromptTemplates: true,
  }),
  tools: profileToTools(request.toolsProfile).split(","),
});
// Escuta eventos message_update para streaming
session.subscribe("message_update", (event) => {
  deltas.push(event.data.text_delta);
});
await session.prompt(request.prompt);
```

### 2. CLI Backend (Subprocess)

```bash
pi -p --no-session --no-extensions --no-skills --no-prompt-templates \
   --no-themes --tools read,bash,edit,write \
   --model <model> \
   -e <extensionPath> \
   "prompt"
```

### 3. tmux Backend (Visual)

```bash
# Cria sessão tmux pi-rlm-<runId>
# Cada depth=N recebe janela depth-N
# Panes tiled para concorrência
# Suporta tmuxUseCurrentSession para panes no sessão atual
```

---

## Integração com Pi Agent

### Como Extensão (`index.ts`)

```typescript
// package.json: "pi": { "extensions": ["./index.ts"] }
export default function activate(pi: ExtensionAPI) {
  pi.registerTool({
    name: "rlm",
    parameters: rlmToolParamsSchema,
    execute: async (params, ctx) => {
      // start: inicia run, retorna result (sync) ou run_id (async)
      // status: retorna info de runs
      // wait: espera run completar
      // cancel: aborta run via AbortController
    }
  });
}
```

### Como CLI (`src/cli.ts`)

```bash
# Spawning Pi com a extensão
pi -p --no-session --no-extensions --no-tools \
   -e <extensionPath> --mode json \
   "Use the rlm tool to analyze this document"

# CLI lê eventos JSON de Pi:
# tool_execution_start → tool_execution_update → tool_execution_end
```

---

## Bug Fix (fix_case.md)

### Bug 1: `--no-tools` → `--no-builtin-tools`

```diff
-  "--no-tools",
+  "--no-builtin-tools",
```

**Causa:** `--no-tools` desabilita TODAS as tools, incluindo a extensão `rlm`. O CLI não conseguia invocar a tool.

**Correção:** `--no-builtin-tools` desabilita apenas tools built-in do Pi (read, bash, etc.), preservando tools de extensões.

**Severidade:** Crítica — CLI não funcionava sem isso.

### Bug 2: SDK backend tool creation

```diff
-  createCodingTools,
-  createReadOnlyTools,
+  getAgentDir,

// Substitui:
-  const tools = request.toolsProfile === "read-only"
-    ? createReadOnlyTools(request.cwd)
-    : createCodingTools(request.cwd);
+  const tools = profileToTools(request.toolsProfile).split(",");
```

**Causa:** A API `createCodingTools`/`createReadOnlyTools` do Pi SDK mudou. O módulo `@mariozechner/pi-coding-agent` alterou sua interface.

**Correção:** Usa `getAgentDir()` para o ResourceLoader e strings simples para tools.

### Bug 3: Decompose mode fallback

```diff
-  throw new Error(
-    `mode=decompose requires planner action=decompose; got ${parsedForced.action}`
-  );
+  // Planner judged the task atomic — honor that instead of failing.
+  return { action: "solve", reason: parsedForced.reason };
```

**Causa:** `mode=decompose` exigia que o planner retornasse `action: "decompose"`. Se o planner julgasse a tarefa atômica, o engine crashava.

**Correção:** Trata `mode=decompose` como preferência, não requisito. Se o planner diz "solve", respeita.

---

## Ideias Inteligentes

### 1. Três Backends com Abstração Uniforme

```typescript
// Mesmo interface para SDK, CLI e tmux
const result = await completeWithBackend({
  backend: "sdk" | "cli" | "tmux",
  prompt: "...",
  cwd: "...",
  model: "...",
  toolsProfile: "coding" | "read-only",
  timeoutMs: 30000,
});
```

Isso permite:
- **SDK**: performance máxima, sem overhead de processo
- **CLI**: isolamento total, sem poluir o processo principal
- **tmux**: debugging visual com múltiplos panes

**Não existe no Python original** — o Python tem apenas o backend socket.

### 2. Live Tree Visualization

```bash
pi-rlm --live "Analyze this codebase"

# Output em tempo real:
# RLM run abc123 (auto, maxDepth=3)
# ├─ n1 [completed/solve] Analyze core modules ✓ (2.3s)
# ├─ n2 [running/decompose] Review test coverage...
# │  ├─ n3 [completed/solve] Unit tests ✓ (1.1s)
# │  └─ n4 [running/solve] Integration tests... (3.2s)
# └─ n5 [pending] Synthesize findings
```

Implementa polling de `events.jsonl` a cada 250ms, renderizando árvore ASCII com códigos ANSI. **Inovação significativa** para debugging de RLM recursivo.

### 3. Planner como Guardrail

```typescript
// Três modos de operação:
// "auto"    → planner decide solve/decompose
// "solve"   → força solve (árvore mono-nó)
// "decompose" → força decompose (com fallback gracefully)
```

O planner LLM age como **guardrail inteligente** — decide quando decompor e quando resolver diretamente. Isso é mais flexível que o Python original que usa regras fixas.

### 4. Ciclo Detection por Lineage

```typescript
// Normaliza task (lowercase, collapse whitespace)
// Compara com lineage do pai
const normalized = normalizeTask(params.task);
const cycleDetected = params.lineage.some(l => l === normalized);
// Se ciclo → força solve
```

Detecta quando o planner gera subtasks que são equivalentes à tarefa pai, evitando recursão infinita.

### 5. Event-Sourced State

```typescript
// Todos os eventos são escritos em events.jsonl:
// run_start, node_start, node_plan, node_solve, node_synthesize, run_end
// Permite:
// - Live monitoring (--live)
// - Post-hoc analysis (tree.json)
// - Replay de execuções
```

### 6. Zero Dependencies Runtime

```json
// package.json
"dependencies": {},  // ZERO runtime dependencies
"peerDependencies": {
  "@mariozechner/pi-coding-agent": ">=0.0.156",
  "@sinclair/typebox": "^0.34.0"
}
```

Apenas peer dependencies do Pi SDK. Nenhuma dependência externa.

### 7. Concurrency Pool com Budget

```typescript
// mapConcurrent limita concorrência
// Mas também respeita remainingChildBudget
const childBudget = Math.max(0, remainingBudget);
const maxChildren = Math.min(input.maxBranching, childBudget);
const subtasksToProcess = subtasks.slice(0, maxChildren);
```

### 8. Graceful Degradation em Múltiplos Níveis

```typescript
// Nível 1: planner falha → fallback para solve
// Nível 2: decompose com < 2 subtasks → fallback para solve
// Nível 3: todos filhos falham → nó falha
// Nível 4: todos filhos cancelados → nó cancelado
// Nível 5: ciclo detectado → força solve
// Nível 6: budget esgotado → skip nós restantes
```

---

## Comparação com o Python Original

| Aspecto | Python RLM | Pi-RLM (TypeScript) | Veredicto |
|---------|-----------|---------------------|-----------|
| **Arquitetura** | Standalone (orchestrator + REPL + LLM) | Extensão Pi Agent | Diferente |
| **Loop recursivo** | while iteration < max | Árvore com planner | Pi mais sofisticado |
| **Decisão de decompor** | Fixa (LLM decide no prompt) | Planner LLM com 3 modos | Pi mais flexível |
| **Backends LLM** | 1 (socket → LMHandler) | 3 (SDK/CLI/tmux) | Pi mais versátil |
| **Execução de código** | Python exec() | Pi Agent executa | Equivalente |
| **Prompt de sistema** | Customizável via kwargs | 3 prompts fixos | Python mais flexível |
| **Streaming** | Sim (SSE) | Apenas via tmux | Python superior |
| **Compaction** | Sim | Não | Python superior |
| **Custom tools** | Sim (exec environment) | Via toolsProfile | Diferente |
| **Async/Cancel** | Não nativo | AbortController | Pi superior |
| **Live monitoring** | Não | --live com árvore ASCII | Pi superior |
| **Testes** | pytest | Nenhum | Python superior |
| **Persistência** | JSON logs | events.jsonl + tree.json | Equivalente |
| **Run lifecycle** | Não | start/status/wait/cancel | Pi superior |

---

## O que Falta (comparado com Python)

1. **Compaction/Summarization** — Para árvores profundas com outputs grandes, o prompt do synthesizer pode exceder context windows
2. **Streaming dentro de nós** — Cada nó espera resposta completa antes de recursar
3. **Cost/token tracking** — RunStats só rastreia nós e tempo, não tokens ou custos
4. **Retry logic** — Se chamada LLM falha, nó falha sem retry
5. **Model fallback** — Se modelo especificado não está disponível, run falha
6. **Result caching** — Subtasks idênticas em branches diferentes re-executam
7. **Testes** — Zero testes unitários ou de integração
8. **Custom sub-tools** — Subcalls rodam com `--no-skills`, não herdam skills do Pi

---

## Conclusão

### O que o Pi-RLM faz bem:
1. **Integração nativa** — Como extensão Pi, é invocado naturalmente pelo agente
2. **Três backends** — SDK/CLI/tmux cobrem performance, isolamento e debugging
3. **Planner inteligente** — Decide quando decompor vs resolver, com fallbacks
4. **Live visualization** — Árvore ASCII em tempo real é inovação de UX
5. **Run lifecycle** — CRUD completo com async, timeout, cancel
6. **Ciclo detection** — Evita recursão infinita por normalização de task
7. **Zero dependencies** — Apenas peer deps do Pi SDK
8. **Graceful degradation** — Múltiplos níveis de fallback

### O que falta para ser production-ready:
1. **Testes** — Nenhum teste existe
2. **Compaction** — Sem gerenciamento de contexto para árvores profundas
3. **Retry/fallback** — Sem resiliência a falhas de API
4. **Cost tracking** — Sem visibilidade de custos
5. **Streaming** — Sem output parcial durante execução

### Veredicto:
O Pi-RLM é a implementação **mais completa do RLM** em termos de arquitetura recursiva. Enquanto o Python tem o REPL e o Rust tem o storage, o Pi-RLM tem o **engine de decisão** (planner/solver/synthesizer) que é o coração conceitual do RLM. A integração com Pi Agent como extensão é elegante — o Pi fornece a execução de código e o Pi-RLM fornece a orquestração recursiva.
