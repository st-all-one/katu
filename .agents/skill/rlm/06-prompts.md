# Engenharia de Prompts para RLM

## Visão Geral

O RLM usa uma arquitetura de prompts em camadas para guiar o LLM a agir como um orquestrador recursivo.

```
┌─────────────────────────────────────────────────┐
│               Estrutura de Prompts              │
│                                                  │
│  ┌───────────────────────────────────────────┐  │
│  │ 1. System Prompt                          │  │
│  │    - Identidade: "You are an RLM"         │  │
│  │    - Capacidades disponíveis              │  │
│  │    - Estratégias de uso                   │  │
│  │    - Regras de submissão                  │  │
│  └───────────────────────────────────────────┘  │
│                                                  │
│  ┌───────────────────────────────────────────┐  │
│  │ 2. Orchestrator Addendum (opcional)       │  │
│  │    - Instruções de delegação              │  │
│  │    - Gestão de orçamento                  │  │
│  │    - Estratégias de chunking              │  │
│  └───────────────────────────────────────────┘  │
│                                                  │
│  ┌───────────────────────────────────────────┐  │
│  │ 3. Query Metadata                         │  │
│  │    - Tipo e tamanho do contexto           │  │
│  │    - Prompt do usuário (root_prompt)      │  │
│  └───────────────────────────────────────────┘  │
│                                                  │
│  ┌───────────────────────────────────────────┐  │
│  │ 4. User Prompt (por iteração)             │  │
│  │    - "Turn {N}/{max}:"                    │  │
│  │    - Safeguard na iteração 0              │  │
│  │    - Info sobre contextos/históricos      │  │
│  └───────────────────────────────────────────┘  │
│                                                  │
│  ┌───────────────────────────────────────────┐  │
│  │ 5. Histórico de REPL Outputs              │  │
│  │    - Código executado + resultados        │  │
│  │    - stdout/stderr capturados             │  │
│  └───────────────────────────────────────────┘  │
└─────────────────────────────────────────────────┘
```

## System Prompt - Minimal

```python
REPL_SYSTEM_PROMPT = """You are tasked with answering a query with associated context. 
You can access, transform, and analyze this context interactively in a REPL environment 
that can recursively query sub-LLMs, which you are strongly encouraged to use as much 
as possible. You will be queried iteratively until you provide a final answer.

The REPL environment is initialized with:
1. A `context` variable that contains extremely important information about your query.
2. A `llm_query` function that allows you to query an LLM (that can handle around 500K chars).
3. The ability to use `print()` statements to view the output of your REPL code.

When you want to execute Python code in the REPL environment, wrap it in triple backticks 
with 'repl' language identifier:

```repl
chunk = context[:10000]
answer = llm_query(f"What is the magic number in the context? Here is the chunk: {chunk}")
print(answer)
```

IMPORTANT: When you are done, provide a final answer inside a FINAL function:
1. FINAL(your final answer here) - direct answer
2. FINAL_VAR(variable_name) - return a REPL variable

Think step by step, plan, and execute immediately.
"""
```

## System Prompt - Full (Orchestrator)

```python
RLM_SYSTEM_PROMPT = """You are a Recursive Language Model (RLM): a language model with 
a prompt, and a very important context stored in a Python REPL related to that prompt.

Available in the REPL:
- `context`: the important information related to the prompt
- `llm_query(prompt, model=None) -> str`: single sub-LLM completion. Use for extraction, 
  summarization, or Q&A over a chunk. Sub-LLM context ≈ 500K chars.
- `llm_query_batched(prompts, model=None) -> list[str]`: concurrent LLM calls.
- `rlm_query(prompt, model=None)`: recursive RLM sub-calls (deeper thinking).
- `rlm_query_batched(prompts, model=None)`: batched recursive sub-calls.
- `SHOW_VARS() -> str`: list all variables.
- `answer`: dict initialized to {"content": "", "ready": False}.

REPL outputs over ~20K characters are truncated. Use `llm_query` for longer payloads.

Plan in prose, then execute one ```repl``` block every turn.
"""
```

## Orchestrator Addendum

```python
ORCHESTRATOR_ADDENDUM = "\n\n".join([
    "As an RLM, you should act as an orchestrator, not a solver.",
    
    # Planejamento
    "Directly after you probe the `context` and understand your task, pause and plan: "
    "state explicitly how the task decomposes into sub-LLM / REPL steps, and sketch "
    "the concrete sequence of turns — what each turn computes and which sub-LLM call "
    "it issues — like a condensed trajectory, before you execute them.",
    
    # Uso eficiente de contexto
    "Your own context window is small. Push every long-context operation that would "
    "not fit comfortably in your own working window — reading, summarizing, "
    "classifying, verifying, answering sub-questions — into `llm_query` / "
    "`llm_query_batched` calls instead of pulling that text into your own message stream.",
    
    # Formato de sub-calls
    "Sub-LLMs have no REPL; they only see the prompt and the `context` slice you pass "
    "them. Hand them clean, focused inputs and ask for terse, structured outputs.",
    
    # Limites de budget
    "Sub-call budget is finite on two axes: (1) Per-prompt capacity (~100K chars), "
    "(2) Per-batch fan-out (~20 prompts). Fat-prompt small batches are correct.",
    
    # Uso de tokens
    "Reserve your own tokens for high-level decisions: what to ask next, how to combine "
    "sub-LM outputs, when to finalize. Delegate everything else.",
])
```

## User Prompt (por iteração)

