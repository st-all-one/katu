# Referência de API Completa

Este guia documenta todos os tipos, protocols, constantes e funções que faltaram nos guias anteriores.

## 1. Tipos do Sistema (`core/types.py`)

### Tipos Literais

```python
ClientBackend = Literal[
    "openai", "portkey", "openrouter", "vercel", "vllm",
    "anthropic", "azure_openai", "gemini",
]

EnvironmentType = Literal[
    "local", "ipython", "docker", "modal",
    "prime", "daytona", "e2b",
]
```

### ModelUsageSummary

```python
@dataclass
class ModelUsageSummary:
    total_calls: int
    total_input_tokens: int
    total_output_tokens: int
    total_cost: float | None = None  # USD, se disponível
    
    def to_dict(self) -> dict: ...
    @classmethod
    def from_dict(cls, data: dict) -> "ModelUsageSummary": ...
```

### UsageSummary

```python
@dataclass
class UsageSummary:
    model_usage_summaries: dict[str, ModelUsageSummary]
    
    @property
    def total_cost(self) -> float | None: ...      # Agrega custo
    @property
    def total_input_tokens(self) -> int: ...        # Agrega input tokens
    @property
    def total_output_tokens(self) -> int: ...       # Agrega output tokens
    
    def to_dict(self) -> dict: ...
    @classmethod
    def from_dict(cls, data: dict) -> "UsageSummary": ...
```

### RLMChatCompletion

```python
@dataclass
class RLMChatCompletion:
    """Registro de uma única chamada LLM feita do environment."""
    root_model: str
    prompt: str | dict[str, Any]
    response: str
    usage_summary: UsageSummary
    execution_time: float
    metadata: dict | None = None       # Trajetória completa (logger)
    error: str | None = None           # Set quando a chamada falhou
    
    def to_dict(self) -> dict: ...
    @classmethod
    def from_dict(cls, data: dict) -> "RLMChatCompletion": ...
```

### REPLResult

```python
@dataclass
class REPLResult:
    stdout: str
    stderr: str
    locals: dict                       # Variáveis locais após execução
    execution_time: float
    llm_calls: list[RLMChatCompletion] # Sub-calls feitos durante a execução
    final_answer: str | None = None    # Resposta final (se answer["ready"]=True)
    
    def to_dict(self) -> dict: ...
```

### CodeBlock

```python
@dataclass
class CodeBlock:
    """Um bloco de código ```repl``` executado e seu resultado."""
    code: str
    result: REPLResult
    
    def to_dict(self) -> dict: ...
```

### RLMIteration

```python
@dataclass
class RLMIteration:
    """Uma iteração completa do loop RLM."""
    prompt: str | dict[str, Any]
    response: str
    code_blocks: list[CodeBlock]       # Blocos executados nesta iteração
    final_answer: str | None = None
    iteration_time: float | None = None
    
    def to_dict(self) -> dict: ...
```

### RLMMetadata

```python
@dataclass
class RLMMetadata:
    """Snapshot de configuração do RLM."""
    root_model: str
    max_depth: int
    max_iterations: int
    backend: str
    backend_kwargs: dict[str, Any]
    environment_type: str
    environment_kwargs: dict[str, Any]
    other_backends: list[str] | None = None
    
    def to_dict(self) -> dict: ...
```

### QueryMetadata

```python
@dataclass
class QueryMetadata:
    """Metadados do contexto para construção do prompt."""
    context_lengths: list[int]   # Tamanho de cada chunk
    context_total_length: int    # Soma total
    context_type: str            # "str", "dict", "list"
    
    def __init__(self, prompt):
        """
        Constrói metadados a partir do contexto.
        
        Suporta:
        - str: context_lengths = [len(prompt)]
        - dict: mede cada valor
        - list[dict] com "content": extrai content de cada dict
        - list[dict] sem "content": JSON-serializa cada um
        - list[str]: mede cada string
        """
```

