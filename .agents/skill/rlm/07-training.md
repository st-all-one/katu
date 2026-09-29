# Treinamento de RLMs com Reinforcement Learning

## Visão Geral

O RLM pode ser treinado via Reinforcement Learning usando o framework **verifiers** (Prime Intellect) + **prime-rl**. O treinamento ocorre em `training/`.

```
┌─────────────────────────────────────────────────────────────┐
│                  Pipeline de Treinamento                     │
│                                                              │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐              │
│  │ Dataset  │───►│  RLM     │───►│ Rubric   │───► Reward   │
│  │ (ex:     │    │  Train   │    │ (grader) │              │
│  │  OOLONG) │    │  Env     │    │          │              │
│  └──────────┘    └──────────┘    └──────────┘              │
│       │               │               │                     │
│       ▼               ▼               ▼                     │
│  Context +      Rollout com      Score de                  │
│  Question       iterações        correção                  │
│                  + código                                   │
│                  executado                                  │
└─────────────────────────────────────────────────────────────┘
```

## Arquitetura de Treinamento

```
┌─────────────────────────────────────────────────────────────┐
│                    Training Pipeline                         │
│                                                              │
│  ┌─────────────────────────────────────────────────────┐    │
│  │ prime-rl (RL Training Framework)                     │    │
│  │                                                      │    │
│  │  ┌──────────────┐     ┌──────────────┐              │    │
│  │  │ Train Worker │     │ Infer Worker  │              │    │
│  │  │ (GPU 0-3)    │     │ (GPU 4-7)    │              │    │
│  │  │              │     │              │              │    │
│  │  │ - Forward    │     │ - LLM infer  │              │    │
│  │  │ - Backward   │     │ - RLM env    │              │    │
│  │  │ - Update     │     │ - Rollouts   │              │    │
│  │  └──────────────┘     └──────────────┘              │    │
│  └─────────────────────────────────────────────────────┘    │
│                          │                                   │
│                          ▼                                   │
│  ┌─────────────────────────────────────────────────────┐    │
│  │ RLMTrainEnv (vf.MultiTurnEnv)                        │    │
│  │                                                      │    │
│  │  - setup_state(): Cria SubLLMProxy + SubprocessRepl  │    │
│  │  - get_prompt_messages(): Processa trajectory        │    │
│  │  - has_final_answer(): Condição de parada            │    │
│  │  - cleanup_rlm(): Limpa recursos                     │    │
│  └─────────────────────────────────────────────────────┘    │
│                          │                                   │
│                          ▼                                   │
│  ┌─────────────────────────────────────────────────────┐    │
│  │ Worker (Subprocess REPL)                              │    │
│  │                                                      │    │
│  │  - Executa código em processo separado               │    │
│  │  - Comunica via JSONL stdio                          │    │
│  │  - _llm_query() rota via HTTP proxy                  │    │
│  │  - _exec_with_timeout() via SIGALRM                  │    │
│  └─────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

## RLMTrainEnv

```python
# training/src/rlm_train/env.py
import verifiers as vf

