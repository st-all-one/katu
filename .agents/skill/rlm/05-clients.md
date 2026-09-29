# Clients (Backends de LLM) - Guia Completo

## Visão Geral

O RLM suporta múltiplos backends de LLM através de uma interface abstrata unificada.

```
┌──────────────────────────────────────────────────────┐
│                 Client Architecture                   │
│                                                       │
│  ┌──────────┐                                         │
│  │ BaseLM   │ ← Classe abstrata                      │
│  │ (ABC)    │                                         │
│  └────┬─────┘                                         │
│       │                                                │
│       ├──► OpenAIClient    (OpenAI, vLLM, OpenRouter) │
│       ├──► AnthropicClient (Claude)                   │
│       ├──► GeminiClient    (Google Gemini)            │
│       ├──► AzureOpenAIClient (Azure)                  │
│       └──► PortkeyClient   (Multi-provider router)    │
│                                                       │
│  Factory: get_client(backend, **kwargs)               │
└──────────────────────────────────────────────────────┘
```

## BaseLM - Interface Abstrata

```python
# rlm/clients/base_lm.py
from abc import ABC, abstractmethod
from rlm.core.types import ModelUsageSummary, UsageSummary

class BaseLM(ABC):
    def __init__(self, model_name: str, timeout: int = 300, **kwargs):
        self.model_name = model_name
        self.timeout = timeout
        self._usage: list[ModelUsageSummary] = []
    
    @abstractmethod
    def completion(
        self, 
        prompt: str | list[dict], 
        model: str | None = None
    ) -> str:
        """Chamada síncrona de completion."""
        pass
    
    @abstractmethod
    async def acompletion(
        self, 
        prompt: str | list[dict], 
        model: str | None = None
    ) -> str:
        """Chamada assíncrona de completion."""
        pass
    
    @abstractmethod
    def get_usage_summary(self) -> UsageSummary:
        """Retorna resumo de uso agregado."""
        pass
    
    @abstractmethod
    def get_last_usage(self) -> ModelUsageSummary | None:
        """Retorna uso da última chamada."""
        pass
    
    def _track_cost(self, model, input_tokens, output_tokens, cost=None):
        """Rastreia custo por chamada."""
        summary = ModelUsageSummary(
            model=model,
            calls=1,
            input_tokens=input_tokens,
            output_tokens=output_tokens,
            cost=cost or 0.0,
        )
        self._usage.append(summary)
```

## OpenAIClient

Suporta OpenAI, vLLM, OpenRouter, Vercel, Prime Intellect.

```python
# rlm/clients/openai.py
from openai import OpenAI, AsyncOpenAI
from rlm.clients.base_lm import BaseLM

class OpenAIClient(BaseLM):
    def __init__(
        self, 
        api_key: str | None = None,
        model_name: str = "gpt-4o",
        base_url: str | None = None,
        **kwargs
    ):
        super().__init__(model_name=model_name, **kwargs)
        
        # Auto-detect API key por base_url
        if api_key is None:
            api_key = self._detect_api_key(base_url)
        
        self._client = OpenAI(
            api_key=api_key,
            base_url=base_url,
        )
        self._async_client = AsyncOpenAI(
            api_key=api_key,
            base_url=base_url,
        )
    
    def _detect_api_key(self, base_url):
        """Detecta API key baseado no base_url."""
        if base_url and "openrouter" in base_url:
            return os.getenv("OPENROUTER_API_KEY")
        elif base_url and "vllm" in base_url:
            return os.getenv("VLLM_API_KEY", "dummy")
        return os.getenv("OPENAI_API_KEY")
    
    def completion(self, prompt, model=None):
        model = model or self.model_name
        
        # Normaliza sampling args
        kwargs = self._normalize_sampling_args(self._sampling_args)
        
        response = self._client.chat.completions.create(
            model=model,
            messages=self._to_messages(prompt),
            **kwargs,
        )
        
        # Rastreia usage
        usage = response.usage
        self._track_cost(
            model=model,
            input_tokens=usage.prompt_tokens,
            output_tokens=usage.completion_tokens,
            cost=self._extract_cost(response),
        )
        
        return response.choices[0].message.content
    
    def _normalize_sampling_args(self, args):
        """Renomeia max_tokens para max_completion_tokens."""
        if args is None:
            return {}
        result = args.copy()
        if "max_tokens" in result:
            result["max_completion_tokens"] = result.pop("max_tokens")
        return result
    
    def _extract_cost(self, response):
        """Extrai custo do response (OpenRouter fornece)."""
        try:
            return response.model_extra.get("cost", 0.0)
        except:
            return 0.0
```