Uso no prompt:
```
Your context is a {context_type} of {context_total_length} total characters.
Each sub-LLM call can handle roughly ~100k tokens at once.
```

### _serialize_value()

```python
def _serialize_value(value: Any) -> Any:
    """Serializa valor para JSON (usado em to_dict())."""
    # None, bool, int, float, str → retorna como está
    # ModuleType → "<module 'name'>"
    # list/tuple → serializa recursivamente
    # dict → serializa recursivamente (keys viram str)
    # callable → "<ClassName 'name'>"
    # outro → repr()
```

---

## 2. Token Utils (`utils/token_utils.py`)

### Constantes

```python
DEFAULT_CONTEXT_LIMIT = 128_000         # Tokens para modelo desconhecido
CHARS_PER_TOKEN_ESTIMATE = 4            # Estimativa sem tiktoken
```

### MODEL_CONTEXT_LIMITS

```python
MODEL_CONTEXT_LIMITS: dict[str, int] = {
    # OpenAI
    "gpt-5-nano": 272_000,
    "gpt-5": 272_000,
    "gpt-4o-mini": 128_000,
    "gpt-4o": 128_000,
    "gpt-4-turbo": 128_000,
    "gpt-4-32k": 32_768,
    "gpt-4": 8_192,
    "gpt-3.5-turbo": 16_385,
    "o1": 200_000,
    # Anthropic
    "claude-3-5-sonnet": 200_000,
    "claude-3-opus": 200_000,
    "claude-2.1": 200_000,
    "claude-2": 100_000,
    # Gemini (1M context!)
    "gemini-2.5-flash": 1_000_000,
    "gemini-2.5-pro": 1_000_000,
    "gemini-2.0-flash": 1_000_000,
    "gemini-1.5-pro": 1_000_000,
    "gemini-1.0-pro": 30_720,
    # Qwen
    "qwen3-max": 256_000,
    "qwen3-72b": 128_000,
    "qwen3-8b": 32_768,
    # Kimi
    "kimi-k2.5": 262_000,
    "kimi-k2": 128_000,
    # GLM
    "glm-4-9b": 1_000_000,
    "glm-4.6": 200_000,
    "glm-4": 128_000,
}
```

### get_context_limit()

```python
def get_context_limit(model_name: str) -> int:
    """
    Retorna limite de contexto em tokens.
    
    Match: key contida em model_name.
    Ex: "gpt-4o" matches "@openai/gpt-4o"
    Longest key wins.
    Fallback: DEFAULT_CONTEXT_LIMIT (128K)
    """
```

### count_tokens()

```python
def count_tokens(messages: list[dict], model_name: str) -> int:
    """
    Conta tokens em lista de mensagens.
    
    1. Tenta tiktoken (se disponível e modelo é OpenAI)
    2. Fallback: len(str(content)) / CHARS_PER_TOKEN_ESTIMATE
    
    Overhead OpenAI: 3 tokens/mensagem + 1 token/name
    """
```

---

## 3. Protocols (`environments/base_env.py`)

### SupportsPersistence

```python
@runtime_checkable
class SupportsPersistence(Protocol):
    """Protocol para ambientes com sessões multi-turn persistentes."""
    
    def update_handler_address(self, address: tuple[str, int]) -> None:
        """Atualiza endereço do LMHandler entre completions."""
        ...
    
    def add_context(self, context_payload, context_index: int | None = None) -> int:
        """
        Adiciona contexto como context_N.
        context_index=None: auto-incrementa
        Retorna: índice usado
        """
        ...
    
    def get_context_count(self) -> int: ...
    
    def add_history(self, message_history: list[dict], 
                    history_index: int | None = None) -> int:
        """
        Adiciona histórico como history_N.
        IMPORTANTE: armazena deep copy, não referência.
        """
        ...
    
    def get_history_count(self) -> int: ...
```

**Versionamento:**
```
context_0, context_1, context_2, ...  → contextos
history_0, history_1, history_2, ...  → históricos
context → alias para context_0
history → alias para history_0
```

### SupportsCustomTools

