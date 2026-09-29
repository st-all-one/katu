# Arquitetura do RLM - Deep Dive

## Diagrama de Arquitetura Geral

```
┌──────────────────────────────────────────────────────────────────┐
│                        RLM System                                │
│                                                                  │
│  ┌────────────┐    ┌─────────────┐    ┌───────────────────────┐ │
│  │   RLM      │    │  LMHandler  │    │  Environment (REPL)   │ │
│  │ Orchestrator│◄──►│  TCP Server │◄──►│  Code Execution       │ │
│  └────────────┘    └─────────────┘    └───────────────────────┘ │
│        │                  │                     │                │
│        │                  │                     │                │
│        ▼                  ▼                     ▼                │
│  ┌────────────┐    ┌─────────────┐    ┌───────────────────────┐ │
│  │  Iteration │    │  LM Clients │    │  Namespace Globals     │ │
│  │  Loop      │    │  (OpenAI,   │    │  - context             │ │
│  │            │    │   Anthropic, │    │  - llm_query()         │ │
│  │  1. Prompt │    │   Gemini)   │    │  - rlm_query()         │ │
│  │  2. LLM    │    └─────────────┘    │  - answer{}            │ │
│  │  3. Parse  │                       │  - SHOW_VARS()         │ │
│  │  4. Exec   │                       └───────────────────────┘ │
│  │  5. Check  │                                                  │
│  └────────────┘                                                  │
└──────────────────────────────────────────────────────────────────┘
```

## Ciclo de Vida de uma Completion

```python
rlm.completion(context="...", query="...")
```

### Fase 1: Setup

```python
def _spawn_completion_context(self, prompt):
    # 1. Cria LMHandler (servidor TCP na porta auto-atribuída)
    self._lm_handler = LMHandler(clients=[openai_client])
    
    # 2. Cria Environment (LocalREPL com contexto carregado)
    self._environment = get_environment(
        environment_type="local",
        lm_handler_address=("127.0.0.1", port),
        context_payload=prompt,
    )
```

### Fase 2: Loop Iterativo

```
Para cada iteração (até max_iterations):
  │
  ├── 1. Constrói prompt do usuário
  │      "Turn {N}/{max}:"
  │      + safeguard na iteração 0
  │
  ├── 2. Envia para o LLM
  │      messages = [system_prompt, user_prompt, ...histórico]
  │      response = llm.completion(messages)
  │
  ├── 3. Extrai blocos ```repl
  │      code_blocks = find_code_blocks(response)
  │
  ├── 4. Executa código no REPL
  │      result = environment.execute_code(code)
  │      # stdout, stderr, variáveis locais
  │
  ├── 5. Adiciona resultado ao histórico
  │      messages.append({
  │          "role": "user",
  │          "content": "REPL Output:\nstdout: ...\nstderr: ..."
  │      })
  │
  └── 6. Verifica resposta final
         if answer["ready"] == True:
             return answer["content"]
```

## Comunicação: Socket Protocol

Todas as comunicações usam TCP sockets com protocolo length-prefixed JSON:

```
┌─────────────────────────────────────────────────────────┐
│ Wire Protocol                                            │
│                                                          │
│  [4 bytes: big-endian length] [N bytes: UTF-8 JSON]     │
│                                                          │
│  Exemplo:                                                │
│  \x00\x00\x00\x1a{"prompt": "hello", "model": "gpt-4"} │
└─────────────────────────────────────────────────────────┘
```

### Por que TCP sockets?

- **Consistência**: Mesmo protocolo para ambientes locais e isolados
- **Isolamento**: Código malicioso no REPL não afeta o host
- **Escalabilidade**: Suporte a batched requests via asyncio
- **Flexibilidade**: Substitui clientes facilmente

### Fluxo de uma `llm_query()` interna:

```python
# Dentro do REPL, quando o código faz:
answer = llm_query("Summarize this chunk: ...")

# O que acontece:
1. REPLEnv.globals['llm_query']("Summarize this chunk: ...")
2. → LMRequest(prompt="Summarize...", model=None, depth=0)
3. → socket_send(handler_address, request)
4. → LMHandler.handle(request)
5. → client.completion(prompt)
6. → LMResponse(chat_completion=RLMChatCompletion(...))
7. → return "Chunk summarizes to..."
```

## Recursão: `rlm_query()` vs `llm_query()`

### `llm_query()` - Chamada simples

```
Parent RLM
    │
    └──► llm_query("Analyze chunk X")
         │
         └──► Sub-LLM (1 chamada, sem REPL)
              │
              └──► Resposta string
```

### `rlm_query()` - Chamada recursiva

```
Parent RLM (depth=0)
    │
    └──► rlm_query("Solve this complex sub-problem: ...")
         │
         └──► Child RLM (depth=1)
              ├── LMHandler próprio
              ├── Environment próprio
              ├── Loop iterativo próprio
              ├── Pode chamar rlm_query() novamente
              │
              └──► Resposta string (após N iterações)
