# E10 — TUI e UX de codificação

> **Fase 6.** A interface desenhada para o fluxo de codificação (tese, §0). Mostra a política em
> ação: `blocked`/`needs_human` com evidência e `override_reason`.
>
> **Decisões:** DF3, DF4. **Depende de:** E09.
> **Gate do épico:** panic-safe + evidência visível + keymap testável.

---

## Decisões de UX herdadas

- **Panic hook que restaura o terminal** (§14, taskdeck; §16.7).
- **Mapa puro de teclas:** `map_key(KeyEvent, Mode) -> Action` + `apply_action(&mut App, Action)`,
  testável por modo (§14).
- **Split live ↔ durable** (§50.3): widget ao vivo que **não entra no chat**; transcrição completa
  sempre em ficheiro; viewer read-only.
- **Resultado de subagente = status + resposta final**, nunca a transcrição (§50.4).
- **Thinking fora do contexto durável e do ecrã**, com toggle em runtime (§50.17).

---

## Tarefas

### E10-T01 ☐ Esqueleto TUI (`crossterm` cru)
- **Entregáveis:** loop de eventos, alt-screen, panic hook, restauro do terminal.
- **Aceite:** matar o processo durante o render deixa o terminal utilizável; teste de panic hook.

### E10-T02 ☐ Keymap puro e modos
- **Entregáveis:** `Action`, `AppMode`, `map_key`, `apply_action`.
- **Aceite:** o keymap é uma função pura testada por modo; nenhuma lógica de UI no handler de
  eventos.

### E10-T03 ☐ Render diferencial e orçamento de render
- **Entregáveis:** diff de frames; throttle; teto de trabalho por frame.
- **Aceite:** benchmark de render por frame dentro do orçamento (E15); sem alocações no hot path
  de render (verificado por lint/bench).

### E10-T04 ☐ Superfície de política: `blocked`/`needs_human`
- **Entregáveis:** apresentação do veredicto com `rule_id`, evidência e o caminho de override
  (challenge-and-response, não rubber-stamp, §33).
- **Aceite:** um `Deny` mostra a regra e a evidência; um override exige resposta a perguntas
  positivas e regista `override_reason` + `overridden_by`.

### E10-T05 ☐ Split live/durable
- **Entregáveis:** painel de observação efémero; transcrições em ficheiro; viewer read-only.
- **Aceite:** o live não entra no transcript do modelo; o durable é reconstruível do log.

### E10-T06 ☐ Checkpoint e estado visíveis
- **Entregáveis:** indicador de fase, checkpoint atual, pendências, próxima ação.
- **Aceite:** o estado mostrado deriva do `State` (fonte única), nunca de variável de UI paralela.

---

## Definition of Done

- [ ] E10-T01…T06 concluídas.
- [ ] Terminal panic-safe; keymap testado; evidência visível.
- [ ] Benchmark de render no orçamento.
- [ ] `cargo xtask check` e job `msrv` verdes.

## Não-objetivos

- Temas, imagens inline, plugins de UI: adiados.
- Desktop/Electron: fora de escopo (§0).
