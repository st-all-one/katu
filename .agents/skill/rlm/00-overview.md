# O que é RLM (Recursive Language Model)?

## Conceito Central

RLM é um paradigma de inferência que permite a um LLM processar contextos de tamanho **quase infinito** recursivamente. A ideia principal é simples:

> Substitua `llm.completion(prompt, model)` por `rlm.completion(prompt, model)`

O RLM age como um "language model" que se auto-chama recursivamente para decomposar e resolver problemas sobre contextos massivos.

## Problema que Resolve

LLMs têm janelas de contexto limitadas (ex: GPT-4o = 128K tokens, Claude = 200K tokens). Quando você precisa analisar:

- Documentos de 1M+ de caracteres
- Bases de conhecimento inteiras
- Logs massivos de aplicação
- Dados de múltiplas fontes simultaneamente

Um LLM simples **não cabe** tudo no contexto. RLM resolve isso programaticamente.

## Como Funciona (Visão de Alto Nível)

```
┌─────────────────────────────────────────────────┐
│  Usuario: "Analise estes 10GB de logs"          │
└──────────────────────┬──────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────┐
│  RLM (Root LLM - orquestrador)                  │
│  "Preciso dividir isso em pedaços..."           │
│                                                 │
│  1. Escreve Python para chunkar o contexto      │
│  2. Chama llm_query() por chunk (sub-LLM)       │
│  3. Agrega resultados em buffers                 │
│  4. Chama llm_query() com todos os buffers       │
│  5. Retorna resposta final                       │
└─────────────────────────────────────────────────┘
```

## A Arquitetura em 3 Camadas

### 1. Root LLM (Orquestrador)
- Modelo principal (ex: GPT-4o, Claude)
- Interage iterativamente com o REPL
- Escreve código Python para manipular o contexto
- Usa `llm_query()` para delegar análise semântica

### 2. REPL Environment (Ambiente de Execução)
- Sandbox Python onde o código do Root LLM é executado
- Fornece variável `context` com os dados
- Disponibiliza `llm_query()` e `rlm_query()` como funções globais
- Mantém estado entre iterações (variáveis persistem)

### 3. Sub-LLM (Analista)
- Modelo mais barato/rápido (ex: GPT-4o-mini)
- Processa chunks do contexto
- Executa tarefas de extração, sumarização, classificação
- Contexto de ~500K chars por chamada

## Fluxo de Execução (Exemplo: Needle-in-a-Haystack)

```python
from rlm.rlm_repl import RLM_REPL

# Contexto: 1 milhão de linhas de texto aleatório
# Resposta escondida: "The magic number is 1298418"

rlm = RLM_REPL(
    model="gpt-4o",           # Root LLM
    recursive_model="gpt-4o-mini",  # Sub-LLM
    max_iterations=10
)

result = rlm.completion(
    context=milhao_de_linhas, 
    query="I'm looking for a magic number. What is it?"
)
# Resultado: "1298418"
```

**O que acontece por baixo dos panos:**

```
Iteração 1:
  Root LLM: "Vou procurar por 'magic number' no contexto"
  → Executa: lines = [l for l in context.split('\n') if 'magic' in l]
  → Resultado: ["The magic number is 1298418"]

Iteração 2:
  Root LLM: "Encontrei! Vou confirmar com o sub-LLM"
  → Executa: answer = llm_query("Qual é o número? The magic number is 1298418")
  → Resposta: "1298418"

Iteração 3:
  Root LLM: "FINAL(1298418)"
  → Retorno final
```

## Diferenças para Abordagens Tradicionais

| Abordagem | Limitação | RLM |
|-----------|-----------|-----|
| Prompt direto | Contexto > janela de contexto = falha | Divide e conquista recursivamente |
| RAG (Retrieval) | Requer embedding + indexação prévia | Não precisa - busca programática |
| Map-Reduce | Fixo, sem adaptação | Dinâmico - o LLM decide a estratégia |
| Agent + Tools | Tools são estáticas | O REPL é um canvas infinito |

## Dois Implementações Disponíveis

### Minimal (~360 linhas)
- Apenas OpenAI como backend
- `Sub_RLM` simples (depth=1 fixo)
- Ideal para aprender e prototipar

### Full (~5000+ linhas)
- Múltiplos backends (OpenAI, Anthropic, Gemini, Azure)
- Múltiplos ambientes (local, Docker, IPython, Modal, etc.)
- Recursão real (depth ilimitado)
- Treinamento com RL (verifiers + prime-rl)
- Produção-ready

## Leitura dos Guias

| Guia | Conteúdo |
|------|----------|
| `01-architecture.md` | Arquitetura detalhada do sistema |
| `02-minimal-implementation.md` | Implementação do zero (minimal) |
| `03-full-implementation.md` | Implementação completa (produção) |
| `04-environments.md` | Ambientes REPL (local, Docker, IPython...) |
| `05-clients.md` | Backends de LLM (OpenAI, Anthropic, Gemini...) |
| `06-prompts.md` | Engenharia de prompts para RLM |
| `07-training.md` | Treinamento com Reinforcement Learning |
| `08-advanced-patterns.md` | Padrões avançados e tools customizadas |
| `09-practical-examples.md` | Exemplos práticos de uso |
