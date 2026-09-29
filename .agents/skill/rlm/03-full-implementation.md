# Implementação Full do RLM - Guia de Produção

A implementação completa (`rlm-full-original/`) é uma versão production-ready com ~5000+ linhas de código.

## Estrutura do Projeto

```
rlm-full-original/
├── rlm/
│   ├── __init__.py              # API pública: RLM + exceções
│   ├── clients/                 # Backends de LLM
│   │   ├── base_lm.py           # ABC para todos os clientes
│   │   ├── openai.py            # OpenAI, vLLM, OpenRouter, Vercel
│   │   ├── anthropic.py         # Anthropic
│   │   ├── gemini.py            # Google Gemini
│   │   ├── azure_openai.py      # Azure OpenAI
│   │   └── portkey.py           # Portkey router
│   ├── core/
│   │   ├── types.py             # Dataclasses compartilhadas
│   │   ├── rlm.py               # Orquestrador principal (914 linhas)
│   │   ├── lm_handler.py        # Servidor TCP para LM calls
│   │   └── comms_utils.py       # Protocolo socket
│   ├── environments/
│   │   ├── base_env.py          # ABCs: BaseEnv, NonIsolated, Isolated
│   │   ├── local_repl.py        # REPL local (exec)
│   │   ├── docker_repl.py       # Docker container
│   │   ├── ipython_repl.py      # IPython kernel
│   │   ├── modal_repl.py        # Modal cloud sandbox
│   │   ├── prime_repl.py        # Prime Intellect sandbox
│   │   ├── daytona_repl.py      # Daytona cloud sandbox
│   │   └── e2b_repl.py          # E2B cloud sandbox
│   ├── logger/
│   │   ├── rlm_logger.py        # Captura de trajetória
│   │   └── verbose.py           # Output Rich (Tokyo Night theme)
│   └── utils/
│       ├── exceptions.py        # 5 exceções customizadas
│       ├── parsing.py           # Extração de code blocks
│       ├── prompts.py           # System prompts + orchestrator
│       ├── token_utils.py       # Contagem de tokens + limits
│       └── rlm_utils.py         # Filtragem de chaves sensíveis
├── training/                    # Treinamento com RL
├── tests/                       # Suite de testes
├── docs/                        # Documentação Next.js
└── examples/                    # Exemplos de uso
```

## Diferenças-Chave vs Minimal

| Feature | Minimal | Full |
|---------|---------|------|
| Backends | Apenas OpenAI | OpenAI, Anthropic, Gemini, Azure, Portkey |
| Ambientes | Local exec apenas | Local, Docker, IPython, Modal, Prime, Daytona, E2B |
| Recursão | Depth=1 fixo | Depth ilimitado (configurável) |
| `llm_query` | Simples | Simples + batched |
| `rlm_query` | Não existe | Recursivo com child RLMs |
| Limites | Iterações apenas | Budget, timeout, tokens, erros |
| Compaction | Não | Sim (resumo automático de histórico) |
| Custom Tools | Não | Sim (funções injetáveis) |
| Persistent Env | Não | Sim (multi-turn sessions) |
| Logger | ANSI simples | Rich com JSONL |
| Training | Não | Verifiers + prime-rl |
| Orchestrator | Não | Modo orquestrador |

## O Orquestrador Principal (`core/rlm.py`)

### Construtor (25+ parâmetros)

