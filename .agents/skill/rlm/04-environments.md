# Ambientes REPL - Guia Completo

## Visão Geral

O RLM suporta múltiplos ambientes de execução, cada um com trade-offs diferentes:

```
┌─────────────────────────────────────────────────────────────┐
│                     Ambientes Disponíveis                    │
├──────────────┬────────────┬──────────┬──────────┬──────────┤
│              │  Segurança │  Veloci- │  Persist-│  Custom  │
│  Ambiente    │            │  dade    │  encia   │  Tools   │
├──────────────┼────────────┼──────────┼──────────┼──────────┤
│  Local       │  Baixa     │  Alta    │  Sim     │  Sim     │
│  Docker      │  Alta      │  Média   │  Sim     │  Parcial │
│  IPython     │  Média     │  Alta    │  Sim     │  Sim     │
│  Modal       │  Alta      │  Baixa   │  Não     │  Parcial │
│  Prime       │  Alta      │  Baixa   │  Não     │  Parcial │
│  Daytona     │  Alta      │  Média   │  Não     │  Parcial │
│  E2B         │  Alta      │  Média   │  Não     │  Parcial │
└──────────────┴────────────┴──────────┴──────────┴──────────┘
```

## 1. LocalREPL (Padrão)

**Arquivo:** `rlm/environments/local_repl.py` (604 linhas)

Executa código via `exec()` no mesmo processo Python. Mais rápido, sem overhead.

### Como Funciona

```python
class LocalREPL(NonIsolatedEnv):
    def setup(self):
        self._globals = {
            '__builtins__': _SAFE_BUILTINS,
            'llm_query': self._make_llm_query(),
            'llm_query_batched': self._make_llm_query_batched(),
            'rlm_query': self._make_rlm_query(),
            'rlm_query_batched': self._make_rlm_query_batched(),
            'SHOW_VARS': self._show_vars,
            'answer': _AnswerDict(self._on_answer_ready),
        }
        self._locals = {}
    
    def execute_code(self, code: str) -> REPLResult:
        with self._capture_output():
            combined = {**self._globals, **self._locals}
            
            # Separa imports
            import_lines, other_lines = self._split_imports(code)
            
            if import_lines:
                exec('\n'.join(import_lines), self._globals, self._globals)
            
            if other_lines:
                # Detecta expressão na última linha
                if self._is_expression(other_lines[-1]):
                    # Executa statements + eval na última
                    ...
                else:
                    exec('\n'.join(other_lines), combined, combined)
                
                # Atualiza locals
                for k, v in combined.items():
                    if k not in self._globals:
                        self._locals[k] = v
            
            # Restaura scaffold
            self._restore_scaffold(combined)
```

### Namespace Seguro

```python
_SAFE_BUILTINS = {
    # Operações básicas
    'print': print, 'len': len, 'str': str, 'int': int,
    'float': float, 'bool': bool, 'type': type,
    
    # Coleções
    'list': list, 'dict': dict, 'set': set, 'tuple': tuple,
    'range': range, 'enumerate': enumerate, 'zip': zip,
    'sorted': sorted, 'reversed': reversed,
    
    # Funções funcionais
    'map': map, 'filter': filter, 'sum': sum,
    'min': min, 'max': max, 'abs': abs,
    
    # I/O
    'open': open, '__import__': __import__,
    
    # Exceções (para try/except)
    'Exception': Exception, 'ValueError': ValueError,
    'TypeError': TypeError, 'KeyError': KeyError,
    'IndexError': IndexError, 'RuntimeError': RuntimeError,
    
    # ❌ Bloqueados
    'input': None, 'eval': None, 'exec': None,
    'compile': None, 'globals': None, 'locals': None,
}
```

### Answer Dict

```python
class _AnswerDict(dict):
    """Dict customizado que dispara callback quando answer['ready'] = True."""
    
    def __init__(self, on_ready_callback):
        super().__init__({"content": "", "ready": False})
        self._on_ready = on_ready_callback
    
    def __setitem__(self, key, value):
        super().__setitem__(key, value)
        if key == "ready" and value is True:
            self._on_ready(self.get("content", ""))
```