```python
USER_PROMPT = "Turn {iter_1}/{max_iter}:"

def build_user_prompt(root_prompt=None, iteration=0, context_count=1, 
                      history_count=0, max_iterations=30):
    iter_1 = iteration + 1
    body = USER_PROMPT.format(iter_1=iter_1, max_iter=max_iterations)
    
    if iteration == 0:
        safeguard = (
            "You have not interacted with the REPL environment or seen your prompt / context "
            "yet. Look at the context first; do not provide a final answer yet.\n\n"
        )
        prompt = safeguard + body
    else:
        prompt = body
    
    # Info sobre múltiplos contextos
    if context_count > 1:
        prompt += (
            f"\n\nNote: You have {context_count} contexts available "
            f"(context_0 through context_{context_count - 1})."
        )
    
    # Info sobre históricos
    if history_count > 0:
        prompt += (
            f"\n\nNote: You have {history_count} prior conversation histories available "
            f"(history_0 through history_{history_count - 1})."
        )
    
    return {"role": "user", "content": prompt}
```

## Prompt de Montagem

```python
def build_rlm_system_prompt(system_prompt, query_metadata, 
                            custom_tools=None, root_prompt=None,
                            orchestrator=True):
    # 1. Formata custom tools
    tools_formatted = format_tools_for_prompt(custom_tools)
    custom_tools_section = ""
    if tools_formatted:
        custom_tools_section = f"\n6. Custom tools and data available in the REPL:\n{tools_formatted}"
    
    # 2. Combina system prompt com tools
    final_prompt = system_prompt.format(custom_tools_section=custom_tools_section)
    
    # 3. Adiciona orchestrator addendum
    if orchestrator:
        final_prompt = f"{final_prompt}\n\n{ORCHESTRATOR_ADDENDUM}"
    
    # 4. Adiciona metadata do contexto
    metadata_body = (
        f"Your context is a {query_metadata.context_type} of "
        f"{query_metadata.context_total_length} total characters. "
        "Each sub-LLM call can handle roughly ~100k tokens at once."
    )
    
    if root_prompt:
        metadata_prompt = f"Answer the following: {root_prompt}\n\n{metadata_body}"
    else:
        metadata_prompt = metadata_body
    
    return [
        {"role": "system", "content": final_prompt},
        {"role": "user", "content": metadata_prompt},
    ]
```

## Estratégias de Prompt para Casos de Uso

### Needle-in-a-Haystack

```
System: "You have a massive text. Search for a specific piece of information."

User turn 0: "Look at the context first to understand its structure."

# O LLM tipicamente faz:
```repl
print(f"Context length: {len(context)}")
print(f"First 500 chars: {context[:500]}")
print(f"Looking for 'needle'...")
needle_lines = [l for l in context.split('\n') if 'needle' in l.lower()]
print(f"Found {len(needle_lines)} matches")
```

User turn 1: "Turn 2/10:"

```repl
result = llm_query(f"Extract the needle from these lines: {needle_lines}")
print(f"Needle found: {result}")
answer["content"] = result
answer["ready"] = True
```
```

### Multi-Document QA

```
System: "You have multiple documents. Answer a question using all of them."

# Estratégia typical:
```repl
# 1. Entende a estrutura
print(f"Number of documents: {len(context)}")
print(f"Doc 0 preview: {context[0][:200]}")

# 2. Chunking inteligente
chunk_size = len(context) // 10
chunks = [context[i*chunk_size:(i+1)*chunk_size] for i in range(10)]

# 3. Análise paralela
prompts = [f"Answer this question from the document: {chunk}\nQuestion: {query}" 
           for chunk in chunks]
answers = llm_query_batched(prompts)

# 4. Agregação
summary = llm_query(f"Aggregate these answers: {answers}\nQuestion: {query}")
answer["content"] = summary
answer["ready"] = True
```
```

### Deep Analysis (Orchestrator)

```
System: (com orchestrator addendum)

User turn 0: "Analyze the performance metrics in this dataset."

# O orquestrador tipicamente:
```repl
# 1. Planejamento
print("=== ANALYSIS PLAN ===")
print("1. Explore data structure")
print("2. Identify key metrics")
print("3. Statistical analysis per metric")
print("4. Trend analysis")
print("5. Generate recommendations")
print("=====================")

# 2. Exploração inicial
print(f"Data type: {type(context)}")
if isinstance(context, list):
    print(f"Records: {len(context)}")
    print(f"Sample: {context[0]}")
```

User turn 1: "Turn 2/30:"

```repl
# 3. Análise estatística
stats = llm_query(f"Calculate stats for: {context[:50000]}")
print(f"Stats: {stats[:200]}")

# 4. Tendências
trends = llm_query(f"Identify trends in: {stats}")
print(f"Trends: {trends[:200]}")

# 5. Recomendações
recs = llm_query(f"Based on trends: {trends}, suggest actions")
answer["content"] = recs
answer["ready"] = True
```
```

## Regras de Ouro para Prompts RLM

1. **Sempre comece explorando** - Nunca assuma a estrutura do contexto
2. **Delegue operações pesadas** - Use `llm_query` para análise semântica
3. **Mantenha respostas curtas** - Outputs longos poluem o histórico
4. **Use batched quando possível** - `llm_query_batched` é mais eficiente
5. **Verifique o answer dict** - Não esqueça de submeter `answer["ready"] = True`
6. **Planeje antes de executar** - Escreva o plano em texto, execute em código
7. **Trate erros** - Sub-LLMs podem falhar; tenha fallbacks
