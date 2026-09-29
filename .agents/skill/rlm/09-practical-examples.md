# Exemplos Práticos de RLM

## Exemplo 1: Needle-in-a-Haystack (Básico)

**Problema**: Encontrar uma informação específica em 1M+ de linhas de texto.

```python
# main.py
from rlm.rlm_repl import RLM_REPL
import random

def generate_massive_context(num_lines=1_000_000, answer="1298418"):
    """Gera contexto massivo com resposta escondida."""
    random_words = ["blah", "random", "text", "data", "content"]
    lines = []
    for _ in range(num_lines):
        num_words = random.randint(3, 8)
        line_words = [random.choice(random_words) for _ in range(num_words)]
        lines.append(" ".join(line_words))
    
    # Insere resposta em posição aleatória
    magic_position = random.randint(400000, 600000)
    lines[magic_position] = f"The magic number is {answer}"
    return "\n".join(lines)

def main():
    answer = str(random.randint(1000000, 9999999))
    context = generate_massive_context(answer=answer)
    
    rlm = RLM_REPL(
        model="gpt-5",
        recursive_model="gpt-5-nano",
        max_iterations=10,
    )
    
    result = rlm.completion(
        context=context,
        query="I'm looking for a magic number. What is it?"
    )
    
    print(f"Result: {result}")
    print(f"Expected: {answer}")
    print(f"Correct: {answer in result}")

if __name__ == "__main__":
    main()
```

**O que o LLM tipicamente faz:**

```
Iteração 1:
  "Vou procurar por 'magic number' no contexto"
  → lines = [l for l in context.split('\n') if 'magic' in l]
  → print(f"Found: {lines}")

Iteração 2:
  → answer = llm_query(f"What is the number? {lines[0]}")
  → print(f"Number: {answer}")

Iteração 3:
  → FINAL(1298418)
```

---

## Exemplo 2: Multi-Document QA

**Problema**: Analisar múltiplos documentos para responder uma pergunta.

```python
from rlm import RLM

# Simula 10 documentos
documents = [
    {"title": f"Doc {i}", "content": f"Conteúdo do documento {i}..." * 1000}
    for i in range(10)
]

rlm = RLM(
    backend="openai",
    max_iterations=30,
    orchestrator=True,
)

result = rlm.completion(
    prompt=documents,
    root_prompt="Quais são os 3 temas mais recorrentes em todos os documentos?"
)

print(result)
```

**Estratégia típica do LLM:**

```python
# Turn 1: Exploração
print(f"Number of documents: {len(context)}")
print(f"Doc 0: {context[0]['title']}, length: {len(context[0]['content'])}")

# Turn 2: Análise paralela
prompts = [
    f"List the top 3 themes in this document:\n{doc['content'][:10000]}"
    for doc in context
]
themes_per_doc = llm_query_batched(prompts)

# Turn 3: Agregação
all_themes = "\n".join([f"Doc {i}: {t}" for i, t in enumerate(themes_per_doc)])
summary = llm_query(f"Given these themes per document, what are the top 3 overall:\n{all_themes}")

# Turn 4: Resposta
answer["content"] = summary
answer["ready"] = True
```

---

## Exemplo 3: Análise de Logs

**Problema**: Encontrar padrões de erro em logs massivos.

```python
from rlm import RLM

# Logs de aplicação (pode ser GBs)
with open("/var/log/app.log") as f:
    logs = f.read()  # ou carrega em chunks

rlm = RLM(
    backend="openai",
    max_iterations=40,
    environment="local",
)

result = rlm.completion(
    prompt=logs,
    root_prompt="Quais são os 5 erros mais frequentes? Forneça: erro, contagem, e sugestão de fix."
)

print(result)
```

**Código típico gerado:**

```python
# Turn 1: Entender estrutura
print(f"Log size: {len(context)} chars")
print(f"Lines: {context.count(chr(10))}")
print(f"First 500 chars:\n{context[:500]}")

# Turn 2: Extrair erros
error_lines = [l for l in context.split('\n') if 'ERROR' in l or 'Exception' in l]
print(f"Found {len(error_lines)} error lines")

# Turn 3: Contar erros por tipo
import re
error_types = {}
for line in error_lines:
    match = re.search(r'(Error|Exception):?\s*(\w+)', line)
    if match:
        err_type = match.group(2)
        error_types[err_type] = error_types.get(err_type, 0) + 1

print(f"Error types: {error_types}")

# Turn 4: Análise semântica
top_errors = sorted(error_types.items(), key=lambda x: -x[1])[:5]
analysis = llm_query(
    f"Analyze these top errors and suggest fixes:\n{top_errors}\n"
    f"Sample lines:\n{error_lines[:10]}"
)

# Turn 5: Resposta
answer["content"] = analysis
answer["ready"] = True
```

---

## Exemplo 4: RAG Alternativo (sem embeddings)

**Problema**: Responder perguntas sobre uma base de conhecimento usando RLM como alternativa ao RAG tradicional.

```python
from rlm import RLM

# Base de conhecimento (sem embedding necessário)
knowledge_base = """
# Guia de Produto X
## Instalação
Para instalar o Produto X, siga estes passos...

## Configuração
As configurações principais são...

## Solução de Problemas
Se o produto não inicia, verifique...

# FAQ
## P: Como resetar a senha?
## R: Acesse /settings/reset...

# Políticas
## Reembolso
O cliente pode solicitar reembolso em até 30 dias...
""".strip()

rlm = RLM(orchestrator=True, max_iterations=25)

# Pergunta sobre a base
result = rlm.completion(
    prompt=knowledge_base,
    root_prompt="Como faço para resetar minha senha e qual a política de reembolso?"
)

print(result)
```