### Persistência

```python
# Com environment persistente:
with RLM(persistent=True) as rlm:
    rlm.load_context("doc1.txt")
    r1 = rlm.completion("Pergunta 1")
    
    # Segunda chamada - contexto e variáveis persistem!
    r2 = rlm.completion("Pergunta 2 sobre o mesmo doc")

# Variáveis versionadas:
# context_0, context_1, ... (múltiplos contextos)
# history_0, history_1, ... (múltiplos históricos)
```

## 2. DockerREPL

**Arquivo:** `rlm/environments/docker_repl.py` (773 linhas)

Executa código em container Docker isolado. Mais seguro para código não confiável.

### Arquitetura

```
┌─────────────────────────────────────────────┐
│ Host Machine                                 │
│                                              │
│  RLM ──► LMHandler (TCP) ──► DockerREPL     │
│                                  │           │
│                                  │ HTTP      │
│                                  ▼           │
│  ┌──────────────────────────────────────┐   │
│  │ Docker Container                      │   │
│  │                                       │   │
│  │  LLMProxyHandler (Flask)             │   │
│  │  ├── /llm_query                       │   │
│  │  ├── /llm_query_batched               │   │
│  │  ├── /rlm_query                       │   │
│  │  └── /rlm_query_batched               │   │
│  │                                       │   │
│  │  Exec Script (exec code)              │   │
│  │  └── llm_query() → HTTP → proxy       │   │
│  └──────────────────────────────────────┘   │
└─────────────────────────────────────────────┘
```

### Setup

```python
from rlm import RLM

rlm = RLM(
    environment="docker",
    environment_kwargs={
        "image": "python:3.12-slim",
        "timeout": 60,
    }
)
```

### State Persistence

```python
# Usa dill para serializar estado
import dill

# Salva
with open("/workspace/state.dill", "wb") as f:
    dill.dump({
        "locals": repl._locals,
        "globals_keys": list(repl._globals.keys()),
    }, f)

# Carrega
with open("/workspace/state.dill", "rb") as f:
    state = dill.load(f)
```

## 3. IPythonREPL

**Arquivo:** `rlm/environments/ipython_repl.py` (1200+ linhas)

Usa IPython/Jupyter kernel para execução. Suporta dois modos.

### Dois Modos

```python
# Modo 1: In-process (padrão)
rlm = RLM(environment="ipython")
# Usa InteractiveShell do IPython no mesmo processo

# Modo 2: Subprocess
rlm = RLM(
    environment="ipython",
    environment_kwargs={"mode": "subprocess"}
)
# Usa jupyter_client.KernelManager em processo separado
```

### In-Process vs Subprocess

| Feature | In-Process | Subprocess |
|---------|------------|------------|
| Velocidade | Mais rápido | Mais lento |
| Isolamento | Mesmo processo | Processo separado |
| Cell timeout | SIGALRM | `execute_interactive(timeout=...)` |
| Interrupt | Não suportado | `km.interrupt_kernel()` |
| Memória | Compartilhada | Separada |

### Subcall Broker (Subprocess)

```python
class _SubcallBroker:
    """Roteador TCP para sub-calls no modo subprocess."""
    
    def __init__(self, handler_address):
        self._server = ThreadingTCPServer(
            ("127.0.0.1", 0), SubcallRequestHandler
        )
        self._address = self._server.server_address
    
    def get_address(self):
        return self._address
```

## 4. ModalREPL

**Arquivo:** `rlm/environments/modal_repl.py` (515 linhas)

Executa em sandbox cloud da Modal. Seguro para código arbitrário.

### Arquitetura HTTP Broker