class RLMTrainEnv(vf.MultiTurnEnv):
    """Environment de treinamento que espelha rlm.RLM.completion."""
    
    def __init__(
        self,
        dataset,
        correctness_fn,
        max_iterations=20,
        orchestrator=True,
        bootstrap_code=None,
        user_prologue=None,
    ):
        super().__init__(dataset=dataset)
        self.correctness_fn = correctness_fn
        self.max_iterations = max_iterations
        self.orchestrator = orchestrator
        self.bootstrap_code = bootstrap_code
        self.user_prologue = user_prologue
    
    def setup_state(self, **kwargs):
        """Configura o state para um rollout."""
        # 1. Cria SubLLMProxy (aiohttp server)
        self.proxy = SubLLMProxy()
        self.proxy_address = self.proxy.start()
        
        # 2. Registra o client de inferência
        self.inference_client = kwargs.get("inference_client")
        
        # 3. Cria SubprocessReplBackend
        self.repl_backend = SubprocessReplBackend(
            proxy_address=self.proxy_address,
            model=kwargs.get("model", "qwen3-30b"),
        )
        
        # 4. Carrega contexto
        context = kwargs.get("context", "")
        self.repl_backend.load_context(context)
        
        # 5. Constrói system prompt
        self.system_prompt = self._build_system_prompt()
        
        # 6. Retorna messages iniciais
        return [
            {"role": "system", "content": self.system_prompt},
            {"role": "user", "content": self.user_prologue or context},
        ]
    
    def get_prompt_messages(self, state, **kwargs):
        """Processa trajectory e executa code blocks."""
        messages = state["messages"]
        
        # Pega última resposta do modelo
        last_response = messages[-1]["content"]
        
        # Extrai code blocks
        code_blocks = find_code_blocks(last_response)
        
        if code_blocks:
            # Executa cada code block
            for code in code_blocks:
                result = self.repl_backend.execute(code)
                
                # Adiciona resultado ao histórico
                messages.append({
                    "role": "user",
                    "content": f"REPL Output:\nstdout: {result.stdout}\nstderr: {result.stderr}"
                })
        
        # Verifica answer["ready"]
        if self.repl_backend.is_answer_ready():
            state["final_answer"] = self.repl_backend.get_answer()
        
        return messages
    
    def has_final_answer(self, state, **kwargs):
        """Verifica se há resposta final."""
        return "final_answer" in state
    
    def cleanup_rlm(self, state, **kwargs):
        """Limpa recursos."""
        self.repl_backend.shutdown()
        self.proxy.stop()
```

## Worker (Subprocess REPL)

```python
# training/src/rlm_train/worker.py
import sys
import json
import os
import signal

class Worker:
    """Worker REPL em subprocess separado."""
    
    def __init__(self, proxy_address, model):
        self.proxy_address = proxy_address
        self.model = model
        self.locals = {}
        
        # Namespace seguro (mesmo do LocalREPL)
        self.globals = {
            '__builtins__': _SAFE_BUILTINS,
            'llm_query': self._llm_query,
            'llm_query_batched': self._llm_query_batched,
            'answer': {"content": "", "ready": False},
        }
    
    def _llm_query(self, prompt):
        """Roteamento via HTTP proxy para inferência."""
        import requests
        
        response = requests.post(
            f"http://{self.proxy_address}/llm_query",
            json={"prompt": prompt, "model": self.model},
            timeout=300,
        )
        return response.json()["response"]
    
    def _llm_query_batched(self, prompts):
        """Chamadas paralelas via proxy."""
        import requests
        
        response = requests.post(
            f"http://{self.proxy_address}/llm_query_batched",
            json={"prompts": prompts, "model": self.model},
            timeout=300,
        )
        return response.json()["responses"]
    
    def exec_with_timeout(self, code, timeout=30):
        """Executa código com timeout via SIGALRM."""
        def handler(signum, frame):
            raise TimeoutError("Code execution timed out")
        
        signal.signal(signal.SIGALRM, handler)
        signal.alarm(timeout)
        
        try:
            # Executa código
            exec(code, self.globals, self.locals)
            signal.alarm(0)  # Cancela alarme
        except TimeoutError:
            return "Execution timed out"
    
    def run(self):
        """Loop principal do worker."""
        # Lê comandos do stdin (JSONL)
        for line in sys.stdin:
            command = json.loads(line)
            
            if command["type"] == "exec":
                result = self.exec_with_timeout(command["code"])
                self._send_result(result)
            
            elif command["type"] == "load_context":
                self.globals["context"] = command["context"]
                self._send_result("ok")
            
            elif command["type"] == "bootstrap":
                exec(command["code"], self.globals, self.locals)
                self._send_result("ok")
            
            elif command["type"] == "shutdown":
                break