```

### Propagação de Limites na Recursão

```python
def _subcall(self, prompt, model):
    # Limites restantes = limite_total - usado
    remaining_timeout = self._max_timeout - elapsed
    remaining_budget = self._max_budget - spent
    remaining_tokens = self._max_tokens - tokens_used
    remaining_errors = self._max_errors - errors
    
    child = RLM(
        timeout=remaining_timeout,
        budget=remaining_budget,
        max_tokens=remaining_tokens,
        max_errors=remaining_errors,
        depth=self.depth + 1,  # incrementa
    )
    return child.completion(prompt)
```

## Gerenciamento de Contexto

### Compaction (Compactação)

Quando o histórico de mensagens se aproxima do limite de contexto:

```python
def _compact_history(self, messages, model):
    context_limit = get_context_limit(model)
    current_tokens = count_tokens(messages, model)
    
    if current_tokens > context_limit * 0.85:  # 85% do limite
        # 1. Pega as últimas N mensagens
        # 2. Envia para o LLM: "Resuma esta conversa em 2-3 parágrafos"
        # 3. Substitui o histórico antigo pelo resumo
        # 4. Mantém as últimas 2-3 mensagens originais
```

### Suporte a Múltiplos Contextos

```python
# A variável context pode ser:
context = "string simples"           # → context = "..."
context = ["doc1", "doc2", "doc3"]   # → context_0, context_1, context_2
context = {"key": "value"}           # → context["key"]
```

## Ciclo de Vida por Completion

```
completion() chamado
    │
    ├── LMHandler criado (porta TCP auto-atribuída)
    ├── Environment criado (namespace fresh)
    ├── Logger registrado
    │
    ├── Loop de iterações...
    │   │
    │   ├── [Cada iteração pode gerar sub-calls]
    │   └── [Sub-calls criam child RLMs recursivamente]
    │
    ├── answer["ready"] = True → retorna
    │
    ├── Cleanup:
    │   ├── Environment.cleanup()
    │   ├── LMHandler.shutdown()
    │   └── Logger.clear_iterations()
    │
    └── Retorna RLMChatCompletion
```

## Namespace do REPL

O ambiente fornece estas variáveis globais ao código executado:

```python
# Dados
context          # O contexto principal (str | list[str] | dict)
context_0, ...   # Múltiplos contextos (quando aplicável)
history          # Histórico de conversas anteriores

# Funções de chamada
llm_query(prompt, model=None)           # Chamada simples ao sub-LLM
llm_query_batched(prompts, model=None)  # Chamadas paralelas
rlm_query(prompt, model=None)           # Chamada recursiva (child RLM)
rlm_query_batched(prompts, model=None)  # Recursivas paralelas

# Controle
answer = {"content": "", "ready": False}  # Submissão de resposta
SHOW_VARS()                                # Lista variáveis disponíveis

# Ferramentas customizadas (opcional)
custom_tool_1, custom_tool_2, ...
```

## Segurança

### Builtins Seguros (`_SAFE_BUILTINS`)

O REPL restringe builtins perigosos:

```python
_SAFE_BUILTINS = {
    # ✅ Permitidos
    'print': print, 'len': len, 'str': str, 'int': int,
    'range': range, 'enumerate': enumerate, 'zip': zip,
    'sorted': sorted, 'map': map, 'filter': filter,
    'open': open, '__import__': __import__,
    
    # ❌ Bloqueados
    'input': None,    # Sem input interativo
    'eval': None,     # Sem eval arbitrária
    'exec': None,     # Sem exec aninhado
    'compile': None,  # Sem compilação
    'globals': None,  # Sem acesso ao global scope
    'locals': None,   # Sem acesso ao local scope
}
```

### Restauração de Scaffold

Após cada `exec()`, nomes reservados são restaurados:

```python
# Após cada execução:
reserved_names = ['llm_query', 'llm_query_batched', 
                  'rlm_query', 'rlm_query_batched',
                  'answer', 'SHOW_VARS', 'context']

for name in reserved_names:
    if name not in combined_namespace:
        combined_namespace[name] = original_globals[name]
```

Isso impede que o código malicioso substitua funções críticas.

## Metadados de Execução

Cada completion gera metadados completos:

```python
RLMMetadata(
    root_model="gpt-4o",
    max_depth=3,
    max_iterations=30,
    backend="openai",
    environment_type="local",
    orchestrator=True,
    # ...
)

RLMChatCompletion(
    root_model="gpt-4o",
    prompt=[...messages...],
    response="A resposta é 42",
    usage=UsageSummary(total_calls=15, ...),
    execution_time=12.5,
    iterations=[RLMIteration(...), ...],
)
```