## AnthropicClient

```python
# rlm/clients/anthropic.py
from anthropic import Anthropic, AsyncAnthropic
from rlm.clients.base_lm import BaseLM

class AnthropicClient(BaseLM):
    def __init__(self, api_key=None, model_name="claude-3-opus-20240229", **kwargs):
        super().__init__(model_name=model_name, **kwargs)
        self._client = Anthropic(api_key=api_key or os.getenv("ANTHROPIC_API_KEY"))
    
    def completion(self, prompt, model=None):
        model = model or self.model_name
        
        messages, system = self._extract_system(prompt)
        
        response = self._client.messages.create(
            model=model,
            max_tokens=self._sampling_args.get("max_tokens", 4096),
            system=system,
            messages=messages,
        )
        
        self._track_cost(
            model=model,
            input_tokens=response.usage.input_tokens,
            output_tokens=response.usage.output_tokens,
        )
        
        return response.content[0].text
    
    def _extract_system(self, prompt):
        """Anthropic trata system message separadamente."""
        if isinstance(prompt, str):
            return [{"role": "user", "content": prompt}], None
        
        system = None
        messages = []
        for msg in prompt:
            if msg["role"] == "system":
                system = msg["content"]
            else:
                messages.append(msg)
        
        return messages, system
```

## GeminiClient

```python
# rlm/clients/gemini.py
from google import genai
from rlm.clients.base_lm import BaseLM

class GeminiClient(BaseLM):
    def __init__(self, api_key=None, model_name="gemini-2.0-flash", **kwargs):
        super().__init__(model_name=model_name, **kwargs)
        self._client = genai.Client(api_key=api_key or os.getenv("GEMINI_API_KEY"))
    
    def completion(self, prompt, model=None):
        model = model or self.model_name
        
        # Converte formato OpenAI → Gemini
        contents = self._to_gemini_contents(prompt)
        
        response = self._client.models.generate_content(
            model=model,
            contents=contents,
        )
        
        return response.text
    
    def _to_gemini_contents(self, prompt):
        """Converte mensagens OpenAI para formato Gemini."""
        if isinstance(prompt, str):
            return prompt
        
        contents = []
        for msg in prompt:
            role = "user" if msg["role"] in ("user", "system") else "model"
            contents.append({"role": role, "parts": [msg["content"]]})
        return contents
```

## AzureOpenAIClient

```python
# rlm/clients/azure_openai.py
from openai import AzureOpenAI
from rlm.clients.base_lm import BaseLM

class AzureOpenAIClient(BaseLM):
    def __init__(self, api_key=None, model_name="gpt-4", 
                 azure_endpoint=None, api_version="2024-02-01", **kwargs):
        super().__init__(model_name=model_name, **kwargs)
        self._client = AzureOpenAI(
            api_key=api_key or os.getenv("AZURE_OPENAI_API_KEY"),
            azure_endpoint=azure_endpoint or os.getenv("AZURE_OPENAI_ENDPOINT"),
            api_version=api_version,
        )
    
    def completion(self, prompt, model=None):
        # Azure usa deployment name como model
        response = self._client.chat.completions.create(
            model=model or self.model_name,
            messages=self._to_messages(prompt),
        )
        return response.choices[0].message.content
```

