# Implementação Minimal do RLM - Guia Completo

Este guia mostra como construir um RLM do zero, baseado no projeto `rlm-minimal/`.

## Estrutura do Projeto

```
rlm-minimal/
├── main.py                  # Entry point
├── rlm/
│   ├── __init__.py          # Exporta classe RLM (ABC)
│   ├── rlm.py               # Classe abstrata RLM
│   ├── rlm_repl.py          # Implementação concreta RLM_REPL
│   ├── repl.py              # Ambiente REPL (exec de código)
│   └── utils/
│       ├── __init__.py
│       ├── llm.py           # Cliente OpenAI wrapper
│       ├── prompts.py       # System prompts
│       └── utils.py         # Parsing, execução, utilitários
├── requirements.txt
└── .env-example
```

## Passo 1: A Classe Abstrata RLM

```python
# rlm/rlm.py
from abc import ABC, abstractmethod

class RLM(ABC):
    """Interface que todo RLM deve implementar."""
    
    @abstractmethod
    def completion(self, context, query) -> str:
        """
        Dado um contexto (potencialmente gigante) e uma query,
        retorna a resposta usando um REPL recursivo.
        """
        pass
    
    @abstractmethod
    def cost_summary(self) -> dict:
        pass
    
    @abstractmethod
    def reset(self):
        pass
```

## Passo 2: O Cliente LLM

```python
# rlm/utils/llm.py
import os
from openai import OpenAI
from dotenv import load_dotenv

load_dotenv()

class OpenAIClient:
    def __init__(self, api_key=None, model="gpt-5"):
        self.api_key = api_key or os.getenv("OPENAI_API_KEY")
        self.model = model
        self.client = OpenAI(api_key=self.api_key)
    
    def completion(self, messages, max_tokens=4096, **kwargs):
        # Aceita string ou lista de dicts
        if isinstance(messages, str):
            messages = [{"role": "user", "content": messages}]
        
        response = self.client.chat.completions.create(
            model=self.model,
            messages=messages,
            max_completion_tokens=max_tokens,
            **kwargs
        )
        return response.choices[0].message.content
```

## Passo 3: O Sub-RLM (chamada recursiva simples)

```python
# rlm/repl.py (parte do Sub_RLM)
class Sub_RLM(RLM):
    """LLM client simples para sub-chamadas dentro do REPL."""
    
    def __init__(self, model="gpt-5"):
        self.client = OpenAIClient(model=model)
    
    def completion(self, prompt) -> str:
        try:
            return self.client.completion(messages=prompt)
        except Exception as e:
            return f"Error: {str(e)}"
    
    def cost_summary(self):
        raise NotImplementedError
    
    def reset(self):
        raise NotImplementedError
```

**Ponto chave**: `Sub_RLM` poderia ser substituído por `RLM_REPL` para permitir recursão real (depth > 1).

## Passo 4: O Ambiente REPL

Esta é a parte mais complexa. O REPL precisa:

1. Manter um namespace Python isolado
2. Executar código de forma segura
3. Capturar stdout/stderr
4. Disponibilizar `llm_query()` e `FINAL_VAR()`