```python
@runtime_checkable
class SupportsCustomTools(Protocol):
    """Protocol para ambientes que suportam custom tools."""
    custom_tools: dict[str, Any]
```

**Verificar suporte:**
```python
if isinstance(env, SupportsCustomTools):
    env.custom_tools = {"minha_tool": func}
```

**NOTA IMPORTANTE:** `llm_query()` NÃO tem acesso a custom tools. Apenas o contexto de execução principal do RLM tem acesso.

### RESERVED_TOOL_NAMES

```python
RESERVED_TOOL_NAMES: frozenset[str] = frozenset({
    "llm_query",
    "llm_query_batched",
    "rlm_query",
    "rlm_query_batched",
    "SHOW_VARS",
    "answer",
    "context",
    "history",    # ← nota: history também é reservado
})
```

### ToolInfo e Parse

```python
@dataclass
class ToolInfo:
    name: str
    value: Any
    description: str | None = None
    
    @property
    def is_callable(self) -> bool: ...

# Dois formatos suportados:
tools = {
    # Formato 1: valor direto
    "fetch_data": my_function,
    "API_KEY": "sk-...",
    
    # Formato 2: dict com descrição
    "calculator": {
        "tool": calc_function,
        "description": "Performs arithmetic calculations",
    },
}
```

### BaseEnv

```python
class BaseEnv(ABC):
    def __init__(
        self,
        persistent: bool = False,
        depth: int = 1,
        max_concurrent_subcalls: int = 4,
        **kwargs
    ):
        ...
    
    @abstractmethod
    def setup(self): ...
    
    @abstractmethod
    def load_context(self, context_payload: dict | list | str): ...
    
    @abstractmethod
    def execute_code(self, code: str) -> REPLResult: ...
```

---

## 4. Socket Protocol (`core/comms_utils.py`)

### Wire Protocol

```
┌──────────────────────────────────────────────────┐
│ [4 bytes: big-endian length] [N bytes: UTF-8 JSON] │
└──────────────────────────────────────────────────┘
```

### LMRequest

```python
@dataclass
class LMRequest:
    prompt: str | dict | None = None       # Single prompt
    prompts: list[str | dict] | None = None # Batched prompts
    model: str | None = None
    depth: int = 0
    
    @property
    def is_batched(self) -> bool: ...  # prompts is not None and len > 0
    
    def to_dict(self) -> dict: ...     # Exclui None values
    @classmethod
    def from_dict(cls, data: dict) -> "LMRequest": ...
```

### LMResponse

```python
@dataclass
class LMResponse:
    error: str | None = None
    chat_completion: RLMChatCompletion | None = None       # Single
    chat_completions: list[RLMChatCompletion] | None = None # Batched
    
    @property
    def success(self) -> bool: ...    # error is None
    @property
    def is_batched(self) -> bool: ... # chat_completions is not None
    
    @classmethod
    def success_response(cls, completion) -> "LMResponse": ...
    @classmethod
    def batched_success_response(cls, completions) -> "LMResponse": ...
    @classmethod
    def error_response(cls, error: str) -> "LMResponse": ...
```

### Funções de Socket

```python
def socket_send(sock, data):
    """Envia mensagem length-prefixed."""
    payload = json.dumps(data).encode("utf-8")
    sock.sendall(struct.pack(">I", len(payload)) + payload)

def socket_recv(sock) -> dict:
    """Recebe mensagem. Loop para receber mensagem parcial."""
    raw_len = sock.recv(4)
    if not raw_len:
        return {}
    length = struct.unpack(">I", raw_len)[0]
    payload = b""
    while len(payload) < length:
        chunk = sock.recv(length - len(payload))
        if not chunk:
            raise ConnectionError("Connection closed mid-message")
        payload += chunk
    return json.loads(payload.decode("utf-8"))

def socket_request(address, data, timeout=300) -> dict:
    """Request/response completo em uma chamada."""
    with socket.socket(AF_INET, SOCK_STREAM) as sock:
        sock.settimeout(timeout)
        sock.connect(address)
        socket_send(sock, data)
        return socket_recv(sock)

def send_lm_request(address, request, timeout=300, depth=None) -> LMResponse:
    """Helper tipado para single request."""
    ...

def send_lm_request_batched(address, prompts, model=None, timeout=300, depth=0) -> list[LMResponse]:
    """Helper tipado para batched requests. 
    Erro por prompt: retorna erro only para aquele slot, outros continuam OK."""
    ...
```

