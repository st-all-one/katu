# Padrões Avançados e Tools Customizadas

## Custom Tools

O RLM suporta injeção de tools customizadas que ficam disponíveis no namespace do REPL.

### Definindo Tools

```python
# Tools são funções Python normais
def search_database(query: str) -> str:
    """Busca no banco de dados interno."""
    results = db.execute(f"SELECT * FROM items WHERE name LIKE '%{query}%'")
    return json.dumps(results)

def get_weather(city: str) -> str:
    """Obtém clima atual de uma cidade."""
    return api.weather(city)

def calculate(expression: str) -> str:
    """Calcula uma expressão matemática de forma segura."""
    import ast
    try:
        tree = ast.parse(expression, mode='eval')
        return str(eval(compile(tree, '<expr>', 'eval')))
    except:
        return "Error: Invalid expression"

# Dicionário de tools
tools = {
    "search_database": search_database,
    "get_weather": get_weather,
    "calculate": calculate,
}
```

### Registrando Tools

```python
from rlm import RLM

rlm = RLM(
    backend="openai",
    custom_tools=tools,
)

# Tools ficam disponíveis no REPL:
# result = search_database("laptops")
# weather = get_weather("São Paulo")
# answer = calculate("2**10")
```

### Como Funciona Internamente

```python
# rlm/environments/base_env.py

RESERVED_TOOL_NAMES = {
    'context', 'llm_query', 'llm_query_batched',
    'rlm_query', 'rlm_query_batched',
    'answer', 'SHOW_VARS',
}

def parse_custom_tools(tools: dict) -> dict[str, ToolInfo]:
    """Parseia tools customizadas."""
    parsed = {}
    for name, value in tools.items():
        if name in RESERVED_TOOL_NAMES:
            raise ValueError(f"Tool name '{name}' is reserved")
        
        if callable(value):
            # Função: extrai docstring e assinatura
            parsed[name] = ToolInfo(
                name=name,
                value=value,
                type="function",
                docstring=value.__doc__,
                signature=str(inspect.signature(value)),
            )
        else:
            # Valor: serializa como JSON
            parsed[name] = ToolInfo(
                name=name,
                value=value,
                type="value",
                docstring=f"Constant value: {value}",
            )
    
    return parsed

def format_tools_for_prompt(tools: dict[str, ToolInfo]) -> str:
    """Formata tools para inclusão no system prompt."""
    if not tools:
        return ""
    
    lines = ["Custom tools available in the REPL:"]
    for name, info in tools.items():
        if info.type == "function":
            lines.append(f"- `{name}{info.signature}`: {info.docstring or 'No docstring'}")
        else:
            lines.append(f"- `{name}`: {info.docstring}")
    
    return "\n".join(lines)
```

### Tools: Valores vs Funções

```python
# Valores (constantes injetadas)
tools = {
    "MAX_RETRIES": 3,
    "API_ENDPOINT": "https://api.example.com",
    "CONFIG": {"timeout": 30, "retries": 3},
}

# No REPL:
# print(API_ENDPOINT)  # https://api.example.com
# print(CONFIG["timeout"])  # 30

# Funções (chamáveis)
tools = {
    "fetch_data": lambda url: requests.get(url).json(),
    "process": process_data,
}

# No REPL:
# data = fetch_data("https://api.example.com/data")
# result = process(data)
```

## Modo Orchestrator

O modo orchestrator transforma o RLM em um delegador que coordena sub-tarefas.

### Ativando

```python
rlm = RLM(orchestrator=True)
```

### Comportamento

Com `orchestrator=True`, o system prompt inclui o `ORCHESTRATOR_ADDENDUM` que instrui o LLM a:

1. **Planejar antes de executar** - Descompor em sub-tarefas
2. **Delegar trabalho pesado** - Usar `llm_query` para tudo que não cabe no contexto
3. **Manter respostas curtas** - Não poluir o histórico com stdout longo
4. **Verificar antes de submeter** - Imprimir candidate answer antes de `answer["ready"]`

### Exemplo de Output do Orchestrator

```repl
# Turn 1: Planejamento
print("=== PLAN ===")
print("1. Explore context structure (50K chars)")
print("2. Chunk by sections (~10 chunks)")
print("3. Parallel analyze with llm_query_batched")
print("4. Aggregate and verify")
print("============")
print(f"Context type: {type(context)}")
print(f"Length: {len(context)} chars")
print(f"Preview: {context[:500]}")
```

```repl
# Turn 2: Execução
chunk_size = len(context) // 10
chunks = [context[i*chunk_size:(i+1)*chunk_size] for i in range(10)]
prompts = [f"Analyze this section for key insights:\n{c}" for c in chunks]
results = llm_query_batched(prompts)
for i, r in enumerate(results):
    print(f"Chunk {i}: {r[:100]}...")
```

```repl
# Turn 3: Agregação
summary = llm_query(f"Aggregate these insights into a coherent answer:\n{results}")
print(f"Draft answer: {summary[:200]}...")
answer["content"] = summary
answer["ready"] = True
```

## Persistência Multi-Turn

```python
# Com environment persistente
rlm = RLM(persistent=True)

with rlm:
    # Carrega contexto uma vez
    rlm.load_context("Documentação da API v2.0")
    
    # Turn 1
    r1 = rlm.completion("Quais são os endpoints principais?")
    # Variáveis do REPL persistem
    
    # Turn 2 - pode acessar variáveis do turn anterior
    r2 = rlm.completion("Como autenticar nesses endpoints?")
    # O REPL já tem as variáveis do turn 1
    
    # Turn 3
    r3 = rlm.completion("Crie um exemplo de código para o /users")
```