## PortkeyClient

```python
# rlm/clients/portkey.py
from portkey import PORTKEY_GATEWAY_URL, createHeaders
from openai import OpenAI
from rlm.clients.base_lm import BaseLM

class PortkeyClient(BaseLM):
    def __init__(self, api_key=None, model_name="gpt-4o", 
                 virtual_key=None, **kwargs):
        super().__init__(model_name=model_name, **kwargs)
        
        self._client = OpenAI(
            api_key=api_key or os.getenv("PORTKEY_API_KEY"),
            base_url=PORTKEY_GATEWAY_URL,
            default_headers=createHeaders(
                virtual_key=virtual_key or os.getenv("PORTKEY_VIRTUAL_KEY")
            ),
        )
    
    def completion(self, prompt, model=None):
        response = self._client.chat.completions.create(
            model=model or self.model_name,
            messages=self._to_messages(prompt),
        )
        return response.choices[0].message.content
```

## Factory

```python
# rlm/clients/__init__.py
def get_client(backend: str, **kwargs) -> BaseLM:
    """Factory para criar clientes LLM."""
    if backend == "openai":
        from rlm.clients.openai import OpenAIClient
        return OpenAIClient(**kwargs)
    elif backend == "anthropic":
        from rlm.clients.anthropic import AnthropicClient
        return AnthropicClient(**kwargs)
    elif backend == "gemini":
        from rlm.clients.gemini import GeminiClient
        return GeminiClient(**kwargs)
    elif backend == "azure_openai":
        from rlm.clients.azure_openai import AzureOpenAIClient
        return AzureOpenAIClient(**kwargs)
    elif backend == "portkey":
        from rlm.clients.portkey import PortkeyClient
        return PortkeyClient(**kwargs)
    else:
        raise ValueError(f"Unknown backend: {backend}")
```

## Uso

```python
# Uso direto
client = get_client("openai", api_key="sk-...", model_name="gpt-4o")
response = client.completion("Olá, como vai?")
print(client.get_usage_summary())

# Via RLM
rlm = RLM(
    backend="openai",
    backend_kwargs={"api_key": "sk-..."},
    # Outro backend para child RLMs
    other_backends=[
        {"backend": "anthropic", "backend_kwargs": {"model_name": "claude-3-haiku"}}
    ],
)
```

## Criando um Client Customizado

```python
from rlm.clients.base_lm import BaseLM
from rlm.core.types import ModelUsageSummary, UsageSummary

class MeuClient(BaseLM):
    def __init__(self, api_key, model_name="meu-modelo", **kwargs):
        super().__init__(model_name=model_name, **kwargs)
        self._client = MeuSDK(api_key=api_key)
    
    def completion(self, prompt, model=None):
        # 1. Converte prompt para formato do SDK
        messages = self._to_messages(prompt)
        
        # 2. Chama o SDK
        response = self._client.generate(messages)
        
        # 3. Rastreia usage
        self._track_cost(
            model=model or self.model_name,
            input_tokens=response.input_tokens,
            output_tokens=response.output_tokens,
        )
        
        # 4. Retorna string
        return response.text
    
    async def acompletion(self, prompt, model=None):
        # Versão async
        pass
    
    def get_usage_summary(self) -> UsageSummary:
        total_calls = sum(u.calls for u in self._usage)
        total_input = sum(u.input_tokens for u in self._usage)
        total_output = sum(u.output_tokens for u in self._usage)
        total_cost = sum(u.cost for u in self._usage)
        
        return UsageSummary(
            total_calls=total_calls,
            total_input_tokens=total_input,
            total_output_tokens=total_output,
            total_cost=total_cost,
            per_model={u.model: u for u in self._usage},
        )
    
    def get_last_usage(self):
        return self._usage[-1] if self._usage else None
    
    def _to_messages(self, prompt):
        if isinstance(prompt, str):
            return [{"role": "user", "content": prompt}]
        return prompt
```
