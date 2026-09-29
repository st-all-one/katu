# E11 — Plugin host e ABI

> **Fase 7.** Extensibilidade **com poder limitado e explícito** (§0, tese item 3). Só depois de o
> núcleo estar estável.
>
> **Decisões:** DF2, DF4, DF6, DF7. **Depende de:** E06.
> **Gate do épico:** o plugin **declara** capacidades; o host exige a **conjunção** declarado ∧
> concedido; **erro como política**, sem caminho alternativo.

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