---

## Exemplo 5: Código com Custom Tools

**Problema**: Analisar dados usando tools customizadas.

```python
import sqlite3
from rlm import RLM

# Setup: banco de dados de exemplo
conn = sqlite3.connect(":memory:")
conn.execute("""
    CREATE TABLE sales (
        id INTEGER PRIMARY KEY,
        product TEXT,
        amount REAL,
        date TEXT
    )
""")

# Insere dados
for i in range(1000):
    conn.execute(
        "INSERT INTO sales VALUES (?, ?, ?, ?)",
        (i, f"Product {i % 10}", i * 10.5, f"2024-{(i % 12)+1:02d}-{(i % 28)+1:02d}")
    )
conn.commit()

# Tools customizadas
def query_db(sql: str) -> str:
    """Executa uma query SQL no banco de vendas."""
    try:
        cursor = conn.execute(sql)
        results = cursor.fetchall()
        return str(results)
    except Exception as e:
        return f"SQL Error: {e}"

def get_table_schema() -> str:
    """Retorna o schema das tabelas."""
    cursor = conn.execute("SELECT sql FROM sqlite_master WHERE type='table'")
    return "\n".join(row[0] for row in cursor.fetchall())

# RLM com tools
rlm = RLM(
    custom_tools={
        "query_db": query_db,
        "get_table_schema": get_table_schema,
    },
    orchestrator=True,
)

result = rlm.completion(
    prompt="Base de dados de vendas disponível via tools customizadas.",
    root_prompt="Quais foram os top 5 produtos por receita total em 2024?"
)

print(result)
```

**Código gerado:**

```python
# Turn 1: Explorar schema
schema = get_table_schema()
print(f"Schema:\n{schema}")

# Turn 2: Query SQL
top_products = query_db("""
    SELECT product, SUM(amount) as total_revenue
    FROM sales
    WHERE date LIKE '2024%'
    GROUP BY product
    ORDER BY total_revenue DESC
    LIMIT 5
""")
print(f"Top products:\n{top_products}")

# Turn 3: Análise
analysis = llm_query(f"Analyze these sales results:\n{top_products}")
answer["content"] = analysis
answer["ready"] = True
```

---

## Exemplo 6: Treinamento (Dataset OOLONG)

```python
# training/environments/oolong/run_training.py
from rlm_train.env import RLMTrainEnv
from rlm_train.rubric import RLMTrainRubric
from datasets import load_dataset

# Carrega dataset
dataset = load_dataset("oolongbench/oolong-synth", split="train")

# Função de scoring
def synth_score(prediction, expected):
    if expected["type"] == "numeric":
        try:
            return 1.0 if abs(float(prediction) - float(expected["value"])) < 0.01 else 0.0
        except:
            return 0.0
    return 1.0 if prediction.strip() == expected["value"].strip() else 0.0

# Cria environment
env = RLMTrainEnv(
    dataset=dataset,
    correctness_fn=synth_score,
    max_iterations=20,
    orchestrator=True,
)

# Cria rubric
rubric = RLMTrainRubric(
    correctness_fn=synth_score,
    min_iterations=2,
    min_subcall=1,
)

# Inicia treinamento (via prime-rl CLI)
# prime-rl train --config configs/rlm-qwen3-30b-example.toml
```

---

## Exemplo 7: Multi-Turn com Persistência

```python
from rlm import RLM

rlm = RLM(persistent=True, max_iterations=15)

# Sessão de análise de código
with rlm:
    # Carrega repositório
    rlm.load_context(open("src/main.py").read())
    
    # Turn 1: Entender o código
    r1 = rlm.completion("Qual é a estrutura geral deste código?")
    
    # Turn 2: Encontrar bugs (acessa variáveis do turn 1)
    r2 = rlm.completion("Existem bugs de concorrência?")
    
    # Turn 3: Sugerir melhorias
    r3 = rlm.completion("Sugira refatorações específicas com código")
    
    # Turn 4: Gerar testes
    r4 = rlm.completion("Gere testes unitários para as funções críticas")
```

---

## Exemplo 8: Docker Environment

```python
from rlm import RLM

# Código não confiável → usa Docker
rlm = RLM(
    environment="docker",
    environment_kwargs={
        "image": "python:3.12-slim",
        "timeout": 120,
    },
    max_iterations=20,
)

# Código do usuário (pode ser malicioso)
user_code = """
import os
# Código potencialmente perigoso
"""

# Executa de forma segura no Docker
result = rlm.completion(
    prompt=user_code,
    root_prompt="Analise e execute este código de forma segura"
)
```

---

## Resumo de Casos de Uso

| Caso de Uso | Contexto | Estratégia Principal |
|-------------|----------|---------------------|
| Needle-in-Haystack | Texto massivo | Busca programática + sub-LLM |
| Multi-Document QA | Lista de docs | Paralelizar + agregar |
| Análise de Logs | Logs gigantes | Regex + contagem + sub-LLM |
| RAG Alternativo | Base conhecimento | Busca semântica via sub-LLM |
| Análise de Código | Repositório | Explorar + analisar + tooling |
| Treinamento RL | Dataset QA | Rollout + scoring + update |
| Multi-Turn | Conversa longa | Persistência de variáveis |
| Código Perigoso | Não confiável | Docker sandbox |