```

## Rubric (Scoring)

```python
# training/src/rlm_train/rubric.py
class RLMTrainRubric:
    """Rubric para scoring de treinamento."""
    
    def __init__(
        self,
        correctness_fn,
        min_iterations=1,
        min_subcall=0,
        min_reward=0.0,
    ):
        self.correctness_fn = correctness_fn
        self.min_iterations = min_iterations
        self.min_subcall = min_subcall
        self.min_reward = min_reward
    
    def score(self, trajectory):
        """Calcula reward para um trajectory."""
        # 1. Verifica mínimo de iterações
        if len(trajectory.iterations) < self.min_iterations:
            return 0.0
        
        # 2. Verifica mínimo de sub-calls
        subcall_count = sum(
            len(it.code_blocks) for it in trajectory.iterations
        )
        if subcall_count < self.min_subcall:
            return 0.0
        
        # 3. Calcula reward de correção
        reward = self.correctness_fn(
            trajectory.final_answer,
            trajectory.metadata["expected_answer"]
        )
        
        # 4. Verifica mínimo de reward
        if reward < self.min_reward:
            return 0.0
        
        return reward
    
    def get_metrics(self, trajectory):
        """Retorna métricas de monitoramento."""
        return {
            "rlm_iterations": len(trajectory.iterations),
            "rlm_repl_calls": sum(
                len(it.code_blocks) for it in trajectory.iterations
            ),
            "rlm_sub_llm_calls": sum(
                it.code_blocks.count("llm_query") 
                for it in trajectory.iterations
            ),
            "rlm_has_final_answer": trajectory.final_answer is not None,
        }
```

## Exemplo: OOLONG (Long-Context QA)

```python
# training/environments/oolong/env.py
from datasets import load_dataset

def create_oolong_env(split="train"):
    """Cria environment OOLONG para treinamento."""
    
    # 1. Carrega dataset
    dataset = load_dataset("oolongbench/oolong-synth", split=split)
    
    # 2. Função de correção
    def synth_score(prediction, expected):
        """Score para respostas OOLONG."""
        # Suporta: numérico, data, entidade, comparação
        if expected["type"] == "numeric":
            try:
                pred_num = float(prediction.replace(",", ""))
                exp_num = float(expected["value"])
                return 1.0 if abs(pred_num - exp_num) < 0.01 else 0.0
            except:
                return 0.0
        
        elif expected["type"] == "entity":
            return 1.0 if expected["value"].lower() in prediction.lower() else 0.0
        
        elif expected["type"] == "comparison":
            # Compara dois valores
            return 1.0 if prediction.strip() == expected["value"] else 0.0
        
        return 0.0
    
    # 3. Cria environment
    env = RLMTrainEnv(
        dataset=dataset,
        correctness_fn=synth_score,
        max_iterations=20,
        orchestrator=True,
    )
    
    # 4. Cria rubric
    rubric = RLMTrainRubric(
        correctness_fn=synth_score,
        min_iterations=2,
        min_subcall=1,
    )
    
    return env, rubric
```

## Configuração de Treinamento (TOML)

```toml
# training/configs/rlm-qwen3-30b-example.toml

[model]
name = "Qwen3-30B-A3B-Instruct-2507"
lora_rank = 32

[training]
batch_size = 32
max_steps = 200
learning_rate = 1e-5
gpus = 4  # GPUs de treinamento

[inference]
gpus = 4  # GPUs de inferência
model = "Qwen3-30B-A3B-Instruct-2507"
max_concurrent = 8

[environment]
type = "oolong-spam"
max_iterations = 20
orchestrator = true

[eval]
type = "oolong-trec-coarse"
num_examples = 25

[orchestrator_filters]
repetition_filter = true
zero_advantage_filter = true
```

## Métricas de Treinamento

```python
# Métricas coletadas durante treinamento
{
    "rlm_iterations": 12,           # Iterações médias por rollout
    "rlm_repl_calls": 8,            # Chamadas REPL médias
    "rlm_sub_llm_calls": 15,        # Sub-LLM calls médias
    "rlm_has_final_answer": 0.85,   # % com resposta final
    "rlm_below_min_iterations": 0.05,  # % abaixo do mínimo
    "reward_mean": 0.72,            # Reward médio
    "reward_std": 0.15,             # Desvio padrão do reward
    "tokens_per_step": 125000,      # Tokens por step
}
```

## Checklist de Treinamento

- [ ] Dataset configurado (ex: OOLONG)
- [ ] Função de correção implementada
- [ ] Rubric com thresholds apropriados
- [ ] Config TOML com GPUs mapeadas
- [ ] Orchestrator filters habilitados
- [ ] Eval dataset separado
- [ ] Métricas de monitoramento configuradas