```python
# rlm/repl.py
import sys
import io
import threading
import json
import tempfile
import os
import time
from contextlib import contextmanager
from dataclasses import dataclass

@dataclass
class REPLResult:
    stdout: str
    stderr: str
    locals: dict
    execution_time: float

class REPLEnv:
    def __init__(self, recursive_model="gpt-5-mini", 
                 context_json=None, context_str=None):
        
        # Diretório temporário para arquivos
        self.temp_dir = tempfile.mkdtemp(prefix="repl_env_")
        
        # Sub-LLM para chamadas internas
        self.sub_rlm = Sub_RLM(model=recursive_model)
        
        # Namespace seguro
        self.globals = {
            '__builtins__': {
                # ✅ Builtins permitidos
                'print': print, 'len': len, 'str': str,
                'int': int, 'float': float, 'list': list,
                'dict': dict, 'set': set, 'tuple': tuple,
                'range': range, 'enumerate': enumerate,
                'zip': zip, 'map': map, 'filter': filter,
                'sorted': sorted, 'min': min, 'max': max,
                'sum': sum, 'abs': abs, 'round': round,
                '__import__': __import__, 'open': open,
                # ... mais builtins seguros
                
                # ❌ Bloqueados
                'input': None, 'eval': None,
                'exec': None, 'compile': None,
                'globals': None, 'locals': None,
            }
        }
        self.locals = {}
        self._lock = threading.Lock()
        
        # Carrega contexto
        self.load_context(context_json, context_str)
        
        # Injeta funções especiais
        self.globals['llm_query'] = self._llm_query
        self.globals['FINAL_VAR'] = self._final_var
    
    def _llm_query(self, prompt: str) -> str:
        """Chamada ao sub-LLM."""
        return self.sub_rlm.completion(prompt)
    
    def _final_var(self, variable_name: str) -> str:
        """Retorna variável do REPL como resposta final."""
        variable_name = variable_name.strip().strip('"').strip("'")
        if variable_name in self.locals:
            return str(self.locals[variable_name])
        return f"Error: Variable '{variable_name}' not found"
    
    def load_context(self, context_json=None, context_str=None):
        """Carrega contexto no namespace como variável 'context'."""
        if context_json is not None:
            path = os.path.join(self.temp_dir, "context.json")
            with open(path, "w") as f:
                json.dump(context_json, f, indent=2)
            self.code_execution(
                f"import json\n"
                f"with open(r'{path}', 'r') as f:\n"
                f"    context = json.load(f)\n"
            )
        
        if context_str is not None:
            path = os.path.join(self.temp_dir, "context.txt")
            with open(path, "w") as f:
                f.write(context_str)
            self.code_execution(
                f"with open(r'{path}', 'r') as f:\n"
                f"    context = f.read()\n"
            )
    
    def code_execution(self, code: str) -> REPLResult:
        """Executa código Python no REPL."""
        start_time = time.time()
        
        with self._lock:
            # Redireciona stdout/stderr
            old_stdout, old_stderr = sys.stdout, sys.stderr
            stdout_buf, stderr_buf = io.StringIO(), io.StringIO()
            sys.stdout, sys.stderr = stdout_buf, stderr_buf
            
            try:
                old_cwd = os.getcwd()
                os.chdir(self.temp_dir)
                
                # Separa imports do resto
                lines = code.split('\n')
                import_lines = [l for l in lines 
                               if l.startswith(('import ', 'from '))]
                other_lines = [l for l in lines 
                              if not l.startswith(('import ', 'from '))]
                
                # Executa imports primeiro
                if import_lines:
                    exec('\n'.join(import_lines), self.globals, self.globals)
                
                # Executa o resto
                if other_lines:
                    combined = {**self.globals, **self.locals}
                    exec('\n'.join(other_lines), combined, combined)
                    
                    # Atualiza locals com novas variáveis
                    for k, v in combined.items():
                        if k not in self.globals:
                            self.locals[k] = v
                
                os.chdir(old_cwd)
            except Exception as e:
                stderr_buf.write(str(e))
            finally:
                sys.stdout, sys.stderr = old_stdout, old_stderr
        
        execution_time = time.time() - start_time
        return REPLResult(
            stdout=stdout_buf.getvalue(),
            stderr=stderr_buf.getvalue(),
            locals=self.locals.copy(),
            execution_time=execution_time
        )
```

## Passo 5: O Orquestrador (RLM_REPL)

```python
# rlm/rlm_repl.py
from rlm import RLM
from rlm.repl import REPLEnv
from rlm.utils.llm import OpenAIClient
from rlm.utils.prompts import build_system_prompt, next_action_prompt
import rlm.utils.utils as utils

class RLM_REPL(RLM):
    def __init__(self, api_key=None, model="gpt-5",
                 recursive_model="gpt-5-nano", max_iterations=20):
        self.llm = OpenAIClient(api_key, model)
        self.recursive_model = recursive_model
        self._max_iterations = max_iterations
        self.repl_env = None
        self.messages = []
        self.query = None
    
    def setup_context(self, context, query):
        """Inicializa REPL com contexto."""
        self.query = query
        self.messages = build_system_prompt()
        
        context_data, context_str = utils.convert_context_for_repl(context)
        self.repl_env = REPLEnv(
            context_json=context_data,
            context_str=context_str,
            recursive_model=self.recursive_model,
        )
    
    def completion(self, context, query=None) -> str:
        """Loop principal do RLM."""
        self.setup_context(context, query)
        
        for iteration in range(self._max_iterations):
            # 1. Pede ao LLM para agir
            response = self.llm.completion(
                self.messages + [next_action_prompt(query, iteration)]
            )
            
            # 2. Verifica se tem código ```repl
            code_blocks = utils.find_code_blocks(response)
            
            if code_blocks:
                # 3. Executa cada bloco no REPL
                for code in code_blocks:
                    result = self.repl_env.code_execution(code)
                    # Adiciona resultado ao histórico
                    self.messages.append({
                        "role": "user",
                        "content": f"REPL Output:\n{result.stdout}"
                    })
            else:
                # 4. Sem código, adiciona como mensagem
                self.messages.append({
                    "role": "assistant",
                    "content": response
                })
            
            # 5. Verifica se há resposta final
            final = utils.check_for_final_answer(response, self.repl_env)
            if final:
                return final
        
        # 6. Força resposta final
        self.messages.append(next_action_prompt(query, 0, final_answer=True))
        return self.llm.completion(self.messages)
