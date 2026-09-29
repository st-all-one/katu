# E11 — (FUTURO) Plugin host e ABI

> **Status: deferido — fora do plano principal.** O plugin host **não** é necessário para controlar
> a IA: o seam de controlo é a **política** (E02), que é **dado**. Isto é maquinaria pesada (ABI
> carimbada, manifests, pontos de extensão) e só se justifica **depois** de o produto geral estar
> estável — a lição do `dsh` (§46.1).
>
> **Decisões:** DF2, DF4, DF6, DF7. **Depende de:** E06 (se um dia retomar).
> **Gate (se retomar):** o plugin **declara** capacidades; o host exige a **conjunção** declarado ∧
> concedido; **erro como política**, sem caminho alternativo.
>
> O **modelo** de capacidades já está no plano principal (E02: `Capability`, `Decision`); o que
> fica deferido é o **runtime** de plugins. Retomar exige decisão registada e consumidor **atual**
> (filtro `00b` §4).

---

## Extensões por código (a ideia "rule = script") — análise

Três níveis, do mais seguro ao mais perigoso:

1. **Regra (dados)** — decisão **pura** sobre factos tipados (E02). Determinística, auditável, com
   exemplo negativo e teste de caminho real. É o **único** nível que pode ser `Enforced`.
2. **Check determinístico** — um comando/processo que produz **evidência** para o gate de
   verificação (E09-T03). *Linter personalizado* cai aqui: contrato estável (exit/artefacto),
   capability-gated, sem estado escondido.
3. **Hook/plugin (efeito colateral)** — reage a `ToolCall`/eventos e faz I/O (*fluxo de criação de
   arquivos*, *envio de log para endpoint*). **Nunca** é uma regra; corre **WASM/out-of-process**,
   com capacidades (incl. `Net`), opt-in, e os efeitos entram no log (`Model-visible ⟺ logged`).

**Lua?** É interpretável e sandboxável (`mlua`/Luau), mas é um **DSL novo** dentro do kernel —
precisamente o risco que [`00-tese`](00-tese-e-escopo.md) §4 rejeita (postmortem `!!js` do `dsh`) —
e um predicado Turing-completo **não** é uma regra pura. Se se quiser linguagem, o encaixe certo é
**Lua/Luau compilado para WASM** dentro deste host de plugins, **nunca** dentro do `evaluate`.

**Rust em runtime?** `rustc` + `cdylib` + `dlopen` exige toolchain, quebra o binário único (G7),
não isola e acopla o MSRV 1.97.0 — é **pior** que Lua. Código arbitrário quer **isolamento** → WASM.

**Regra de ouro:** `evaluate(facts, rules) -> Decision` permanece **total e sem I/O**. Se precisa de
I/O ou de estado, **não é uma regra — é um hook**. Manter essa fronteira preserva o replay, a
auditoria (DF3) e o determinismo (G8). Ver **OA14**.

---

## Princípio e ordem correta

> **Primeiro o kernel de política; o plugin host depois.** Construir um framework de plugins como
> primeiro produto é o erro do `dsh` contra-checklist (§46.1).

No MVP **não há WASM**. A fronteira de capacidades é desenhada **como se fosse**, e imposta a
plugins nativos. O runtime WASM pode esperar (§58, "lapidar").

---

## Modelo (do zed, §55)

```rust
pub enum ExtensionCapability {
    ProcessExec { command: String, args: ArgGlob },   // NÃO "pode executar"
    DownloadFile { host: String, path: PathGlob },
    NpmInstall { package: String },
    FsRead(PathRoot), FsWrite(PathRoot),
}

pub fn grant_exec(&self, cmd: &str, args: &[String]) -> Result<()> {
    self.manifest.allow_exec(cmd, args)?;                     // 1. declarado
    ensure!(self.granted.iter().any(|c| c.matches(cmd, args)), // 2. concedido
            "capability for process:exec {cmd} {args:?} is not granted");
    Ok(())
}
```

Três invariantes: **(a)** o erro é a política (`Err`, sem modo permissivo); **(b)** a capacidade é
um facto estruturado (comando **e** argumentos), não um booleano por plugin; **(c)** o default
pode ser largo, mas é **fechável** — e diz-se sem vergonha que fechar torna muitos plugins
inúteis.

---

## Tarefas

### E11-T01 ☐ Manifesto tipado
- **Entregáveis:** `plugin.toml` com `id`, `name`, `version`, `schema_version`, `provides`,
  `capabilities`; `provides()` derivado **estaticamente**, sem executar o plugin.
- **Aceite:** o host sabe o que o plugin oferece antes de o carregar; manifesto validado por tipo
  com mensagem acionável (`thiserror`, "error that teaches", §38/§57.2).

### E11-T02 ☐ Capacidades declaradas × concedidas
- **Entregáveis:** `CapabilityGranter` com as duas barreiras em série.
- **Aceite:** nomear uma capacidade ausente devolve `Err`; globs (`*`, `**` na última posição)
  testados; capacidade parametrizada (comando + args) testada.

### E11-T03 ☐ Composição estática por feature flags
- **Entregáveis:** plugins de primeira parte como features de compilação; registo pequeno.
- **Aceite:** `xtask check-layers` garante que o núcleo não puxa plugins; nenhum contentor de DI
  geral (`ctx` dinâmico) — decisão explícita (§45).

### E11-T04 ☐ Plugin como ator e instalação atómica
- **Entregáveis:** plugin corre numa tarefa com canal; `Drop` fecha o canal e cancela; instalação
  em staging → `rename` com verificação de tamanho; operações em curso deduplicadas por id com
  guard `on_drop`.
- **Aceite:** instalação concorrente não duplica; estado não é partilhado (evita a cicatriz do
  §49.3); falha de instalação limpa o staging.

### E11-T05 ☐ ABI versionada (preparação)
- **Entregáveis:** `PENDING_CHANGES.md` (mudanças incompatíveis a agrupar num release); geração de
  ABI como cópia integral congelada (`since_vX.Y.Z`); carimbo de versão **no artefacto**, lido
  **antes** de instanciar; tabela pública produto ↔ ABI; ABI nova atrás de canal de staging.
- **Aceite:** uma ABI publicada nunca é editada; a versão vem do artefacto, não do manifesto; uma
  incompatibilidade é recusada com a faixa de versões e mensagem de atualização (§56/§59).

### E11-T06 ☐ Gate de plugins opt-in e licenças
- **Entregáveis:** plugins externos desativados por default; proveniência (nome + módulo);
  conjunto carregado observável; **proibição de plugins omnibus**; política de licenças fechada
  verificada em CI; não empacotar binários de terceiros.
- **Aceite:** `allow_external_plugins = false` recusa com warning; licença inválida falha o CI.

### E11-T07 ☐ Deprecação retirando o ponto de extensão
- **Entregáveis:** regra: quando um ponto de extensão vira protocolo/registo, ele **sai** da ABI
  (§57.5).
- **Aceite:** existe um ADR que nomeia os pontos de extensão atuais e o seu estado
  (ativo/deprecado); nenhum ponto é mantido só por inércia.

---

## Definition of Done

- [ ] E11-T01…T07 concluídas.
- [ ] Nenhum plugin roda com poder total; toda negação é `Err`.
- [ ] ABI versionada e carimbada no artefacto; `PENDING_CHANGES.md` vivo.
- [ ] `cargo xtask check` e job `msrv` verdes.

## Não-objetivos

- WASM obrigatório (§59): entra, se entrar, depois de o modelo estar provado.
- Frota de agentes como requisito: o valor está nos artefatos determinísticos (§34, não copiar).