```
┌─────────────────────────────────────────────────────────────┐
│ Host                                                        │
│                                                             │
│  RLM ──► LMHandler ──► ModalREPL (poller thread)           │
│                              │                              │
│                              │ HTTP Poll (100ms)             │
│                              ▼                              │
│  ┌────────────────────────────────────────────────────┐    │
│  │ Modal Cloud Sandbox                                 │    │
│  │                                                     │    │
│  │  Broker Server (Flask)                              │    │
│  │  ├── /enqueue  → Submit LLM request (blocks)        │    │
│  │  ├── /pending  → Get pending requests (poll)        │    │
│  │  ├── /respond  → Submit response                    │    │
│  │  └── /health   → Health check                       │    │
│  │                                                     │    │
│  │  Exec Script (exec code)                            │    │
│  │  └── llm_query() → POST /enqueue → blocks → resp   │    │
│  └────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

### Endpoints do Broker

```python
# /enqueue - Submete request do sandbox
@app.route('/enqueue', methods=['POST'])
def enqueue():
    request_id = str(uuid.uuid4())
    event = threading.Event()
    pending[request_id] = {
        'request': request.json,
        'event': event,
        'response': None
    }
    event.wait()  # Bloqueia até resposta
    return pending[request_id]['response']

# /pending - Host poll para requests
@app.route('/pending', methods=['GET'])
def pending_requests():
    return jsonify([
        {'id': rid, 'request': data['request']}
        for rid, data in pending.items()
        if data['response'] is None
    ])

# /respond - Host envia resposta
@app.route('/respond', methods=['POST'])
def respond():
    request_id = request.json['id']
    pending[request_id]['response'] = request.json['response']
    pending[request_id]['event'].set()  # Desbloqueia
    return jsonify({'status': 'ok'})
```

### Setup

```python
rlm = RLM(
    environment="modal",
    environment_kwargs={
        "image": modal.Image.debian_slim().pip_install("openai"),
        "timeout": 120,
    }
)
```

## 5. Ambientes de Nuvem (Prime, Daytona, E2B)

Seguem o mesmo padrão HTTP broker do Modal, com SDKs específicos de cada provider:

- **PrimeREPL**: Prime Intellect - bom para workloads pesados
- **DaytonaREPL**: Daytona - dev environments rápidos
- **E2BREPL**: E2B - sandboxes para AI agents

## Criando um Ambiente Customizado

### Non-Isolated (mesma máquina)

```python
from rlm.environments.base_env import NonIsolatedEnv
from rlm.core.types import REPLResult

class MeuAmbiente(NonIsolatedEnv):
    def setup(self):
        # Inicializa namespace
        self._globals = {
            '__builtins__': _SAFE_BUILTINS,
            'llm_query': self._make_llm_query(),
            'llm_query_batched': self._make_llm_query_batched(),
            'rlm_query': self._make_rlm_query(),
            'rlm_query_batched': self._make_rlm_query_batched(),
            'SHOW_VARS': self._show_vars,
            'answer': _AnswerDict(self._on_answer_ready),
        }
        self._locals = {}
    
    def load_context(self, payload):
        # Carrega contexto como variável 'context'
        self._globals['context'] = payload
    
    def execute_code(self, code: str) -> REPLResult:
        # Implementa execução
        # Retorna REPLResult(stdout, stderr, locals, execution_time)
        pass
    
    def cleanup(self):
        # Limpa recursos
        pass
```

### Isolated (cloud sandbox)

```python
from rlm.environments.base_env import IsolatedEnv

class MeuSandbox(IsolatedEnv):
    def setup(self):
        # Cria sandbox remoto
        self._sandbox = create_sandbox()
        
        # Inicia broker server no sandbox
        self._broker_url = self._start_broker()
        
        # Inicia poller no host
        self._start_poller()
    
    def load_context(self, payload):
        # Envia contexto para o sandbox
        self._sandbox.upload(payload)
    
    def execute_code(self, code: str) -> REPLResult:
        # Envia código para execução no sandbox
        # Retorna resultado
        pass
    
    def cleanup(self):
        # Destroi sandbox
        self._sandbox.destroy()