### Variáveis Versionadas

```python
# Múltiplos contextos
rlm.load_context("Doc API v1")  # → context_0
rlm.load_context("Doc API v2")  # → context_1

# No REPL:
# print(context_0)  # Doc API v1
# print(context_1)  # Doc API v2

# Múltiplos históricos (multi-turn)
# history_0, history_1, ... (conversas anteriores)
```

## Compaction (Compactação Automática)

```python
rlm = RLM(
    compaction=True,           # Habilita compactação
    max_tokens=1_000_000,      # Limite de tokens
)

# Quando o histórico se aproxima do limite:
# 1. Resume o histórico antigo em 2-3 parágrafos
# 2. Mantém as últimas 4 mensagens originais
# 3. Continua a conversa normalmente
```

### Como Funciona

```python
def _compact_history(self, messages, model):
    limit = get_context_limit(model)
    current = count_tokens(messages, model)
    
    if current > limit * 0.85:
        # 1. Separa mensagens antigas e recentes
        recent = messages[-4:]
        old = messages[:-4]
        
        # 2. Resume
        summary = llm.completion([
            {"role": "system", "content": "Summarize this conversation concisely."},
            {"role": "user", "content": str(old)}
        ])
        
        # 3. Substitui
        return [
            {"role": "system", "content": f"Previous summary:\n{summary}"},
            *recent
        ]
    
    return messages
```

## Sub-Calls Recursivos (Depth > 1)

```python
rlm = RLM(max_depth=3)

# depth=0: Root LLM
#   └── depth=1: Child RLM (próprio REPL)
#       └── depth=2: Grandchild RLM (próprio REPL)
#           └── depth=3: falls back to plain llm_query
```

### Propagação de Limites

```python
# Child herda limites restantes do parent
child_rlm = RLM(
    timeout=parent_remaining_timeout,
    budget=parent_remaining_budget,
    tokens=parent_remaining_tokens,
    errors=parent_remaining_errors,
    depth=parent_depth + 1,
)
```

### Modelo Diferente por Depth

```python
rlm = RLM(
    backend="openai",  # Root: GPT-4o
    other_backends=[
        {"backend": "anthropic", "model_name": "claude-3-haiku"},  # depth=1
    ],
)

# depth=0: GPT-4o (orquestrador inteligente)
# depth=1: Claude 3 Haiku (analisador rápido)
# depth=2+: fallback para plain llm_query
```

## Callbacks

```python
# Assinaturas REAIS do código fonte:
callbacks = {
    # depth: int, iteration_num: int
    "on_iteration_start": lambda depth, iteration_num: print(f"[depth={depth}] Iter {iteration_num} start"),
    
    # depth: int, iteration_num: int, duration: float
    "on_iteration_complete": lambda depth, iteration_num, duration: print(f"[depth={depth}] Iter {iteration_num} done in {duration:.1f}s"),
    
    # depth: int, model: str, prompt_preview: str
    "on_subcall_start": lambda depth, model, preview: print(f"[depth={depth}] Subcall to {model}: {preview[:80]}..."),
    
    # depth: int, model: str, duration: float, error: str | None
    "on_subcall_complete": lambda depth, model, duration, err: print(f"[depth={depth}] Subcall done: {err or 'ok'}"),
}

rlm = RLM(callbacks=callbacks)
```

**NOTA:** As assinaturas usam `depth` e `iteration_num` como integers, não objetos RLM.

## Sampling Args

```python
rlm = RLM(
    sampling_args={
        "temperature": 0.7,
        "top_p": 0.9,
        "max_tokens": 4096,
        "seed": 42,
    }
)

# Para child RLMs com args diferentes
rlm = RLM(
    sampling_args={"temperature": 0.3},  # Root: mais determinístico
    other_backends=[
        {"sampling_args": {"temperature": 0.7}},  # Child: mais criativo
    ],
)
```

## Logger Customizado

```python
from rlm.logger.rlm_logger import RLMLogger

# Logger para JSONL no disco
logger = RLMLogger(
    log_dir="/tmp/rlm_logs",     # None = só memória
    file_name="rlm",             # Nome base do arquivo
)

rlm = RLM(logger=logger)

# Após completion:
trajectory = logger.get_trajectory()
# {
#     "run_metadata": {...},
#     "iterations": [
#         {"prompt": [...], "response": "...", "code_blocks": [...]},
#         ...
#     ]
# }
```

**Formato do arquivo JSONL:**
```
rlm_2024-01-15_10-30-00_a1b2c3d4.jsonl
```

**VerbosePrinter (output Rich com tema Tokyo Night):**
```python
from rlm.logger.verbose import VerbosePrinter

printer = VerbosePrinter(enabled=True)
# Cores: primary=#7AA2F7, success=#9ECE6A, error=#F7768E
# Métodos: print_metadata(), print_iteration(), print_subcall(),
#          print_compaction(), print_final_answer(), print_summary()
```

## Parâmetros Adicionais

### custom_sub_tools

```python
rlm = RLM(
    custom_tools={...},          # Tools para o root RLM
    custom_sub_tools={...},      # Tools separadas para child RLMs
    # Se None: children herdam parent tools
    # Se {}: children NÃO têm tools
)
```

### compaction_threshold_pct

```python
rlm = RLM(
    compaction=True,
    compaction_threshold_pct=0.85,  # 85% do contexto = compacta
    # 0.7 = compacta mais cedo
    # 0.9 = compacta mais tarde
)
```

### setup_code

```python
rlm = RLM(
    environment="local",
    environment_kwargs={
        "setup_code": "import numpy as np\nimport pandas as pd",
        # Roda ANTES de cada completion
    },
)
```