---

## 5. Parsing (`utils/parsing.py`)

### find_code_blocks()

```python
def find_code_blocks(text: str) -> list[str]:
    """
    Extrai blocos ```repl ... ``` de um texto.
    Retorna lista de strings de código (vazia se nenhum encontrado).
    """
    pattern = r"```repl\s*\n(.*?)\n```"
    return [m.group(1).strip() for m in re.finditer(pattern, text, re.DOTALL)]
```

### format_iteration()

```python
def format_iteration(iteration: RLMIteration, max_character_length=20000) -> list[dict]:
    """
    Formata iteração para histórico de mensagens.
    
    Retorna EXATAMENTE:
    - 1 mensagem (assistant) se nenhum código executado
    - 2 mensagens (assistant + user) se código executado
    
    A mensagem user contém TODOS os outputs de blocos concatenados,
    com header "REPL output (block N):" se múltiplos blocos.
    Cada output é truncado em max_character_length.
    """
```

### format_execution_result()

```python
def format_execution_result(result: REPLResult) -> str:
    """
    Formata resultado para exibição.
    
    Inclui:
    - stdout (se existe)
    - stderr (se existe)
    - Lista de nomes de variáveis (não valores!)
    
    IMPORTANTE: mostra NOMES das variáveis, não valores.
    Isso evita poluir o contexto com dados grandes.
    """
```

### convert_context_for_repl()

```python
def convert_context_for_repl(context):
    """
    Converte contexto para formato REPL.
    
    dict        → (context, None)
    str         → (None, context)
    list[dict] com "content" → ([msg["content"] for msg], None)
    list[dict] sem "content" → (context, None)
    list[str]   → (context, None)
    outro       → (context, None)
    """
```

---

## 6. Parâmetros Ausentes

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
    # Pode ajustar: 0.7 = compacta mais cedo, 0.9 = compacta mais tarde
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

### sampling_args

```python
rlm = RLM(
    sampling_args={
        "temperature": 0.7,
        "top_p": 0.9,
        "max_tokens": 4096,
        "seed": 42,
    }
)
```

---

## 7. Callbacks - Assinaturas Reais

```python
# O que o código fonte realmente define:
callbacks = {
    # depth: int, iteration_num: int
    "on_iteration_start": lambda depth, iteration_num: ...,
    
    # depth: int, iteration_num: int, duration: float
    "on_iteration_complete": lambda depth, iteration_num, duration: ...,
    
    # depth: int, model: str, prompt_preview: str
    "on_subcall_start": lambda depth, model, prompt_preview: ...,
    
    # depth: int, model: str, duration: float, error: str | None
    "on_subcall_complete": lambda depth, model, duration, error: ...,
}
```

**NOTA:** O guia 08 mostrava `(rlm, iteration)` mas as assinaturas reais são `(depth, iteration_num)`.

---

## 8. Context Manager

```python
rlm = RLM(persistent=True)

with rlm:
    # Environment persiste entre completions
    r1 = rlm.completion("...")
    r2 = rlm.completion("...")
# __exit__ chama rlm.close() que limpa o environment
```

**`close()`:** Limpa recursos do environment persistente. Chamado automaticamente pelo `__exit__`.

---

## 9. Thread Safety

- Cada `completion()` cria seu próprio LMHandler e Environment
- Chamadas são independentes
- A instância RLM **NÃO** deve ser compartilhada entre threads sem sincronização externa
- O `LocalREPL` usa `threading.Lock()` para proteger stdout/stderr