```

## Passo 6: Utilitários de Parsing

```python
# rlm/utils/utils.py
import re

def find_code_blocks(text):
    """Extrai blocos ```repl ... ``` de um texto."""
    pattern = r'```repl\s*\n(.*?)```'
    matches = re.findall(pattern, text, re.DOTALL)
    return matches if matches else None

def check_for_final_answer(response, repl_env):
    """Verifica se há FINAL(...) ou FINAL_VAR(...) na resposta."""
    # FINAL(content)
    match = re.search(r'FINAL\((.*?)\)', response, re.DOTALL)
    if match:
        return match.group(1).strip()
    
    # FINAL_VAR(var_name)
    match = re.search(r'FINAL_VAR\((.*?)\)', response)
    if match:
        var_name = match.group(1).strip().strip('"').strip("'")
        if var_name in repl_env.locals:
            return str(repl_env.locals[var_name])
    
    return None

def convert_context_for_repl(context):
    """Converte contexto para formato REPL."""
    if isinstance(context, dict):
        return context, None
    elif isinstance(context, str):
        return None, context
    elif isinstance(context, list):
        if all(isinstance(d, dict) for d in context):
            return context, None
        return None, "\n".join(str(c) for c in context)
    return None, str(context)
```

## Passo 7: System Prompt

```python
# rlm/utils/prompts.py
REPL_SYSTEM_PROMPT = """You are tasked with answering a query with associated context. 
You can access, transform, and analyze this context interactively in a REPL environment 
that can recursively query sub-LLMs.

The REPL environment is initialized with:
1. A `context` variable that contains the data to analyze
2. A `llm_query` function to call a sub-LLM (handles ~500K chars)
3. The ability to use `print()` to view output

When you want to execute Python code, wrap it in ```repl``` blocks.

Example strategy:
1. Check context size and structure
2. Chunk the context into manageable pieces
3. Use llm_query() to analyze each chunk
4. Aggregate results
5. Output FINAL(answer) or FINAL_VAR(variable_name)

When done, provide a final answer:
- FINAL(your answer) - for direct answers
- FINAL_VAR(variable_name) - to return a REPL variable
"""

def build_system_prompt():
    return [{"role": "system", "content": REPL_SYSTEM_PROMPT}]

def next_action_prompt(query, iteration=0, final_answer=False):
    if final_answer:
        return {"role": "user", "content": "Provide your final answer now."}
    if iteration == 0:
        return {"role": "user", "content": 
            f"You haven't interacted with the REPL yet. "
            f"Look at the context first.\n\nQuery: {query}"}
    return {"role": "user", "content": 
        f"Continue using the REPL to answer: {query}"}
```

## Passo 8: Uso

```python
# main.py
from rlm.rlm_repl import RLM_REPL

rlm = RLM_REPL(
    model="gpt-5",
    recursive_model="gpt-5-nano",
    max_iterations=10
)

# Contexto massivo
context = "\n".join(["random text line " * 10 for _ in range(1_000_000)])

result = rlm.completion(
    context=context,
    query="What is the meaning of life?"
)
print(result)
```

## Resumo das Peças

| Componente | Responsabilidade | Linhas (aprox) |
|------------|------------------|----------------|
| `rlm.py` | Interface abstrata | 20 |
| `llm.py` | Wrapper OpenAI | 40 |
| `repl.py` | Execução segura de código | 300 |
| `rlm_repl.py` | Orquestrador principal | 135 |
| `prompts.py` | System prompts | 70 |
| `utils.py` | Parsing e helpers | 100 |
| **Total** | | **~665** |