```python
rlm = RLM(
    # Backend
    backend="openai",           # openai|anthropic|gemini|azure_openai|portkey
    backend_kwargs={},          # kwargs para o cliente LLM
    
    # Environment
    environment="local",        # local|docker|ipython|modal|prime|daytona|e2b
    environment_kwargs={},      # kwargs para o environment
    
    # Limites
    max_depth=3,                # Profundidade máxima de recursão
    max_iterations=30,          # Iterações máximas por completion
    max_budget=10.0,            # Budget máximo em USD
    max_timeout=300,            # Timeout em segundos
    max_tokens=1_000_000,       # Limite de tokens
    max_errors=5,               # Erros máximos antes de abortar
    
    # Prompt
    custom_system_prompt=None,  # System prompt customizado
    orchestrator=True,          # Modo orquestrador (delegação)
    user_prologue=None,         # Mensagem inicial do usuário
    
    # Recursão
    other_backends=None,        # Backends para child RLMs
    max_concurrent_subcalls=5,  # Sub-calls paralelos máximos
    
    # Outros
    logger=None,                # Logger customizado
    verbose=False,              # Output detalhado
    persistent=False,           # Environment persistente
    compaction=True,            # Auto-compactação
    custom_tools=None,          # Tools customizadas
    sampling_args=None,         # temperature, top_p, etc.
    callbacks=None,             # Callbacks de ciclo de vida
)
```

### A Completion Loop (simplificada)

```python
def completion(self, prompt, root_prompt=None):
    with self._spawn_completion_context(prompt) as (handler, env):
        
        iteration = 0
        while iteration < self._max_iterations:
            # 1. Constrói prompt
            messages = self._build_messages(
                env, handler, root_prompt, iteration
            )
            
            # 2. Chama o LLM
            response = handler.completion(messages)
            
            # 3. Extrai code blocks ```repl
            code_blocks = find_code_blocks(response)
            
            if code_blocks:
                # 4. Executa cada bloco
                for code in code_blocks:
                    result = env.execute_code(code)
                    
                    # 5. Verifica sub-calls (llm_query, rlm_query)
                    # (já executadas pelo environment)
                    
                    # 6. Verifica answer["ready"]
                    if result.final_answer:
                        return self._build_completion(
                            messages, response, result, iteration
                        )
                
                # 7. Adiciona resultado ao histórico
                self._append_repl_result(env, result, messages)
            else:
                messages.append({"role": "assistant", "content": response})
            
            # 8. Verifica limites
            self._check_timeout()
            self._check_iteration_limits()
            
            iteration += 1
        
        # 9. Fallback: força resposta final
        return self._fallback_answer(handler, env)
```

## LMHandler - Servidor TCP

```python
class LMHandler:
    """Servidor TCP que roteia chamadas LM."""
    
    def __init__(self, clients, sampling_args=None):
        # Cria servidor TCP em 127.0.0.1:0 (porta auto)
        self._server = ThreadingTCPServer(
            ("127.0.0.1", 0), LMRequestHandler
        )
        self._port = self._server.server_address[1]
        self._clients = clients  # Lista de BaseLM
    
    def completion(self, prompt, model=None):
        """Chamada direta (para o processo principal)."""
        client = self._get_client(model)
        return client.completion(prompt)
    
    def _get_client(self, model=None):
        """Roteamento: depth=0 usa default, depth=1 usa other_backend."""
        if model is None:
            return self._clients[0]
        # Busca cliente por nome do modelo
        for client in self._clients:
            if model in client.model_name:
                return client
        return self._clients[0]
```

## Comunicação Socket

```python
# Formato: 4 bytes big-endian length + UTF-8 JSON
def socket_send(sock, data):
    payload = json.dumps(data).encode("utf-8")
    sock.sendall(struct.pack(">I", len(payload)) + payload)

def socket_recv(sock):
    length_bytes = sock.recv(4)
    length = struct.unpack(">I", length_bytes)[0]
    payload = sock.recv(length)
    return json.loads(payload.decode("utf-8"))

# Request/Response dataclasses
@dataclass
class LMRequest:
    prompt: str | list[dict] | None = None
    prompts: list[str | list[dict]] | None = None  # batched
    model: str | None = None
    depth: int = 0

@dataclass
class LMResponse:
    error: str | None = None
    chat_completion: str | None = None      # single
    chat_completions: list[str] | None = None  # batched
```

## Exceções

```python
class BudgetExceededError(Exception):
    """Gasto financeiro excedeu o limite."""
    def __init__(self, spent, budget):
        self.spent = spent
        self.budget = budget
        super().__init__(f"Budget exceeded: ${spent:.2f} > ${budget:.2f}")