```

### Registro

```python
# rlm/environments/__init__.py
_ENVIRONMENT_REGISTRY = {
    "local": LocalREPL,
    "docker": DockerREPL,
    "ipython": IPythonREPL,
    "modal": ModalREPL,
    "prime": PrimeREPL,
    "daytona": DaytonaREPL,
    "e2b": E2BREPL,
    # Adiciona o novo:
    "meu_sandbox": MeuSandbox,
}

def get_environment(env_type, **kwargs):
    cls = _ENVIRONMENT_REGISTRY.get(env_type)
    if cls is None:
        raise ValueError(f"Unknown environment: {env_type}")
    return cls(**kwargs)
```

## Protocolo: SupportsPersistence

```python
@runtime_checkable
class SupportsPersistence(Protocol):
    """Protocol para ambientes com sessões multi-turn persistentes."""
    
    def update_handler_address(self, address: tuple[str, int]) -> None:
        """Atualiza endereço do LMHandler entre completions."""
        ...
    
    def add_context(self, context_payload, context_index: int | None = None) -> int:
        """
        Adiciona contexto como context_N no namespace do REPL.
        context_index=None → auto-incrementa (0, 1, 2, ...)
        Retorna: índice usado
        """
        ...
    
    def get_context_count(self) -> int:
        """Número de contextos adicionados."""
        ...
    
    def add_history(self, message_history: list[dict], 
                    history_index: int | None = None) -> int:
        """
        Adiciona histórico como history_N.
        IMPORTANTE: armazena deep copy, não referência.
        """
        ...
    
    def get_history_count(self) -> int:
        """Número de históricos adicionados."""
        ...
```

**Versionamento no namespace:**
```
context_0, context_1, context_2, ...  → contextos versionados
history_0, history_1, history_2, ...  → históricos versionados
context → alias para context_0 (primeiro contexto)
history → alias para history_0 (primeiro histórico)
```

**Verificar suporte:**
```python
if isinstance(env, SupportsPersistence):
    env.add_context("Doc API v2")  # → context_1
    env.add_history(prev_messages)  # → history_0
```

## Protocolo: SupportsCustomTools

```python
@runtime_checkable
class SupportsCustomTools(Protocol):
    """Protocol para ambientes que suportam custom tools."""
    custom_tools: dict[str, Any]
```

**NOTA IMPORTANTE:** `llm_query()` NÃO tem acesso a custom tools. Apenas o contexto de execução principal do RLM tem acesso.

**RESERVED_TOOL_NAMES (completo):**
```python
RESERVED_TOOL_NAMES = frozenset({
    "llm_query", "llm_query_batched",
    "rlm_query", "rlm_query_batched",
    "SHOW_VARS", "answer",
    "context", "history",  # ← history também é reservado!
})
```

**Formatos de tool:**
```python
# Formato 1: valor direto
tools = {"fetch_data": my_function}

# Formato 2: dict com descrição
tools = {"calculator": {"tool": calc_fn, "description": "Arithmetic"}}
```

## BaseEnv - Construtor

```python
class BaseEnv(ABC):
    def __init__(
        self,
        persistent: bool = False,
        depth: int = 1,
        max_concurrent_subcalls: int = 4,
        **kwargs
    ):
        self.persistent = persistent
        self.depth = depth
        self.max_concurrent_subcalls = max_concurrent_subcalls
```

## Checklist para Novos Ambientes

- [ ] Herda de `NonIsolatedEnv` ou `IsolatedEnv`
- [ ] Implementa `setup()`, `load_context()`, `execute_code()`, `cleanup()`
- [ ] Retorna `REPLResult` de `execute_code()`
- [ ] Fornece `llm_query`, `llm_query_batched`, `rlm_query`, `rlm_query_batched`
- [ ] Fornece `answer` dict e `SHOW_VARS()`
- [ ] Restaura scaffold (nomes reservados) após cada execução
- [ ] Registro em `_ENVIRONMENT_REGISTRY`
- [ ] Suporta custom_tools (se aplicável)
- [ ] Implementa `SupportsPersistence` (se multi-turn)
- [ ] Testes unitários passam