class TimeoutExceededError(Exception):
    """Timeout excedido."""
    def __init__(self, elapsed, timeout, partial_answer=None):
        self.elapsed = elapsed
        self.timeout = timeout
        self.partial_answer = partial_answer

class TokenLimitExceededError(Exception):
    """Limite de tokens excedido."""
    def __init__(self, tokens_used, token_limit, partial_answer=None):
        self.tokens_used = tokens_used
        self.token_limit = token_limit
        self.partial_answer = partial_answer

class ErrorThresholdExceededError(Exception):
    """Limite de erros excedido."""
    def __init__(self, error_count, threshold, last_error, partial_answer=None):
        self.error_count = error_count
        self.threshold = threshold
        self.last_error = last_error
        self.partial_answer = partial_answer

class CancellationError(Exception):
    """Cancelado pelo usuário (Ctrl+C)."""
    def __init__(self, partial_answer=None):
        self.partial_answer = partial_answer
```

## Compaction (Compactação de Histórico)

Quando o contexto do LLM está ficando cheio:

```python
def _compact_history(self, messages, model):
    limit = get_context_limit(model)  # ex: 128K para GPT-4o
    current = count_tokens(messages, model)
    
    if current > limit * 0.85:  # 85% do limite
        # 1. Pega últimas 4 mensagens (mais recentes)
        recent = messages[-4:]
        
        # 2. Pega todo o resto
        old = messages[:-4]
        
        # 3. Resume o histórico antigo
        summary = self._lm_handler.completion([
            {"role": "system", "content": "Summarize this conversation in 2-3 paragraphs."},
            {"role": "user", "content": str(old)}
        ])
        
        # 4. Substitui
        messages = [
            {"role": "system", "content": f"Previous conversation summary:\n{summary}"},
            *recent
        ]
```

## Custom Tools

```python
# Injetar tools customizadas no REPL
def search_database(query):
    """Busca no banco de dados interno."""
    return db.search(query)

def get_weather(city):
    """Obtém clima de uma cidade."""
    return api.get_weather(city)

rlm = RLM(
    custom_tools={
        "search_database": search_database,
        "get_weather": get_weather,
    }
)

# Dentro do REPL, o código pode usar:
# result = search_database("SELECT * FROM users")
# weather = get_weather("São Paulo")
```

## Uso Completo

```python
from rlm import RLM

# Configuração básica
rlm = RLM(
    backend="openai",
    backend_kwargs={"api_key": "sk-..."},
    environment="local",
    max_depth=3,
    max_iterations=30,
    max_budget=5.0,
    verbose=True,
)

# Completion simples
result = rlm.completion(
    prompt="Analise estes dados e encontre padrões",
    root_prompt="Quais são os top 3 produtos mais vendidos?"
)
print(result)

# Completion com contexto persistente
with rlm:
    rlm.load_context("Base de dados de vendas 2024")
    
    r1 = rlm.completion("Quais foram as vendas em janeiro?")
    r2 = rlm.completion("Compare com dezembro")
    # O contexto e variáveis persistem entre chamadas

# Sub-calls recursivos
rlm = RLM(max_depth=3, orchestrator=True)
# depth=0: Root LLM
# depth=1: Child RLM (pode ter seu próprio REPL)
# depth=2: Grandchild RLM
# depth=3: falls back to plain llm_query
```

## Metadados de Saída

```python
result = rlm.completion(...)

# result é RLMChatCompletion
print(result.root_model)      # "gpt-4o"
print(result.execution_time)   # 12.5 segundos
print(result.usage)            # UsageSummary
print(result.iterations)       # Lista de RLMIteration

# Cada iteração tem:
for it in result.iterations:
    print(it.prompt)           # Mensagens enviadas
    print(it.response)         # Resposta do LLM
    print(it.code_blocks)      # Blocos ```repl executados
    print(it.final_answer)     # Se foi a última
    print(it.iteration_time)   # Tempo desta iteração
```
